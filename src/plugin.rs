use crate::{
    http::Request,
    model::{self, Choice, Choices, Skin},
};
use pumpkin_plugin_api::{
    Context, Player, Plugin, PluginMetadata, Server,
    command::{Arg, Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{ArgumentType, StringType},
    commands::CommandHandler,
    events::{EventData, EventHandler, EventPriority, player::player_login::PlayerLoginEvent},
    permission::{Permission, PermissionDefault, PermissionLevel},
    register_plugin,
    scheduler::SchedulerExt,
    text::TextComponent,
    wit::pumpkin::plugin::player::{JavaKickOptions, PlayerSkin, SocketTeardownPolicy},
};
use std::{
    collections::{BTreeMap, VecDeque},
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
const DATA: &str = "data/skins.json";
const HELP: &str = "Custom Skins (native Pumpkin)\n/customskins set <Minecraft-name> | restore | status\nOperators: setother <player> <Minecraft-name> | moderate <player> [steve|alex] | unmoderate <player>\n/custom skins ... is an alias. Official skins only; changes require reconnecting.";
struct Cached {
    skin: Option<Skin>,
    expires: Instant,
}
struct Job {
    target: String,
    account: String,
    actor: Option<String>,
    privileged: bool,
    revision: u64,
    restore: bool,
    automatic: bool,
    session: bool,
    request: Request,
}
#[derive(Default)]
struct State {
    choices: Choices,
    cache: BTreeMap<String, Cached>,
    jobs: VecDeque<Job>,
    revisions: BTreeMap<String, u64>,
    cooldown: BTreeMap<String, Instant>,
    task: Option<u32>,
}
#[derive(Clone)]
struct CustomSkins {
    state: Arc<Mutex<State>>,
    spaced: bool,
}
fn op(p: &Player) -> bool {
    matches!(p.get_permission_level(), PermissionLevel::Four)
}
fn key(p: &Player) -> String {
    let id = p.get_id();
    format!("{:016x}{:016x}", id.high, id.low)
}
fn chat(p: &Player, s: &str) {
    p.send_system_message(TextComponent::text(s), false);
}
fn online(server: &Server, name: &str) -> Option<Player> {
    server
        .get_all_players()
        .into_iter()
        .find(|p| p.get_name().eq_ignore_ascii_case(name))
}
fn actor(server: &Server, id: &str) -> Option<Player> {
    server.get_all_players().into_iter().find(|p| key(p) == id)
}
fn refresh(server: &Server, target: &str) {
    if let Some(p) = online(server, target)
        && let Some(java) = p.as_java()
    {
        java.kick(JavaKickOptions {
            reason: TextComponent::text("Your skin changed. Rejoin to apply it."),
            log_to_console: false,
            teardown_policy: SocketTeardownPolicy::Graceful,
        });
    }
}
fn commit(s: &mut State, target: &str, choice: Option<Choice>) -> Result<(), String> {
    let mut next = s.choices.clone();
    if let Some(c) = choice {
        next.insert(target.into(), c);
    } else {
        next.remove(target);
    }
    model::save(Path::new(DATA), &next)?;
    s.choices = next;
    *s.revisions.entry(target.into()).or_default() += 1;
    Ok(())
}
fn cache(s: &mut State, account: String, skin: Option<Skin>, ttl: u64) {
    if s.cache.len() >= 512
        && !s.cache.contains_key(&account)
        && let Some(key) = s.cache.keys().next().cloned()
    {
        s.cache.remove(&key);
    }
    s.cache.insert(
        account,
        Cached {
            skin,
            expires: Instant::now() + Duration::from_secs(ttl),
        },
    );
}
fn queue(
    s: &mut State,
    target: String,
    account: String,
    who: Option<String>,
    privileged: bool,
    restore: bool,
    automatic: bool,
) -> Result<(), String> {
    if s.jobs.len() >= 24 {
        return Err("Skin requests are busy; retry shortly.".into());
    }
    if s.jobs.iter().any(|j| j.target == target) {
        return Err("A skin lookup is already pending; retry shortly.".into());
    }
    let request = Request::start(
        "api.mojang.com",
        &format!("/users/profiles/minecraft/{account}"),
    )?;
    let revision = *s.revisions.entry(target.clone()).or_default();
    s.jobs.push_back(Job {
        target,
        account,
        actor: who,
        privileged,
        revision,
        restore,
        automatic,
        session: false,
        request,
    });
    Ok(())
}
impl CustomSkins {
    fn run(&self, sender: &CommandSender, server: &Server, raw: &str) -> Result<String, String> {
        let mut args: Vec<_> = raw.split_whitespace().collect();
        if self.spaced {
            if args.first() != Some(&"skins") {
                return Ok("Use /custom skins".into());
            }
            args.remove(0);
        }
        if args.is_empty() || args[0] == "help" {
            return Ok(HELP.into());
        }
        let action = args[0];
        let privileged = matches!(action, "setother" | "moderate" | "unmoderate");
        if !matches!(
            action,
            "set" | "restore" | "status" | "setother" | "moderate" | "unmoderate"
        ) {
            return Err(HELP.into());
        }
        let player = sender.as_player();
        if privileged
            && !player.as_ref().map(op).unwrap_or_else(|| {
                sender.has_permission_level(pumpkin_plugin_api::command_wit::PermissionLevel::Four)
            })
        {
            return Err("Only level-4 Pumpkin operators may manage other players' skins.".into());
        }
        let target = if privileged {
            model::name(args.get(1).ok_or("Missing player name")?)?
        } else {
            model::name(
                &player
                    .as_ref()
                    .ok_or("Use an operator command from console")?
                    .get_name(),
            )?
        };
        let mut s = self.state.lock().unwrap();
        if action == "status" {
            if args.len() != 1 {
                return Err("Use /customskins status".into());
            }
            return Ok(match s.choices.get(&target) {
                Some(c) => format!(
                    "Skin: {}",
                    c.moderated
                        .as_ref()
                        .map(|v| format!("moderated to {v}; locked"))
                        .unwrap_or_else(|| c.username.clone().unwrap_or_default())
                ),
                None => "Skin: automatic official lookup, with normal client fallback.".into(),
            });
        }
        let expected = match action {
            "set" => 2,
            "setother" => 3,
            "restore" => 1,
            "unmoderate" => 2,
            "moderate" => args.len().clamp(2, 3),
            _ => 0,
        };
        if args.len() != expected {
            return Err(HELP.into());
        }
        if !privileged {
            let now = Instant::now();
            s.cooldown
                .retain(|_, t| now.duration_since(*t) < Duration::from_secs(15));
            if s.cooldown.contains_key(&target) {
                return Err("Wait 15 seconds between skin changes.".into());
            }
            if s.cooldown.len() >= 512 {
                return Err("Skin requests are busy.".into());
            }
            s.cooldown.insert(target.clone(), now);
        }
        match action {
            "moderate" => {
                let model = args.get(2).copied().unwrap_or("steve");
                let skin = model::default_skin(model)?;
                commit(
                    &mut s,
                    &target,
                    Some(Choice {
                        username: None,
                        skin,
                        moderated: Some(model.into()),
                    }),
                )?;
                drop(s);
                refresh(server, &target);
                Ok(format!("Moderated {target} to {model}; changes locked."))
            }
            "unmoderate" => {
                commit(&mut s, &target, None)?;
                drop(s);
                refresh(server, &target);
                Ok(format!("Restored automatic lookup for {target}."))
            }
            _ => {
                if s.choices
                    .get(&target)
                    .is_some_and(|c| c.moderated.is_some())
                {
                    return Err("An operator must unmoderate this skin first.".into());
                }
                let restore = action == "restore";
                let account = if restore {
                    target.clone()
                } else {
                    model::name(args[expected - 1])?
                };
                queue(
                    &mut s,
                    target,
                    account,
                    player.as_ref().map(key),
                    privileged,
                    restore,
                    false,
                )?;
                Ok(
                    "Looking up official skin; you will be asked to reconnect when it is saved."
                        .into(),
                )
            }
        }
    }
}
impl CommandHandler for CustomSkins {
    fn handle(
        &self,
        sender: CommandSender,
        server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let raw = match args.get_value("settings") {
            Arg::Simple(s) => s,
            _ => String::new(),
        };
        let message = self.run(&sender, &server, &raw).unwrap_or_else(|e| e);
        sender.send_message(TextComponent::text(&message));
        Ok(1)
    }
}
impl EventHandler<PlayerLoginEvent> for CustomSkins {
    fn handle(
        &self,
        _server: Server,
        mut event: EventData<PlayerLoginEvent>,
    ) -> EventData<PlayerLoginEvent> {
        if event.cancelled || event.player.as_java().is_none() {
            return event;
        }
        let Ok(name) = model::name(&event.player.get_name()) else {
            return event;
        };
        let mut s = self.state.lock().unwrap();
        let skin = if let Some(c) = s.choices.get(&name) {
            Some(c.skin.clone())
        } else if let Some(c) = s.cache.get(&name).filter(|c| c.expires > Instant::now()) {
            c.skin.clone()
        } else {
            // Resolve outside the login callback. A first uncached login retries after lookup.
            let result = queue(&mut s, name.clone(), name.clone(), None, false, false, true);
            if result.is_ok() || s.jobs.iter().any(|j| j.target == name && j.automatic) {
                event.cancelled = true;
                event.kick_message = TextComponent::text(
                    "Loading your official skin. Please reconnect in a few seconds.",
                );
            }
            None
        };
        if let Some(skin) = skin {
            event.player.set_skin(&PlayerSkin {
                value: skin.value,
                signature: Some(skin.signature),
            });
        }
        event
    }
}
fn tick(state: &Arc<Mutex<State>>, server: &Server) {
    let mut s = state.lock().unwrap();
    let count = s.jobs.len();
    for _ in 0..count {
        let mut job = s.jobs.pop_front().unwrap();
        let Some(result) = job.request.poll() else {
            s.jobs.push_back(job);
            continue;
        };
        let result = match result {
            Ok(Some(bytes)) if !job.session => match model::profile_id(&bytes).and_then(|id| {
                Request::start(
                    "sessionserver.mojang.com",
                    &format!("/session/minecraft/profile/{id}?unsigned=false"),
                )
            }) {
                Ok(req) => {
                    job.request = req;
                    job.session = true;
                    s.jobs.push_back(job);
                    continue;
                }
                Err(e) => Err(e),
            },
            Ok(Some(bytes)) => model::session_skin(&bytes),
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        };
        let message = match result {
            Err(e) => {
                if job.automatic {
                    cache(&mut s, job.account.clone(), None, 60);
                }
                format!("Skin unchanged: {e}")
            }
            Ok(skin) => {
                cache(
                    &mut s,
                    job.account.clone(),
                    skin.clone(),
                    if skin.is_some() { 21600 } else { 900 },
                );
                if job.automatic {
                    continue;
                }
                let actor_valid = job
                    .actor
                    .as_ref()
                    .is_none_or(|id| actor(server, id).is_some_and(|p| !job.privileged || op(&p)));
                if !actor_valid
                    || s.revisions.get(&job.target).copied().unwrap_or_default() != job.revision
                    || s.choices
                        .get(&job.target)
                        .is_some_and(|c| c.moderated.is_some())
                {
                    continue;
                }
                if skin.is_none() && !job.restore {
                    "No official Minecraft skin found for that username.".into()
                } else {
                    let choice = if job.restore {
                        None
                    } else {
                        skin.map(|skin| Choice {
                            username: Some(job.account.clone()),
                            skin,
                            moderated: None,
                        })
                    };
                    match commit(&mut s, &job.target, choice) {
                        Ok(()) => {
                            refresh(server, &job.target);
                            format!("Skin saved for {}. Reconnect to apply it.", job.target)
                        }
                        Err(e) => format!("Skin unchanged: {e}"),
                    }
                }
            }
        };
        if let Some(id) = job.actor {
            if let Some(p) = actor(server, &id) {
                chat(&p, &message);
            }
        } else if !job.automatic {
            println!("{message}");
        }
    }
    // Retain generation counters only while a lookup can still reference them.
    let pending: Vec<_> = s.jobs.iter().map(|j| j.target.clone()).collect();
    s.revisions.retain(|k, _| pending.contains(k));
}
impl Plugin for CustomSkins {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(State::default())),
            spaced: false,
        }
    }
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "CustomSkins".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            authors: vec!["KYURS".into()],
            description: "Native Pumpkin official skins and moderation; no proxy required".into(),
            dependencies: vec![],
            permissions: vec![
                "fs.read.data".into(),
                "fs.write.data".into(),
                "http.outbound".into(),
            ],
        }
    }
    fn on_unload(&self, _context: Context) -> pumpkin_plugin_api::Result<()> {
        let mut s = self.state.lock().unwrap();
        if let Some(task) = s.task.take() {
            pumpkin_plugin_api::wit::pumpkin::plugin::scheduler::cancel_task(task);
        }
        s.jobs.clear();
        Ok(())
    }
    fn on_load(&self, context: Context) -> pumpkin_plugin_api::Result<()> {
        self.state.lock().unwrap().choices = model::load(Path::new(DATA))?;
        context.register_permission(&Permission {
            node: "CustomSkins:command".into(),
            description: "Skin commands (moderation additionally requires operator level 4)".into(),
            default: PermissionDefault::Allow,
            children: vec![],
        })?;
        for (name, spaced) in [("customskins", false), ("custom", true)] {
            let handler = Self {
                state: self.state.clone(),
                spaced,
            };
            context.register_command(
                Command::new(&[name.into()], "Official Minecraft skins")
                    .execute(handler.clone())
                    .then(
                        CommandNode::argument(
                            "settings",
                            &ArgumentType::String(StringType::Greedy),
                        )
                        .execute(handler),
                    ),
                "CustomSkins:command",
            );
        }
        context.register_event_handler::<PlayerLoginEvent, _>(
            self.clone(),
            EventPriority::Highest,
            true,
        )?;
        let state = self.state.clone();
        let task = context.schedule_repeating_task(1, 1, move |server| tick(&state, &server));
        self.state.lock().unwrap().task = Some(task);
        Ok(())
    }
}
register_plugin!(CustomSkins);
