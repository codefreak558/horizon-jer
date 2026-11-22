//! Villager trades from the world's own `trade_set` and `villager_trade` data (26.3's data-driven trades),
//! read at runtime from the player's data packs. Nothing here touches the game.

use crate::loot::count_range;
use mod_api::serde_json::Value;

/// One side of a trade: an item and how many.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Side {
    pub item: String,
    pub low: u32,
    pub high: u32,
}

/// One trade a villager may offer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trade {
    pub wants: Side,
    pub also_wants: Option<Side>,
    pub gives: Side,
    pub max_uses: Option<u32>,
    /// It has conditions this page can't show (a biome variant, enchantments picked at random).
    pub conditional: bool,
}

fn side(v: Option<&Value>) -> Option<Side> {
    let v = v?;
    let item = match v {
        Value::String(id) => id.clone(),
        _ => v.get("id").or_else(|| v.get("item"))?.as_str()?.to_owned(),
    };
    let (low, high) = count_range(v.get("count"));
    Some(Side { item, low, high })
}

/// A `villager_trade` file.
pub fn trade(v: &Value) -> Option<Trade> {
    Some(Trade {
        wants: side(v.get("wants"))?,
        also_wants: side(v.get("additional_wants")),
        gives: side(v.get("gives"))?,
        max_uses: v.get("max_uses").and_then(Value::as_u64).map(|n| n as u32),
        conditional: v.get("merchant_predicate").is_some()
            || v.get("given_item_modifiers").is_some()
            || v.get("given_item_modifier").is_some(),
    })
}

/// A list of trades or a tag of them (`["minecraft:a", …]`, `"minecraft:a"`, `"#minecraft:armorer/level_1"`):
/// the ids, with each `#tag` resolved by `tag` (which returns the tag file's JSON).
pub fn trade_ids(spec: &Value, tag: &dyn Fn(&str) -> Option<Value>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen_tags = Vec::new();
    push_ids(spec, tag, &mut seen_tags, &mut out);
    out
}

fn push_ids(
    spec: &Value,
    tag: &dyn Fn(&str) -> Option<Value>,
    seen_tags: &mut Vec<String>,
    out: &mut Vec<String>,
) {
    match spec {
        Value::String(s) => match s.strip_prefix('#') {
            Some(name) => {
                if seen_tags.iter().any(|t| t == name) {
                    return;
                }
                seen_tags.push(name.to_owned());
                if let Some(file) = tag(name) {
                    push_ids(
                        file.get("values").unwrap_or(&Value::Null),
                        tag,
                        seen_tags,
                        out,
                    );
                }
            }
            None => {
                if !out.contains(s) {
                    out.push(s.clone());
                }
            }
        },
        Value::Array(list) => {
            for v in list {
                push_ids(v, tag, seen_tags, out);
            }
        }
        // A tag file's optional entry: `{"id": "...", "required": false}`.
        Value::Object(_) => {
            if let Some(id) = spec.get("id") {
                push_ids(id, tag, seen_tags, out);
            }
        }
        _ => {}
    }
}

/// How many trades a set gives a villager.
pub fn amount(set: &Value) -> (u32, u32) {
    count_range(set.get("amount"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mod_api::serde_json::json;

    #[test]
    fn a_trade_reads_its_sides() {
        let t = trade(&json!({"gives":{"id":"minecraft:iron_ingot","count":2},"wants":{"id":"minecraft:emerald","count":3},"max_uses":12,"xp":7}))
            .expect("a trade");
        assert_eq!(t.wants.item, "minecraft:emerald");
        assert_eq!((t.wants.low, t.wants.high), (3, 3));
        assert_eq!(
            (t.gives.item.as_str(), t.gives.low),
            ("minecraft:iron_ingot", 2)
        );
        assert_eq!(t.max_uses, Some(12));
        assert!(t.also_wants.is_none());
        assert!(!t.conditional);
    }

    #[test]
    fn provider_counts_and_second_costs() {
        let t = trade(&json!({"gives":{"id":"minecraft:book","count":2},
            "wants":{"id":"minecraft:emerald","count":{"type":"minecraft:uniform","min":5,"max":9}},
            "additional_wants":{"id":"minecraft:book"},
            "given_item_modifier":[{"function":"minecraft:set_enchantments"}]}))
        .expect("a trade");
        assert_eq!((t.wants.low, t.wants.high), (5, 9));
        assert_eq!(t.also_wants.map(|s| s.item), Some("minecraft:book".into()));
        assert!(t.conditional);
        assert!(trade(&json!({"wants": {"id": "minecraft:emerald"}})).is_none());
    }

    #[test]
    fn ids_resolve_tags_once() {
        let tag = |name: &str| match name {
            "minecraft:a" => Some(
                json!({"values": ["minecraft:x", "#minecraft:b", {"id": "minecraft:y", "required": false}]}),
            ),
            "minecraft:b" => Some(json!({"values": ["minecraft:z", "#minecraft:a"]})),
            _ => None,
        };
        assert_eq!(
            trade_ids(&json!("#minecraft:a"), &tag),
            ["minecraft:x", "minecraft:z", "minecraft:y"]
        );
        assert_eq!(
            trade_ids(&json!(["minecraft:x", "minecraft:x"]), &tag),
            ["minecraft:x"]
        );
        assert_eq!(amount(&json!({"amount": 2})), (2, 2));
    }
}
