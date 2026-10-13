package customskins;

import com.google.gson.*;
import com.google.inject.Inject;
import com.velocitypowered.api.command.*;
import com.velocitypowered.api.event.*;
import com.velocitypowered.api.event.connection.PreLoginEvent;
import com.velocitypowered.api.event.player.GameProfileRequestEvent;
import com.velocitypowered.api.event.proxy.*;
import com.velocitypowered.api.plugin.annotation.DataDirectory;
import com.velocitypowered.api.proxy.*;
import com.velocitypowered.api.util.GameProfile;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.format.NamedTextColor;
import net.lax1dude.eaglercraft.backend.server.api.velocity.EaglerXServerAPI;
import net.lax1dude.eaglercraft.backend.server.api.velocity.event.EaglercraftRegisterSkinEvent;
import net.lax1dude.eaglercraft.backend.server.api.skins.EnumPresetSkins;
import org.slf4j.Logger;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.*;

public final class CustomSkins {
    private final ProxyServer proxy;
    private final Logger logger;
    private final Path directory;
    private SkinData data;
    private final ThreadPoolExecutor lookups=pool(2,24,"skin-lookup");
    private final ThreadPoolExecutor changes=pool(1,12,"skin-change");
    private final Map<UUID,Long> cooldown=new ConcurrentHashMap<>();
    private static final String HELP="Custom Skins\n/customskins set <Minecraft-name> — change your skin\n/customskins restore — use your own official skin\n/customskins status\nOperators: /customskins setother <player> <Minecraft-name>\n/customskins moderate <player> [steve|alex]\n/customskins unmoderate <player>\n/custom skins ... works too. Image URLs are not enabled. Skin changes require reconnecting.";

    @Inject public CustomSkins(ProxyServer proxy,Logger logger,@DataDirectory Path directory) {
        this.proxy=proxy;this.logger=logger;this.directory=directory;
    }
    private static ThreadPoolExecutor pool(int threads,int queued,String name) {
        return new ThreadPoolExecutor(threads,threads,30,TimeUnit.SECONDS,new ArrayBlockingQueue<>(queued),r->{Thread t=new Thread(r,name);t.setDaemon(true);return t;},new ThreadPoolExecutor.AbortPolicy());
    }
    @Subscribe public void initialize(ProxyInitializeEvent event) throws Exception {
        data=new SkinData(directory);
        proxy.getCommandManager().register(proxy.getCommandManager().metaBuilder("customskins").plugin(this).build(),new SkinCommand(false));
        proxy.getCommandManager().register(proxy.getCommandManager().metaBuilder("custom").plugin(this).build(),new SkinCommand(true));
        logger.info("Custom Skins ready: official Minecraft profiles, /customskins, persistent moderation");
    }
    static List<GameProfile.Property> properties(List<GameProfile.Property> original,SkinData.Skin skin) {
        List<GameProfile.Property> result=new ArrayList<>();
        for(var property:original)if(!property.getName().equals("textures"))result.add(property);
        if(skin!=null)result.add(new GameProfile.Property("textures",skin.value(),skin.signature()));
        return result;
    }
    @Subscribe(priority=-20000)
    public EventTask profile(GameProfileRequestEvent event) {
        if(data==null)return null;
        return EventTask.withContinuation(cont->{
            try {lookups.execute(()->{
                try {
                    String name=event.getUsername();
                    SkinData.Choice choice=data.choice(name);
                    SkinData.Skin skin;
                    if(choice!=null)skin=choice.skin();
                    else {
                        try {skin=data.official(name);}
                        catch(Exception e){skin=data.cached(name);logger.warn("Official skin lookup unavailable for {}: {}",name,e.getClass().getSimpleName());}
                    }
                    // Moderation can change while the network request is in flight.
                    choice=data.choice(name);if(choice!=null)skin=choice.skin();
                    if(skin!=null)event.setGameProfile(event.getGameProfile().withProperties(properties(event.getGameProfile().getProperties(),skin)));
                } catch(RuntimeException e){logger.warn("Using normal skin fallback for {}",event.getUsername());}
                finally {cont.resume();}
            });} catch(RejectedExecutionException e){
                // A full lookup queue must never bypass a saved moderation decision.
                SkinData.Choice choice=data.choice(event.getUsername());
                if(choice!=null)event.setGameProfile(event.getGameProfile().withProperties(properties(event.getGameProfile().getProperties(),choice.skin())));
                cont.resume();
            }
        });
    }
    @Subscribe(priority=-20000)
    public void eaglerSkin(EaglercraftRegisterSkinEvent event) {
        if(data==null)return;
        String name=event.getLoginConnection().getUsername();
        SkinData.Choice choice=data.choice(name);
        if(choice!=null&&choice.moderated()!=null) {
            var preset=choice.moderated().equals("alex")?EnumPresetSkins.DEFAULT_ALEX:EnumPresetSkins.DEFAULT_STEVE;
            event.forceSkinEagler(EaglerXServerAPI.instance().getSkinService().getSkinLoader().loadPresetSkin(preset));
        } else {
            SkinData.Skin skin=choice==null?data.cached(name):choice.skin();
            if(skin!=null)event.forceSkinFromVanillaTexturesProperty(skin.value());
            // No official skin: leave the client's normal Eaglercraft fallback intact.
        }
    }
    @Subscribe public void guard(PreLoginEvent event){if(data==null)event.setResult(PreLoginEvent.PreLoginComponentResult.denied(Component.text("Custom Skins is unavailable; please try again after the server configuration is repaired.")));}
    @Subscribe public void shutdown(ProxyShutdownEvent event){lookups.shutdownNow();changes.shutdownNow();}
    private static void tell(CommandSource source,String message){source.sendMessage(Component.text(message,NamedTextColor.GOLD));}
    private boolean admin(CommandSource source) {
        if(source instanceof ConsoleCommandSource)return true;
        if(!(source instanceof Player player))return false;
        // Read Pumpkin's authoritative operator UUIDs; never trust client flags or names.
        try {
            JsonArray ops=JsonParser.parseString(Files.readString(Path.of("../data/ops.json"))).getAsJsonArray();
            return isOperator(ops,player.getUniqueId());
        } catch(Exception e){return false;}
    }
    static boolean isOperator(JsonArray ops,UUID id) {
        for(JsonElement element:ops){JsonObject op=element.getAsJsonObject();if(id.toString().equals(op.get("uuid").getAsString())&&op.get("level").getAsInt()>=4)return true;}
        return false;
    }
    private void changed(String name,String message) {
        proxy.getAllPlayers().stream().filter(p->p.getUsername().equalsIgnoreCase(name)).forEach(p->{
            SkinData.Choice choice=data.choice(name);
            if(choice!=null)p.setGameProfileProperties(properties(p.getGameProfileProperties(),choice.skin()));
            // A clean reconnect refreshes the world entity and own skin on every supported Java protocol.
            // It also removes moderated skins immediately, including on older Eaglercraft clients.
            p.disconnect(Component.text(message+" Rejoin the server to continue.",NamedTextColor.GOLD));
        });
    }
    private final class SkinCommand implements SimpleCommand {
        private final boolean spaced;
        SkinCommand(boolean spaced){this.spaced=spaced;}
        @Override public void execute(Invocation invocation) {
            String[] args=invocation.arguments();
            CommandSource sender=invocation.source();
            if(spaced){if(args.length==0||!args[0].equalsIgnoreCase("skins")){tell(sender,"Use /custom skins");return;}args=Arrays.copyOfRange(args,1,args.length);}
            if(args.length==0||args[0].equalsIgnoreCase("help")){tell(sender,HELP);return;}
            final String[] request=args;
            String action=args[0].toLowerCase(Locale.ROOT);
            if(!List.of("set","restore","status","setother","moderate","unmoderate").contains(action)){tell(sender,HELP);return;}
            boolean privileged=List.of("setother","moderate","unmoderate").contains(action);
            if(privileged&&!admin(sender)){tell(sender,"Only a Pumpkin level-4 operator can change or moderate other players' skins.");return;}
            if(!privileged&&!(sender instanceof Player)){tell(sender,"Use setother, moderate or unmoderate from the console.");return;}
            if(sender instanceof Player player&&!action.equals("status")&&!privileged){
                long now=System.currentTimeMillis();cooldown.entrySet().removeIf(e->now-e.getValue()>=15_000);Long last=cooldown.putIfAbsent(player.getUniqueId(),now);
                if(last!=null&&now-last<15_000){tell(sender,"Wait 15 seconds between skin changes.");return;}cooldown.put(player.getUniqueId(),now);
            }
            try {changes.execute(()->{
                try {run(sender,action,request,privileged);}
                catch(Exception e){tell(sender,"Skin unchanged: "+safeError(e));}
            });} catch(RejectedExecutionException e){tell(sender,"Skin requests are busy. Try again shortly.");}
        }
        @Override public List<String> suggest(Invocation invocation) {
            String[] args=invocation.arguments();
            if(spaced&&args.length<=1)return List.of("skins");
            if(spaced)args=Arrays.copyOfRange(args,1,args.length);
            if(args.length<=1){List<String> choices=new ArrayList<>(List.of("set","restore","status","help"));if(admin(invocation.source()))choices.addAll(List.of("setother","moderate","unmoderate"));String prefix=args.length==0?"":args[0].toLowerCase(Locale.ROOT);return choices.stream().filter(s->s.startsWith(prefix)).toList();}
            if(args.length==2&&List.of("setother","moderate","unmoderate").contains(args[0])&&admin(invocation.source())){String prefix=args[1].toLowerCase(Locale.ROOT);return proxy.getAllPlayers().stream().map(Player::getUsername).filter(n->n.toLowerCase(Locale.ROOT).startsWith(prefix)).toList();}
            if(args.length==3&&args[0].equals("moderate"))return List.of("steve","alex");
            return List.of();
        }
    }
    private void run(CommandSource sender,String action,String[] args,boolean privileged) throws Exception {
        if(sender instanceof Player p&&!p.isActive())return;
        if(privileged&&!admin(sender))throw new IllegalArgumentException("Operator permission was removed");
        String target;
        if(privileged){if(args.length<2)throw new IllegalArgumentException("Missing player name");target=args[1];}
        else target=((Player)sender).getUsername();
        SkinData.name(target);
        SkinData.Choice previous=data.choice(target);
        switch(action) {
            case "status" -> {if(args.length!=1)throw new IllegalArgumentException("Use /customskins status");tell(sender,previous==null?"Skin: automatic official Minecraft lookup; Eaglercraft fallback if unavailable.":previous.moderated()!=null?"Skin moderated: "+previous.moderated()+". An operator must unlock skin changes.":"Official skin: "+previous.username());}
            case "moderate" -> {
                if(args.length>3)throw new IllegalArgumentException("Use /customskins moderate <player> [steve|alex]");
                String model=args.length==3?args[2].toLowerCase(Locale.ROOT):"steve";
                SkinData.Skin skin=data.defaultSkin(model);
                data.put(target,new SkinData.Choice(null,skin,model));
                tell(sender,"Moderated "+target+" to "+model+". Skin changes are locked until /customskins unmoderate "+target);
                changed(target,"Your skin was reset to "+model+" by an operator.");
                logger.info("{} moderated {} to {}",actor(sender),target,model);
            }
            case "unmoderate" -> {
                if(args.length!=2)throw new IllegalArgumentException("Use /customskins unmoderate <player>");
                data.put(target,null);tell(sender,"Skin moderation removed for "+target+"; automatic official lookup restored.");
                changed(target,"Your skin moderation was removed.");
                logger.info("{} unmoderated {}",actor(sender),target);
            }
            case "set","setother","restore" -> {
                if(previous!=null&&previous.moderated()!=null)throw new IllegalArgumentException("Skin is moderated; an operator must unmoderate it first");
                int expected=action.equals("setother")?3:action.equals("set")?2:1;
                if(args.length!=expected)throw new IllegalArgumentException("Use /customskins "+action+(action.equals("setother")?" <player> <Minecraft-name>":action.equals("set")?" <Minecraft-name>":""));
                String account=action.equals("restore")?target:args[expected-1];SkinData.name(account);
                tell(sender,"Looking up official Minecraft skin for "+account+"...");
                SkinData.Skin skin=data.official(account);
                if(skin==null&&!action.equals("restore"))throw new IllegalArgumentException("No official Minecraft skin found for that username");
                if(action.equals("restore"))data.put(target,null);else data.put(target,new SkinData.Choice(account,skin,null));
                tell(sender,"Skin saved for "+target+". Reconnect to load it.");
                changed(target,skin==null?"Your normal client skin fallback was restored.":"Your skin is now "+account+"'s official Minecraft skin.");
                logger.info("{} changed {} skin to {}",actor(sender),target,account);
            }
        }
    }
    private static String actor(CommandSource sender){return sender instanceof Player p?p.getUsername():"console";}
    private static String safeError(Exception e){return e instanceof IllegalArgumentException||e instanceof java.io.IOException?e.getMessage():"Official skin lookup failed; try again later";}
}
