# Rust Custom Skins — native Pumpkin

Version **2.0.0** is a Rust/WASM plugin installed directly on **Pumpkin**. **Neither Velocity nor EaglerXServer is required.** Version 1.x was a legacy Velocity plugin and is not the native build.

It fetches signed textures from Mojang's official APIs, lets players select another Minecraft account's skin, and lets level-4 Pumpkin operators moderate skins to Steve or Alex. Selections and moderation persist across restarts. No client mod is needed for Java players.

## Install

Requires a Pumpkin host compatible with SDK commit `204a94ed895f041a845630d5a23c094705a0704e` (plugin API v0.1). Check the host ABI before upgrading. Download `custom_skins.wasm` from [Releases](https://github.com/kyurs/Rust-CustomSkins/releases), stop Pumpkin gracefully, and put it in Pumpkin's `plugins/` directory.

Add this to `pumpkin.toml`:

```toml
[plugins.overrides.CustomSkins]
enabled = true
allow_unsigned = true
max_memory_mb = 64
allowed_permissions = ["fs.read.data", "fs.write.data", "http.outbound"]
```

Start Pumpkin. Permit outbound HTTPS to `api.mojang.com` and `sessionserver.mojang.com`. No extra inbound port, proxy, JVM, signing service or Eaglercraft plugin is needed. Skin state is stored at `plugins/data/CustomSkins/skins.json`.

Skin textures are applied during Java login, before the player joins the world. A first uncached login may ask the player to reconnect after a few seconds while the official lookup completes. Later logins use a bounded cache. API failures fall back to normal client handling after a short retry window. Saved skin changes disconnect the affected online Java player so the next login refreshes their skin.

## Commands

Both `/customskins ...` and `/custom skins ...` work.

| Command | Access | Action |
| --- | --- | --- |
| `/customskins` | Everyone | Help |
| `/customskins status` | Everyone | Current selection or moderation |
| `/customskins set <Minecraft-name>` | Everyone | Select that account's official skin |
| `/customskins restore` | Everyone | Restore automatic lookup for your account |
| `/customskins setother <player> <Minecraft-name>` | Level-4 operators / console | Set another player's skin |
| `/customskins moderate <player> [steve\|alex]` | Level-4 operators / console | Force a default skin and lock changes |
| `/customskins unmoderate <player>` | Level-4 operators / console | Unlock changes and restore automatic lookup |

Administrative access uses Pumpkin's current operator permissions. Async changes recheck the requesting operator and discard results superseded by moderation or another saved change. Assigning a cosmetic rank does not grant operator status.

## Limits

- Official Minecraft account skins only; arbitrary image URLs are not implemented.
- Java skin textures only. This native plugin does not implement the separate Bedrock or Eaglercraft custom-skin protocol. Those clients retain their normal fallback; no Eaglercraft dependency is installed or required.
- At most 24 in-flight lookups, 512 cached results, 4,096 saved selections and a 15-second self-service cooldown.
- HTTPS requests are polled in bounded chunks without blocking the tick callback. Responses are capped at 64 KiB, with request deadlines and fixed official endpoints.
- Atomic state replacement; corrupt existing files are preserved and prevent plugin startup. Fix the error before allowing gameplay if moderation is required.
- Offline-mode usernames are not authentication. Skin customization does not prove ownership of an account.

## Upgrade from 1.x

1. Back up existing skin state and configuration; stop Pumpkin and any proxy running the legacy plugin.
2. Remove `custom-skins.jar` from the proxy's plugin directory. Do not run both implementations together.
3. Copy the legacy `proxy/plugins/custom-skins/skins.json` to `plugins/data/CustomSkins/skins.json`. The native plugin reads the same selection/moderation format. Preserve file ownership for the Pumpkin process.
4. Install the native WASM and configuration above, then restart. The old proxy may continue serving unrelated features, but Custom Skins no longer uses it.

Back up the world and plugin data before upgrades. To uninstall, stop Pumpkin, remove the WASM and its override, and restart. Keep skin state if you plan to reinstall; moderation is not enforced while the plugin is absent. Use a server restart rather than hot reload with the pinned host.

## Build and test

Requires Rust and the WASI Preview 2 target. No Java or EaglerXServer JAR is needed to build.

```sh
git clone https://github.com/kyurs/Rust-CustomSkins.git
cd Rust-CustomSkins
rustup target add wasm32-wasip2
./build.sh
```

Output: `dist/custom_skins.wasm`. The script runs native model tests, WASM Clippy and the release build. `Cargo.lock` pins dependencies. Integration probes under `tests/` require an explicitly authorized test server; never use real player identities for test operators. Install `minecraft-protocol` in a separate test tooling directory, grant the disposable `SkinAdminProbe` account temporary level-4 operator permission, and run:

```sh
PROBE_HOST=your-test-server NODE_PATH=/path/to/test-tools/node_modules node tests/live-probe.cjs
```

The default probe uses Java 1.12.2 through a protocol-compatible endpoint; use `PROBE_VERSION` and `PROBE_PORT` for another client version or port. Revoke the test operator afterward. This probe does not require the legacy skin proxy plugin.

Validation: four model tests, WASM Clippy, and release build passed. On the deployed Pumpkin host, protocol tests passed for official lookup, spaced commands, skin selection/reconnect persistence, non-operator denial, operator moderation and locks, and unmoderation. The old proxy skin JAR was absent. Rendered client appearance, direct Java protocol 777 connectivity, Bedrock and Eaglercraft custom-skin rendering were not tested.

## License and related projects

MIT: use, modify, redistribute and sell the code while retaining its copyright and license notice. See [LICENSE](LICENSE). Third-party dependencies and artwork retain their own licenses.

Optional, independently installed companion projects: [Pumpkin Voice](https://github.com/kyurs/Rust-PumpkinVoice), [Rust Tab List](https://github.com/kyurs/Rust-TabList), [Rust WorldEdit](https://github.com/kyurs/Rust-WorldEdit).
