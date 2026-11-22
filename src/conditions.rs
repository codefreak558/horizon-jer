// From JEI for Horizon (https://github.com/codefreak558/horizon-jei, src/kit_collection.rs at 5e57e74),
// Copyright (c) 2026 JEI for Horizon contributors, MIT License (see NOTICE).
//
// The pure part of the kit's collection pages (where an item comes from, as Just Enough Resources shows it): loot
// table conditions and count functions, evaluated from their JSON. The loaders that read loot tables, world
// generation, structures and villager trades need game internals the mod SDK does not expose yet
// (docs/port-sdk.md), so only this evaluator is ported; nothing in the MVP calls it yet.

use crate::math::{average, range};
use mod_api::serde_json::Value;

/// The tool a source is read for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Hand,
    Silk,
    Shears,
}

pub const SILK: u8 = 1;
pub const SHEARS: u8 = 2;
pub const PLAYER: u8 = 4;
pub const FORTUNE: u8 = 8;
pub const LOOTING: u8 = 16;
/// Depends on something this page can't show (the mob's variant, the weather, ...).
pub const MAYBE: u8 = 32;
/// Depends on the block's state (a grown crop): an `alternatives` entry goes on to the next child.
pub const STATE: u8 = 64;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Test {
    /// Whether the condition holds for the tool.
    pub pass: bool,
    /// The chance it holds, 0 to 1.
    pub chance: f64,
    /// What it depends on: the `SILK`..`STATE` bits.
    pub flags: u8,
}

/// A condition that always holds.
pub const YES: Test = Test {
    pass: true,
    chance: 1.0,
    flags: 0,
};

/// Whether a loot condition's tool predicate matches the tool a source is read for.
pub fn tool_matches(v: &Value, tool: Tool) -> bool {
    let predicate = v.get("predicate").unwrap_or(&Value::Null);
    let text = predicate.to_string();
    match tool {
        Tool::Hand => false,
        Tool::Silk => text.contains("silk_touch"),
        Tool::Shears => text.contains("shears"),
    }
}

/// One loot condition, tested for the source's tool.
pub fn condition(v: &Value, tool: Tool) -> Test {
    // A named predicate: the tools are all this page needs of them.
    if let Value::String(name) = v {
        return match (name.contains("silk_touch"), name.contains("shear")) {
            (true, _) => Test {
                pass: tool == Tool::Silk,
                ..YES
            },
            (_, true) => Test {
                pass: tool == Tool::Shears,
                ..YES
            },
            _ => Test {
                flags: MAYBE,
                ..YES
            },
        };
    }
    let kind = v
        .get("type")
        .or_else(|| v.get("condition"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim_start_matches("minecraft:");
    let list = |key: &str| {
        v.get(key)
            .and_then(Value::as_array)
            .map_or(&[][..], |a| a.as_slice())
    };
    match kind {
        "random_chance" => Test {
            chance: v.get("chance").map_or(1.0, average),
            ..YES
        },
        "random_chance_with_enchanted_bonus" => Test {
            chance: v.get("unenchanted_chance").map_or(1.0, average),
            flags: LOOTING,
            ..YES
        },
        "table_bonus" => Test {
            chance: v
                .get("chances")
                .and_then(Value::as_array)
                .and_then(|c| c.first())
                .map_or(1.0, average),
            flags: FORTUNE,
            ..YES
        },
        "killed_by_player" => Test {
            flags: PLAYER,
            ..YES
        },
        "match_tool" => Test {
            pass: tool_matches(v, tool),
            ..YES
        },
        "survives_explosion" => YES,
        "block_state_property" | "match_block" => Test {
            flags: STATE,
            ..YES
        },
        "inverted" => {
            let t = v.get("term").map_or(YES, |t| condition(t, tool));
            if t.flags & (MAYBE | STATE) != 0 {
                t
            } else {
                Test {
                    pass: !t.pass,
                    chance: 1.0,
                    flags: t.flags,
                }
            }
        }
        "any_of" => {
            let mut out = Test {
                pass: false,
                chance: 0.0,
                flags: 0,
            };
            for t in list("terms").iter().map(|t| condition(t, tool)) {
                if t.pass {
                    out.pass = true;
                    out.chance = out.chance.max(t.chance);
                    out.flags |= t.flags;
                }
            }
            out
        }
        "all_of" => all(list("terms"), tool),
        _ => Test {
            flags: MAYBE,
            ..YES
        },
    }
}

/// All of a list of conditions (an `all_of`, or an entry's conditions).
pub fn all(list: &[Value], tool: Tool) -> Test {
    let mut out = YES;
    for t in list.iter().map(|t| condition(t, tool)) {
        out.pass &= t.pass;
        out.chance *= t.chance;
        out.flags |= t.flags;
    }
    out
}

/// An entry's or pool's conditions, tested together.
pub fn conditions(v: &Value, tool: Tool) -> Test {
    let mut out = v
        .get("conditions")
        .and_then(Value::as_array)
        .map_or(YES, |l| all(l, tool));
    match v.get("condition") {
        Some(Value::Array(l)) => {
            let t = all(l, tool);
            out = Test {
                pass: out.pass && t.pass,
                chance: out.chance * t.chance,
                flags: out.flags | t.flags,
            };
        }
        Some(c) => {
            let t = condition(c, tool);
            out = Test {
                pass: out.pass && t.pass,
                chance: out.chance * t.chance,
                flags: out.flags | t.flags,
            };
        }
        None => {}
    }
    out
}

#[derive(Clone, Debug)]
pub struct Got {
    pub item: String,
    pub chance: f64,
    pub low: u32,
    pub high: u32,
    pub flags: u8,
}

/// What `functions` make of one stack's count.
pub fn count_of(v: &Value, got: &mut Got) {
    let one = v.get("modifier").filter(|m| m.is_object());
    let lists = [v.get("functions"), v.get("modifier")];
    let list = lists
        .iter()
        .flatten()
        .filter_map(|l| l.as_array())
        .flatten()
        .chain(one);
    for f in list {
        match f
            .get("type")
            .or_else(|| f.get("function"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim_start_matches("minecraft:")
        {
            "set_count" => {
                let (low, high, chance) = f.get("count").map_or((1, 1, 1.0), range);
                if f.get("add").and_then(Value::as_bool) == Some(true) {
                    got.high += high;
                } else if f.get("conditions").is_some() || f.get("condition").is_some() {
                    // Only in some states (more candles): the count ranges over both.
                    got.low = got.low.min(low);
                    got.high = got.high.max(high);
                } else {
                    got.low = low;
                    got.high = high;
                    got.chance *= chance;
                }
            }
            "enchanted_count_increase" | "looting_enchant" => got.flags |= LOOTING,
            "apply_bonus" => got.flags |= FORTUNE,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mod_api::serde_json::json;

    #[test]
    fn chances_and_tools() {
        let c = json!({"condition": "minecraft:random_chance", "chance": 0.25});
        assert_eq!(condition(&c, Tool::Hand).chance, 0.25);
        let silk = json!({"condition": "minecraft:match_tool", "predicate": {"predicates": {"minecraft:enchantments": [{"enchantments": "minecraft:silk_touch"}]}}});
        assert!(condition(&silk, Tool::Silk).pass);
        assert!(!condition(&silk, Tool::Hand).pass);
        let player = json!({"conditions": [{"condition": "minecraft:killed_by_player"}]});
        assert_eq!(conditions(&player, Tool::Hand).flags, PLAYER);
    }

    #[test]
    fn set_count_sets_the_range() {
        let mut got = Got {
            item: "minecraft:stick".into(),
            chance: 1.0,
            low: 1,
            high: 1,
            flags: 0,
        };
        count_of(
            &json!({"functions": [{"function": "minecraft:set_count", "count": {"type": "minecraft:uniform", "min": 2, "max": 4}}]}),
            &mut got,
        );
        assert_eq!((got.low, got.high), (2, 4));
    }
}
