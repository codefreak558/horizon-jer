# Just Enough Resources for Horizon

Where things come from, for [Horizon](https://playhorizon.dev): what mobs drop, what chests hold, what plants
give, where ores generate, what villagers trade and which enchantments an item can get. An unofficial port of
[Just Enough Resources](https://github.com/way2muchnoise/JustEnoughResources) by way2muchnoise.

Press **J** to open the window. Over an item in your inventory or a chest, press **J** to see every page about
that item (what drops it, where it generates, who trades it). In the window:

- click a tab to switch pages: mob drops, chest loot, plant drops, world generation, villager trades,
  enchantments;
- click an item (or press J over it) to look it up, **Backspace** or **Back** to go back;
- the arrows, the mouse wheel or the arrow keys turn pages;
- hover an item for its chance, count and what it depends on (a player kill, Looting, Fortune, Silk Touch).

> **Status: 0.2.0, not yet tested in game.** It builds against the pinned SDK and passes its unit tests and
> the registry's package checks. Whether the server answers the biome, placed-feature and trade-set registry
> reads (World Generation and Villager Trades depend on them) is still to be confirmed in game.

## What it shows

| Page | From | Status |
|---|---|---|
| Mob drops | every `entities/<mob>` loot table, with a 3D preview and the biomes it spawns in | ported |
| Chest loot | every `chests/*` and `archaeology/*` loot table | ported |
| Plant drops | crops (`minecraft:crops` tag), grass, ferns, nether wart, cocoa, berries, melon, pumpkin | ported |
| World generation | ore heights per dimension, from the world's placed and configured features, and what mining the ore gives | ported |
| Villager trades | every trade set and trade (26.3's data-driven trades), by profession and level, the wandering trader included | ported |
| Enchantments | the enchantments an enchanting table can put on an item, their levels and the power each needs | ported (no per-level chances) |
| Mob experience, spawn light levels, enchantment chances | | missing: see [docs/missing-hooks.md](docs/missing-hooks.md) |
| Pages inside JEI | one tab per page type in JEI for Horizon 0.5's recipe screen (R / U), when JEI is installed | ported |
| Custom `world-gen.json`, the JER API for other mods | | not ported |

### With JEI

If [JEI for Horizon](https://github.com/codefreak558/horizon-jei) 0.5 or newer is installed, JER also gives it its
pages: a tab per page type in JEI's recipe screen. R over an item lists where it comes from (rotten flesh shows
the zombie's drops, a diamond its ore and the mobs and chests that give it) and U lists what it is used for (an
emerald shows the villager trades that take it). The window (J) works with or without JEI. The dependency is
optional (`[dependencies]` in `horizon-mod.toml`).

Chances are worked out from the tables themselves, the way JER does it. They're exact for the usual shapes
(weighted entries, rolls, random chances, counts). Where a drop depends on something the page can't know (a
mob's variant, the weather), it is shown as possible and marked "Depends on conditions not shown".

Everything is read at runtime from your own game and data packs, data-pack and other mods' changes included.
The mod ships no game data.

## Servers

Loot tables, world generation and trades exist only on the server, so the mod has a server half. In single
player and on a LAN world you host, it just works. On a dedicated server, the server must run the mod too.
Without it, only the enchantments page shows. The channel is optional, so the mod never stops you joining a
server.

## Settings

Options → Mods → Just Enough Resources: turn each tab on or off, and "Developer details" (item ids in the
tooltips). The J key can be changed on the same page.

## Build and install

You need the Horizon launcher (it brings its own Rust).

```sh
git clone https://github.com/codefreak558/horizon-jer
cd horizon-jer
tools/fetch-sdk.sh          # the pinned SDK's API crate, checked by SHA-256, into .horizon/
horizon-launcher native-build . --trust-native-code
```

Native mods aren't sandboxed: build only code you trust.

For development:

```sh
cargo test                                   # the loot evaluator, ore heights, trades, pages, views
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

CI runs the same checks, then `cargo horizon-mod build` and `cargo horizon-mod check` on the package.

## Credits and licence

Just Enough Resources is ©2016 way2muchnoise. This port is a reimplementation in Rust, not a translation of
its code, and ships none of its assets. It is distributed under JER's own "Don't Be a Jerk" non-commercial
licence: see [LICENSE](LICENSE). The loot-condition evaluator and number helpers come from
[JEI for Horizon](https://github.com/codefreak558/horizon-jei) (MIT). Details are in [NOTICE](NOTICE).

Not affiliated with way2muchnoise, Mojang or Microsoft.
