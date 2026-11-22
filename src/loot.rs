//! What a loot table drops, worked out from its JSON: for every item, the chance a roll of the table gives at
//! least one, the count range and the average count, and what the drop depends on (a player kill, Looting,
//! Fortune, Silk Touch, the block's state).
//!
//! The chances are exact for the common shapes (weighted entries, constant/uniform/binomial rolls,
//! `random_chance`, `set_count`) and approximate where vanilla's rules depend on the world (a condition that
//! may or may not hold counts as holding, and a chance-limited candidate leaves the pool's total weight
//! unchanged). Nothing here touches the game.

use crate::conditions::{self, Got, Tool};
use crate::math::{average, range};
use mod_api::serde_json::Value;
use std::collections::BTreeMap;

/// One item a table can drop.
#[derive(Clone, Debug, PartialEq)]
pub struct Drop {
    /// The item id (`minecraft:bone`), or a tag as `#minecraft:logs`.
    pub item: String,
    /// The chance one roll of the whole table gives at least one, 0 to 1.
    pub chance: f64,
    /// The fewest and most it gives when it drops.
    pub low: u32,
    pub high: u32,
    /// The average count per roll of the table, the misses included.
    pub average: f64,
    /// What it depends on: `conditions::PLAYER`, `LOOTING`, `FORTUNE`, `SILK`, `SHEARS`, `MAYBE`, `STATE`.
    pub flags: u8,
}

/// Nested tables (`loot_table` entries) are followed this deep.
const MAX_DEPTH: u32 = 4;

/// One candidate of a pool: its weight and what it gives when chosen.
struct Leaf {
    weight: f64,
    gives: Vec<Got>,
}

/// How many times a pool rolls: each possible count with its chance.
pub fn roll_counts(v: Option<&Value>) -> Vec<(u32, f64)> {
    let Some(v) = v else {
        return vec![(1, 1.0)];
    };
    if let Some(n) = v
        .as_f64()
        .or_else(|| v.get("value").and_then(Value::as_f64))
    {
        return vec![(n.max(0.0) as u32, 1.0)];
    }
    if let (Some(n), Some(p)) = (
        v.get("n").and_then(Value::as_f64),
        v.get("p").and_then(Value::as_f64),
    ) {
        let n = n.max(0.0) as u32;
        let p = p.clamp(0.0, 1.0);
        return (0..=n).map(|k| (k, binomial(n, k, p))).collect();
    }
    if v.get("min").is_some() || v.get("max").is_some() {
        let low = v.get("min").map_or(0.0, average).floor().max(0.0) as u32;
        let high = (v.get("max").map_or(0.0, average).floor().max(0.0) as u32).max(low);
        let each = 1.0 / f64::from(high - low + 1);
        return (low..=high).map(|k| (k, each)).collect();
    }
    vec![(average(v).round().max(0.0) as u32, 1.0)]
}

fn binomial(n: u32, k: u32, p: f64) -> f64 {
    let mut c = 1.0;
    for i in 0..k {
        c = c * f64::from(n - i) / f64::from(i + 1);
    }
    c * p.powi(k as i32) * (1.0 - p).powi((n - k) as i32)
}

/// The type of an entry or function, without `minecraft:`.
fn kind(v: &Value) -> &str {
    v.get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim_start_matches("minecraft:")
}

/// What `table` drops when read for `tool`. `resolve` looks up another table by id, for `loot_table` entries.
pub fn evaluate<'a>(
    table: &'a Value,
    tool: Tool,
    resolve: &dyn Fn(&str) -> Option<&'a Value>,
) -> Vec<Drop> {
    let mut out = Vec::new();
    collect(table, tool, resolve, 0, &mut out);
    merge(out)
}

fn collect<'a>(
    table: &'a Value,
    tool: Tool,
    resolve: &dyn Fn(&str) -> Option<&'a Value>,
    depth: u32,
    out: &mut Vec<Drop>,
) {
    let Some(pools) = table.get("pools").and_then(Value::as_array) else {
        return;
    };
    for pool in pools {
        let test = conditions::conditions(pool, tool);
        if !test.pass || test.chance <= 0.0 {
            continue;
        }
        let mut leaves = Vec::new();
        for entry in pool
            .get("entries")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            expand(entry, tool, resolve, depth, 1.0, 0, &mut leaves);
        }
        let total: f64 = leaves.iter().map(|l| l.weight).sum();
        if total <= 0.0 {
            continue;
        }
        let rolls = roll_counts(pool.get("rolls"));
        let mean_rolls: f64 = rolls.iter().map(|(k, p)| f64::from(*k) * p).sum();
        let max_rolls = rolls.iter().map(|(k, _)| *k).max().unwrap_or(0);
        for leaf in &leaves {
            let pick = leaf.weight / total;
            for got in &leaf.gives {
                // The chance one roll gives this item: picked, its own conditions, a count above 0.
                let each = pick * got.chance * test.chance;
                if each <= 0.0 {
                    continue;
                }
                let chance: f64 = rolls
                    .iter()
                    .map(|(k, p)| p * (1.0 - (1.0 - each).powi(*k as i32)))
                    .sum();
                let mean_count = (f64::from(got.low) + f64::from(got.high)) / 2.0;
                out.push(Drop {
                    item: got.item.clone(),
                    chance,
                    low: got.low.max(1),
                    high: got.high.max(got.low.max(1)) * max_rolls.max(1),
                    average: mean_rolls * each * mean_count.max(1.0),
                    flags: got.flags | test.flags,
                });
            }
        }
    }
}

/// Turns one entry into the pool's candidates, as vanilla's `LootPoolEntryContainer.expand` does: composite
/// entries put their children in the pool, each with its own weight.
fn expand<'a>(
    entry: &'a Value,
    tool: Tool,
    resolve: &dyn Fn(&str) -> Option<&'a Value>,
    depth: u32,
    chance: f64,
    flags: u8,
    leaves: &mut Vec<Leaf>,
) {
    let test = conditions::conditions(entry, tool);
    if !test.pass {
        return;
    }
    let chance = chance * test.chance;
    let flags = flags | test.flags;
    let children = || {
        entry
            .get("children")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice)
    };
    let weight = entry.get("weight").map_or(1.0, average).max(0.0);
    match kind(entry) {
        "alternatives" => {
            // The first child that holds; one that depends on the block's state may not, so the next is
            // shown too (a crop's grown and young drops).
            for child in children() {
                let t = conditions::conditions(child, tool);
                if !t.pass {
                    continue;
                }
                expand(child, tool, resolve, depth, chance, flags, leaves);
                if t.flags & (conditions::STATE | conditions::MAYBE) == 0 && t.chance >= 1.0 {
                    break;
                }
            }
        }
        "group" | "sequence" => {
            for child in children() {
                if kind(entry) == "sequence" && !conditions::conditions(child, tool).pass {
                    break;
                }
                expand(child, tool, resolve, depth, chance, flags, leaves);
            }
        }
        "empty" => leaves.push(Leaf {
            weight,
            gives: Vec::new(),
        }),
        "item" | "tag" => {
            let item = match (kind(entry), entry.get("name").and_then(Value::as_str)) {
                ("tag", Some(name)) => format!("#{name}"),
                (_, Some(name)) => name.to_owned(),
                _ => return,
            };
            if item == "minecraft:air" {
                leaves.push(Leaf {
                    weight,
                    gives: Vec::new(),
                });
                return;
            }
            let mut got = Got {
                item,
                chance,
                low: 1,
                high: 1,
                flags,
            };
            conditions::count_of(entry, &mut got);
            leaves.push(Leaf {
                weight,
                gives: vec![got],
            });
        }
        "loot_table" => {
            let nested = match entry.get("value") {
                Some(Value::String(id)) => resolve(id),
                Some(inline @ Value::Object(_)) => Some(inline),
                _ => None,
            };
            let mut gives = Vec::new();
            if let Some(nested) = nested.filter(|_| depth < MAX_DEPTH) {
                let mut inner = Vec::new();
                collect(nested, tool, resolve, depth + 1, &mut inner);
                for d in merge(inner) {
                    gives.push(Got {
                        item: d.item,
                        chance: chance * d.chance,
                        low: d.low,
                        high: d.high,
                        flags: flags | d.flags,
                    });
                }
            }
            leaves.push(Leaf { weight, gives });
        }
        _ => {}
    }
}

/// One line per item: chances combined as independent pools, counts and averages added.
fn merge(drops: Vec<Drop>) -> Vec<Drop> {
    let mut by_item: BTreeMap<String, Drop> = BTreeMap::new();
    for d in drops {
        match by_item.get_mut(&d.item) {
            Some(m) => {
                m.chance = 1.0 - (1.0 - m.chance) * (1.0 - d.chance);
                m.low = m.low.min(d.low);
                m.high += d.high;
                m.average += d.average;
                m.flags |= d.flags;
            }
            None => {
                by_item.insert(d.item.clone(), d);
            }
        }
    }
    let mut out: Vec<Drop> = by_item.into_values().collect();
    out.sort_by(|a, b| {
        b.chance
            .partial_cmp(&a.chance)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.item.cmp(&b.item))
    });
    out
}

/// The count range of a number provider, for places outside a loot table (trades).
pub fn count_range(v: Option<&Value>) -> (u32, u32) {
    v.map_or((1, 1), |v| {
        let (low, high, _) = range(v);
        (low.max(1), high.max(low.max(1)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mod_api::serde_json::json;

    fn none<'a>(_: &str) -> Option<&'a Value> {
        None
    }

    fn find<'a>(drops: &'a [Drop], item: &str) -> &'a Drop {
        drops.iter().find(|d| d.item == item).expect(item)
    }

    #[test]
    fn a_single_weighted_pool_splits_by_weight() {
        let t = json!({"pools": [{"rolls": 1, "entries": [
            {"type": "minecraft:item", "name": "minecraft:diamond", "weight": 1},
            {"type": "minecraft:item", "name": "minecraft:stick", "weight": 3}
        ]}]});
        let d = evaluate(&t, Tool::Hand, &none);
        assert!((find(&d, "minecraft:diamond").chance - 0.25).abs() < 1e-9);
        assert!((find(&d, "minecraft:stick").chance - 0.75).abs() < 1e-9);
        assert_eq!(d[0].item, "minecraft:stick", "sorted by chance");
    }

    #[test]
    fn several_rolls_raise_the_chance_of_at_least_one() {
        let t = json!({"pools": [{"rolls": 3, "entries": [
            {"type": "minecraft:item", "name": "minecraft:diamond"},
            {"type": "minecraft:empty"}
        ]}]});
        let d = evaluate(&t, Tool::Hand, &none);
        let diamond = find(&d, "minecraft:diamond");
        assert!((diamond.chance - (1.0 - 0.5f64.powi(3))).abs() < 1e-9);
        assert!((diamond.average - 1.5).abs() < 1e-9);
        assert_eq!(diamond.high, 3);
    }

    #[test]
    fn uniform_rolls_average_their_counts() {
        let rolls = roll_counts(Some(
            &json!({"type": "minecraft:uniform", "min": 1, "max": 3}),
        ));
        assert_eq!(rolls.len(), 3);
        assert!((rolls.iter().map(|(_, p)| p).sum::<f64>() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn binomial_rolls_sum_to_one() {
        let rolls = roll_counts(Some(
            &json!({"type": "minecraft:binomial", "n": 4, "p": 0.3}),
        ));
        assert!((rolls.iter().map(|(_, p)| p).sum::<f64>() - 1.0).abs() < 1e-9);
        assert_eq!(rolls.len(), 5);
    }

    #[test]
    fn a_zombie_like_table() {
        // Rotten flesh 0-2 always, and a rare iron ingot on a player kill.
        let t = json!({"pools": [
            {"rolls": 1, "entries": [{"type": "minecraft:item", "name": "minecraft:rotten_flesh",
                "functions": [{"function": "minecraft:set_count", "count": {"type": "minecraft:uniform", "min": 0, "max": 2}},
                              {"function": "minecraft:enchanted_count_increase", "enchantment": "minecraft:looting"}]}]},
            {"rolls": 1, "conditions": [{"condition": "minecraft:killed_by_player"},
                {"condition": "minecraft:random_chance_with_enchanted_bonus", "unenchanted_chance": 0.025}],
             "entries": [{"type": "minecraft:item", "name": "minecraft:iron_ingot"}]}
        ]});
        let d = evaluate(&t, Tool::Hand, &none);
        let flesh = find(&d, "minecraft:rotten_flesh");
        assert!((flesh.chance - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!((flesh.low, flesh.high), (1, 2));
        assert_ne!(flesh.flags & conditions::LOOTING, 0);
        let iron = find(&d, "minecraft:iron_ingot");
        assert!((iron.chance - 0.025).abs() < 1e-9);
        assert_ne!(iron.flags & conditions::PLAYER, 0);
    }

    #[test]
    fn silk_touch_alternatives_follow_the_tool() {
        let t = json!({"pools": [{"rolls": 1, "entries": [{"type": "minecraft:alternatives", "children": [
            {"type": "minecraft:item", "name": "minecraft:diamond_ore", "conditions": [{"condition": "minecraft:match_tool",
                "predicate": {"predicates": {"minecraft:enchantments": [{"enchantments": "minecraft:silk_touch"}]}}}]},
            {"type": "minecraft:item", "name": "minecraft:diamond",
                "functions": [{"function": "minecraft:apply_bonus", "enchantment": "minecraft:fortune", "formula": "minecraft:ore_drops"}]}
        ]}]}]});
        let hand = evaluate(&t, Tool::Hand, &none);
        assert_eq!(hand.len(), 1);
        assert_eq!(hand[0].item, "minecraft:diamond");
        assert_ne!(hand[0].flags & conditions::FORTUNE, 0);
        let silk = evaluate(&t, Tool::Silk, &none);
        assert_eq!(silk[0].item, "minecraft:diamond_ore");
    }

    #[test]
    fn nested_tables_are_followed() {
        let inner = json!({"pools": [{"rolls": 1, "entries": [{"type": "minecraft:item", "name": "minecraft:white_wool"}]}]});
        let outer = json!({"pools": [{"rolls": 1, "entries": [{"type": "minecraft:loot_table", "value": "minecraft:entities/sheep/white"}]}]});
        let resolve = |id: &str| (id == "minecraft:entities/sheep/white").then_some(&inner);
        let d = evaluate(&outer, Tool::Hand, &resolve);
        assert_eq!(find(&d, "minecraft:white_wool").chance, 1.0);
    }

    #[test]
    fn a_self_referencing_table_stops() {
        let t = json!({"pools": [{"rolls": 1, "entries": [{"type": "minecraft:loot_table", "value": "x:loop"},
            {"type": "minecraft:item", "name": "minecraft:stone"}]}]});
        let resolve = |_: &str| Some(&t);
        let d = evaluate(&t, Tool::Hand, &resolve);
        assert!(d.iter().any(|d| d.item == "minecraft:stone"));
    }

    #[test]
    fn independent_pools_combine() {
        let pool = json!({"rolls": 1, "entries": [{"type": "minecraft:item", "name": "minecraft:bone"}, {"type": "minecraft:empty"}]});
        let t = json!({"pools": [pool.clone(), pool]});
        let d = evaluate(&t, Tool::Hand, &none);
        assert!((find(&d, "minecraft:bone").chance - 0.75).abs() < 1e-9);
    }

    #[test]
    fn tags_and_air() {
        let t = json!({"pools": [{"rolls": 1, "entries": [
            {"type": "minecraft:tag", "name": "minecraft:logs", "expand": false},
            {"type": "minecraft:item", "name": "minecraft:air"}
        ]}]});
        let d = evaluate(&t, Tool::Hand, &none);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].item, "#minecraft:logs");
        assert!((d[0].chance - 0.5).abs() < 1e-9);
    }

    #[test]
    fn trade_counts() {
        assert_eq!(count_range(None), (1, 1));
        assert_eq!(count_range(Some(&json!(5))), (5, 5));
        assert_eq!(
            count_range(Some(
                &json!({"type": "minecraft:uniform", "min": 2, "max": 4})
            )),
            (2, 4)
        );
    }
}
