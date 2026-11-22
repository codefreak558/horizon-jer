# Missing hooks

What the Horizon mod SDK (pinned build `2b77287`) lacks for full parity with Just Enough Resources. Each entry
gives the API this mod would use, its shape, and what it would unblock. The features are skipped until then.

## Mob experience reward

- **Want:** `ClientHost`/`ServerHost::entity_type_experience(&self, entity_type: &str) -> Option<(u32, u32)>`,
  the least and most experience a kill of a default mob of that type gives (`Mob.getBaseExperienceReward`, with
  the per-type overrides vanilla keeps in code: a baby, a slime's size, the Ender Dragon).
- **Unblocks:** the "Experience: 5" line on JER's mob pages. The values live in Java code, not in data, so they
  can't be read from data packs, and copying the table would be Mojang data.

## Item enchantability and enchantment weight

- **Want:** `ClientHost::item_enchantability(&self, item: ItemKey) -> Option<u32>` (the `minecraft:enchantable`
  component's `value`), and `weight: u32` on `EnchantmentInfo`.
- **Unblocks:** JER's chance of each enchantment at each level from the enchanting table. Today the
  enchantments page lists which enchantments an item can get, their levels and the power each level needs
  (`enchantment_min_cost`), but not how likely each is.

## Spawn conditions

- **Want:** `ServerHost::entity_spawn_placement(&self, entity_type: &str) -> Option<SpawnPlacementInfo>` with the
  placement type (on ground, in water, …), the heightmap, and the light rule (`monster_spawn_light_level` of the
  dimension for monsters, sky light for animals).
- **Unblocks:** JER's "Light level: below 8" and "Any light level" lines on mob pages. `level_biome_spawners`
  already gives the biomes, which this mod shows.

## Listing data files

- **Want:** `Host::resource_list(&self, pack_type: PackType, folder: &Identifier) -> Vec<Identifier>`, every
  file under a folder of the selected packs (`ResourceManager.listResources`).
- **Unblocks:** reading data that has no registry the API exposes (structure-only loot, archaeology tables a
  mod adds outside `loot_table`). The mod now lists biomes, placed features and trade sets through
  `registry_entries`; if a server answers `None` for one of those registries, that page is empty.

## A recipe viewer to show the pages in

- **Want:** not an SDK hook but a cross-mod API in JEI for Horizon (`jei`): a public function to register a
  category and feed it chance recipes (`runtime::Category`, `ChanceRecipe` and `Layout::Chance` already exist
  there, unused). This mod would declare `jei = { version = "^0.4", optional = true }` in `[dependencies]` and
  call it inside `mod_api::if_installed!`.
- **Unblocks:** JER's pages inside JEI's recipe screen (R/U on an item), as the original mod is a JEI add-on.
  Until then the pages live in this mod's own window (J).

## Data on servers without the mod

- Loot tables, world generation and trades are server data; the vanilla protocol never sends them to a client.
  The mod's server half must be installed for those pages (single player always has it). Not an SDK gap that a
  hook could close, recorded so it isn't mistaken for a bug.
