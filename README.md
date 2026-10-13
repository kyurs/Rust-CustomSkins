# Custom Skins

Runs on a Velocity proxy in front of Pumpkin. It automatically looks up each joining player's Minecraft username through Mojang's official profile and session APIs, obtains the signed textures property, and forwards it to Pumpkin. No client mod is required.

The current scope is **official Minecraft skins only**. Arbitrary image URLs are rejected with an explanation; no MineSkin account, key, or third-party signing service is used.

## Commands

Both `/customskins ...` and `/custom skins ...` work.

| Command | Access | Result |
|---|---|---|
| `/customskins` | Everyone | Help |
| `/customskins status` | Everyone | Your current selection or moderation state |
| `/customskins set <Minecraft-name>` | Everyone | Use that account's official skin |
| `/customskins restore` | Everyone | Restore your own official skin, or normal client fallback if no official skin exists |
| `/customskins setother <player> <Minecraft-name>` | Level-4 operators | Change another player's skin; target may be offline |
| `/customskins moderate <player> [steve\|alex]` | Level-4 operators | Force a default skin and lock skin changes; defaults to Steve |
| `/customskins unmoderate <player>` | Level-4 operators | Remove the lock and restore automatic lookup |

Successful changes disconnect the affected online player with a message asking them to rejoin. A clean reconnect refreshes their own model and everyone's view across the different Java versions and Eaglercraft. Moderation also immediately removes the old skin from the world. These commands do not change the skin on the player's Microsoft/Minecraft account.

Moderation persists through reconnects and restarts. Names are normalized to lowercase so changing capitalization cannot evade a lock. Permissions come from the exact operator UUIDs with level 4 in Pumpkin's `data/ops.json`, not display names or client messages. A moderated player must be explicitly unmoderated before selecting another skin.

## Eaglercraft

The plugin uses the installed EaglerXServer API to override skins from the signed official texture. If an official profile is missing, normal Eaglercraft skin handling remains active. EaglerXServer's texture downloader falls back to the original client skin on download failure. Moderation uses local built-in Steve/Alex presets, so it does not need a network download on Eaglercraft.

Java moderation uses bundled signed texture properties captured from Mojang's `MHF_Steve` and `MHF_Alex` profiles on 2026-10-12. Their texture images were inspected. The captured properties are pinned rather than refetched whenever an operator moderates someone.

## Resource and failure handling

- Runs inside the existing proxy; no extra service, server, port, or increased resource cap.
- Two lookup workers and one serialized change worker, with bounded queues.
- Official lookups cached for 6 hours; nonexistent profiles cached for 15 minutes; up to 512 cache entries.
- 15-second cooldown for self-service changes; moderation is not delayed by that cooldown.
- Fixed official HTTPS API endpoints, username validation, request timeouts, signed-texture checks, and no arbitrary URL downloads.
- State is saved by atomic file replacement before a change is announced. Failed lookups do not overwrite the selected skin.
- Corrupt existing state is preserved; the plugin blocks new logins if initialization fails, avoiding a silent bypass of moderation. Repair the state from a backup and restart the proxy.
- The server's existing offline-mode identity model remains unchanged; skins do not authenticate Minecraft accounts.


## Related KYURS plugins

These projects are independently installed; none requires the other three.

- [Custom Skins](https://github.com/kyurs/Rust-CustomSkins): official skins and moderation on Velocity.
- [Pumpkin Voice](https://github.com/kyurs/Rust-PumpkinVoice): native Pumpkin proximity voice.
- [Rust Tab List](https://github.com/kyurs/Rust-TabList): native ranks, chat and metrics with a Velocity bridge.
- [Rust WorldEdit](https://github.com/kyurs/Rust-WorldEdit): native operator-only core editing.

## Build and install

This is a **Java Velocity plugin**, despite the repository prefix. Use JDK 25, Velocity 4.2.0 and EaglerXServer 1.1.1. EaglerXServer is a required runtime dependency. Dependencies are compile-only and not bundled.

```sh
git clone https://github.com/kyurs/Rust-CustomSkins.git
cd Rust-CustomSkins
export JAVA_HOME=/path/to/jdk-25
export VELOCITY_JAR=/path/to/velocity.jar
export EAGLER_JAR=/path/to/EaglerXServer.jar
./scripts/build.sh
```

The script runs assertion tests and builds `dist/custom-skins.jar`. Downloadable binaries are also available in Releases.

Stop Velocity before replacing the JAR in its `plugins/` directory. Start Velocity with its working directory set to `server/proxy` in this layout:

```text
server/
  data/ops.json
  proxy/
    plugins/custom-skins.jar
    plugins/EaglerXServer.jar
    plugins/custom-skins/skins.json
```

The operator file is read from `../data/ops.json`; administrative commands require exact UUID matches with level 4. The pinned Pumpkin format uses `bypasses_player_limit`, not vanilla's camelCase spelling. Preserve your existing valid operator file. State is created automatically. Back up `skins.json` before upgrades. To uninstall, stop Velocity, remove the JAR, and restart; saved moderation is no longer enforced.

For integration testing, install `minecraft-protocol` into a separate test tooling directory and run:

```sh
PROBE_HOST=your-test-server NODE_PATH=/path/to/test-tools/node_modules node tests/live-probe.cjs
```

Use a disposable authorized server. The probe requires temporary level-4 access for `SkinAdminProbe`; revoke it afterward. Rendered Java/Eaglercraft appearance needs an in-game check. Image-URL skins are not supported.

License: MIT; see [LICENSE](LICENSE).
