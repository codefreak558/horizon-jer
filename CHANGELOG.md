# Changelog

## 0.2.1

- Needs JEI for Horizon 0.6.x for the pages inside JEI (`jei = "^0.6"`); CI and the release check against
  JEI v0.6.1.
- Release files carry the version: `jer-0.2.1.hmod` and `jer-0.2.1.hmod.sha256`.

## 0.2.0

- **Pages inside JEI.** With [JEI for Horizon](https://github.com/codefreak558/horizon-jei) 0.5 or newer
  installed, every page type is also a tab of JEI's recipe screen: R over an item lists where it comes from
  (rotten flesh shows the zombie's drops, a diamond its ore and the mobs and chests that give it), U lists
  what it is used for (an emerald shows the villager trades that take it). Chances, counts and what a drop
  depends on show in the tooltips. The dependency is optional; the window (J) works with or without JEI.
- CI and the release build, lint and test the mod with JEI installed too (`tools/check-with-jei.sh`).

## 0.1.0

- First port of Just Enough Resources: mob drops (with a 3D preview and spawn biomes), chest and archaeology
  loot, plant drops, ore heights per dimension, villager trades and enchantments, in a window of its own (J).
  Look an item up with J over it, in the window or in any inventory.
- Read at runtime from the world's own loot tables, features and trades, data packs included; the server half
  sends them to clients that ask. No game data ships with the mod.
