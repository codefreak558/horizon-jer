//! The server half: keeps a copy of every loot table as it loads, and, when a client asks, builds the pages
//! from those tables and from the world's own biome, feature and trade files, then sends them back.

use crate::catalog::{self, Tab};
use crate::conditions::Tool;
use crate::loot::{self, Drop};
use crate::protocol::{self, Ask, PAGES_TOKEN, Page, Stack};
use crate::trades;
use crate::worldgen::{self, FeatureRef};
use mod_api::serde_json::{self, Value};
use mod_api::{IPayloadContext, Identifier, PackType, RegistryEntry, ServerHost};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// Every loot table as the server last loaded it, after other mods' changes.
static TABLES: Mutex<BTreeMap<String, Value>> = Mutex::new(BTreeMap::new());
/// The pages built from them, until the next data reload.
static PAGES: Mutex<Option<Arc<Vec<Page>>>> = Mutex::new(None);

/// Bytes per batch of pages.
const BATCH: usize = 24 * 1024;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `loot_table_load`: a read-only copy. The table itself is left exactly as it is.
pub fn table_loaded(id: &str, table: &Value) {
    lock(&TABLES).insert(id.to_owned(), table.clone());
    *lock(&PAGES) = None;
}

/// The server stopped: the next world loads its own tables.
pub fn stopped() {
    lock(&TABLES).clear();
    *lock(&PAGES) = None;
}

/// `jer:ask`: build (or reuse) the pages and send them in batches.
pub fn answer(ask: Ask, cx: &mut dyn IPayloadContext) -> Result<(), String> {
    if ask.version != protocol::VERSION_NUMBER {
        return Err(format!(
            "jer: client protocol {} not supported",
            ask.version
        ));
    }
    let pages = {
        let cached = lock(&PAGES).clone();
        match cached {
            Some(pages) => pages,
            None => {
                let host = cx.server_host().map_err(|e| format!("{e:?}"))?;
                let pages = Arc::new(build(host));
                *lock(&PAGES) = Some(Arc::clone(&pages));
                pages
            }
        }
    };
    let token = PAGES_TOKEN
        .get()
        .ok_or("jer: the pages channel is not registered")?;
    for batch in protocol::batches(pages.as_ref().clone(), BATCH) {
        token.reply(cx, &batch).map_err(|e| format!("{e:?}"))?;
    }
    Ok(())
}

fn id(text: &str) -> Option<Identifier> {
    Identifier::parse(text).ok()
}

/// A data file of the world's data packs (`minecraft:worldgen/biome/plains.json`), parsed.
fn read_json(host: &dyn ServerHost, path: &str) -> Option<Value> {
    let bytes = host.resource(PackType::ServerData, &id(path)?)?;
    serde_json::from_slice(&bytes).ok()
}

/// `minecraft:plains` in `worldgen/biome` as `minecraft:worldgen/biome/plains.json`.
fn data_path(folder: &str, entry: &str) -> String {
    let (namespace, path) = entry.split_once(':').unwrap_or(("minecraft", entry));
    format!("{namespace}:{folder}/{path}.json")
}

fn in_tag(host: &dyn ServerHost, registry: &str, entry: &str, tag: &str) -> bool {
    match (id(registry), id(entry)) {
        (Some(r), Some(e)) => host
            .registry_entry_in_tag(&RegistryEntry::new(r, e), tag)
            .unwrap_or(false),
        _ => false,
    }
}

fn entries(host: &dyn ServerHost, registry: &str) -> Vec<String> {
    host.registry_entries(registry)
        .unwrap_or_default()
        .iter()
        .map(ToString::to_string)
        .collect()
}

pub fn stack(d: &Drop) -> Stack {
    Stack {
        item: d.item.clone(),
        chance: d.chance as f32,
        low: d.low as i32,
        high: d.high as i32,
        average: d.average as f32,
        flags: i32::from(d.flags),
    }
}

fn exact(item: &str, low: u32, high: u32) -> Stack {
    Stack {
        item: item.to_owned(),
        chance: 1.0,
        low: low as i32,
        high: high as i32,
        average: (low + high) as f32 / 2.0,
        flags: 0,
    }
}

/// Every page, in tab order.
fn build(host: &mut dyn ServerHost) -> Vec<Page> {
    let tables = lock(&TABLES).clone();
    let spawns = spawn_biomes(host);
    let mut pages = loot_pages(host, &tables, &spawns);
    pages.extend(ore_pages(host, &tables));
    pages.extend(trade_pages(host));
    pages.sort_by_key(|p| p.tab);
    pages
}

/// Mob drops, chest loot and plant drops.
fn loot_pages(
    host: &dyn ServerHost,
    tables: &BTreeMap<String, Value>,
    spawns: &BTreeMap<String, Vec<String>>,
) -> Vec<Page> {
    let resolve = |id: &str| tables.get(id);
    let is_crop = |block: &str| in_tag(host, "minecraft:block", block, "minecraft:crops");
    let mut pages = Vec::new();
    for (table_id, table) in tables {
        let Some((tab, subject)) = catalog::classify(table_id, &is_crop) else {
            continue;
        };
        let drops = loot::evaluate(table, Tool::Hand, &resolve);
        if drops.is_empty() {
            continue;
        }
        let notes = spawns.get(&subject).cloned().unwrap_or_default();
        pages.push(Page {
            tab: tab.code(),
            source: table_id.clone(),
            subject,
            outputs: drops.iter().map(stack).collect(),
            notes,
            ..Page::default()
        });
    }
    pages
}

/// For each entity type, the biomes it spawns in naturally, as `biome:<id>` notes, after one
/// `category:<name>` note.
fn spawn_biomes(host: &dyn ServerHost) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, (String, BTreeSet<String>)> = BTreeMap::new();
    let Some(level) = host.level("minecraft:overworld") else {
        return BTreeMap::new();
    };
    for biome in entries(host, "minecraft:worldgen/biome") {
        for group in level.biome_spawners(host, &biome).unwrap_or_default() {
            for spawn in group.entries {
                let e = out
                    .entry(spawn.entity)
                    .or_insert_with(|| (group.category.clone(), BTreeSet::new()));
                e.1.insert(biome.clone());
            }
        }
    }
    out.into_iter()
        .map(|(entity, (category, biomes))| {
            let mut notes = vec![format!("category:{category}")];
            notes.extend(biomes.into_iter().map(|b| format!("biome:{b}")));
            (entity, notes)
        })
        .collect()
}

/// The placed features each dimension's biomes list, or every placed feature with a guessed dimension
/// when the biomes can't be read.
fn placed_features(host: &dyn ServerHost) -> BTreeMap<String, BTreeSet<usize>> {
    let mut out: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    for biome in entries(host, "minecraft:worldgen/biome") {
        let tag = |t: &str| in_tag(host, "minecraft:worldgen/biome", &biome, t);
        let Some(dimension) = worldgen::dimension_of_biome(&biome, &tag) else {
            continue;
        };
        let Some(file) = read_json(host, &data_path("worldgen/biome", &biome)) else {
            continue;
        };
        for feature in worldgen::biome_features(&file) {
            out.entry(feature).or_default().insert(dimension);
        }
    }
    if out.is_empty() {
        for feature in entries(host, "minecraft:worldgen/placed_feature") {
            let dimension = usize::from(feature.contains("nether") || feature.contains("debris"));
            out.entry(feature).or_default().insert(dimension);
        }
    }
    out
}

/// One page per ore and dimension: its blocks, its height curve and what mining it gives.
fn ore_pages(host: &dyn ServerHost, tables: &BTreeMap<String, Value>) -> Vec<Page> {
    let mut ores = Vec::new();
    for (placed_id, dimensions) in placed_features(host) {
        let Some(placed) = read_json(host, &data_path("worldgen/placed_feature", &placed_id))
        else {
            continue;
        };
        let configured = match worldgen::feature_of(&placed) {
            Some(FeatureRef::Id(c)) => {
                read_json(host, &data_path("worldgen/configured_feature", c))
            }
            Some(FeatureRef::Inline(v)) => Some(v.clone()),
            None => None,
        };
        let Some(configured) = configured else {
            continue;
        };
        for dimension in dimensions {
            ores.extend(worldgen::ore(&placed, &configured, dimension));
        }
    }
    let resolve = |id: &str| tables.get(id);
    worldgen::merge(ores)
        .into_iter()
        .filter(|ore| ore.per_chunk() > 0.0)
        .map(|ore| {
            let block = &ore.blocks[0];
            let drops = tables
                .get(&data_table(block))
                .map(|t| loot::evaluate(t, Tool::Hand, &resolve))
                .unwrap_or_default();
            Page {
                tab: Tab::Ore.code(),
                source: block.clone(),
                subject: block.clone(),
                level: ore.dimension as i32,
                inputs: ore.blocks.iter().map(|b| exact(b, 1, 1)).collect(),
                outputs: drops.iter().map(stack).collect(),
                curve: ore.per_y.iter().map(|v| *v as f32).collect(),
                bottom: ore.bottom(),
                notes: Vec::new(),
            }
        })
        .collect()
}

/// `minecraft:iron_ore`'s loot table id.
fn data_table(block: &str) -> String {
    let (namespace, path) = block.split_once(':').unwrap_or(("minecraft", block));
    format!("{namespace}:blocks/{path}")
}

/// One page per trade a villager profession (or the wandering trader) may offer.
fn trade_pages(host: &dyn ServerHost) -> Vec<Page> {
    let tag = |name: &str| read_json(host, &data_path("tags/villager_trade", name));
    let mut pages = Vec::new();
    for set_id in entries(host, "minecraft:trade_set") {
        let Some((owner, level)) = catalog::trade_set_owner(&set_id) else {
            continue;
        };
        let Some(set) = read_json(host, &data_path("trade_set", &set_id)) else {
            continue;
        };
        let (low, high) = trades::amount(&set);
        for trade_id in trades::trade_ids(set.get("trades").unwrap_or(&Value::Null), &tag) {
            let Some(trade) = read_json(host, &data_path("villager_trade", &trade_id))
                .and_then(|v| trades::trade(&v))
            else {
                continue;
            };
            let mut inputs = vec![exact(&trade.wants.item, trade.wants.low, trade.wants.high)];
            if let Some(also) = &trade.also_wants {
                inputs.push(exact(&also.item, also.low, also.high));
            }
            let mut notes = vec![format!("set:{set_id}"), format!("amount:{low}-{high}")];
            if let Some(uses) = trade.max_uses {
                notes.push(format!("uses:{uses}"));
            }
            if trade.conditional {
                notes.push("conditional".to_owned());
            }
            pages.push(Page {
                tab: Tab::Trade.code(),
                source: trade_id,
                subject: owner.clone(),
                level,
                inputs,
                outputs: vec![exact(&trade.gives.item, trade.gives.low, trade.gives.high)],
                notes,
                ..Page::default()
            });
        }
    }
    pages.sort_by(|a, b| (&a.subject, a.level).cmp(&(&b.subject, b.level)));
    pages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_paths() {
        assert_eq!(
            data_path("worldgen/biome", "minecraft:plains"),
            "minecraft:worldgen/biome/plains.json"
        );
        assert_eq!(
            data_path("tags/villager_trade", "minecraft:armorer/level_1"),
            "minecraft:tags/villager_trade/armorer/level_1.json"
        );
        assert_eq!(
            data_table("minecraft:iron_ore"),
            "minecraft:blocks/iron_ore"
        );
    }
}
