//! Ore heights from the world's own placed and configured features, read at runtime from the player's data
//! packs (no game data is copied into the mod). Nothing here touches the game.

use crate::math::{height_shape, solid, tries, vein_blocks};
use mod_api::serde_json::Value;

/// The dimensions the page shows, with their bottom Y and height.
pub const DIMENSIONS: [(&str, i32, i32); 3] = [
    ("minecraft:overworld", -64, 384),
    ("minecraft:the_nether", 0, 256),
    ("minecraft:the_end", 0, 256),
];

/// One ore of one dimension: the blocks it places and how many of them a chunk holds at each Y.
#[derive(Clone, Debug, PartialEq)]
pub struct Ore {
    /// The blocks the feature places, the first (stone) one leading: `minecraft:iron_ore`, then the
    /// deepslate one.
    pub blocks: Vec<String>,
    /// Index into `DIMENSIONS`.
    pub dimension: usize,
    /// The average number of ore blocks per chunk at each Y, from the dimension's bottom up.
    pub per_y: Vec<f64>,
}

impl Ore {
    pub fn bottom(&self) -> i32 {
        DIMENSIONS[self.dimension].1
    }

    /// The Y with the most ore.
    pub fn peak(&self) -> i32 {
        let best = self
            .per_y
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map_or(0, |(i, _)| i as i32);
        self.bottom() + best
    }

    /// The lowest and highest Y where it generates at all.
    pub fn span(&self) -> Option<(i32, i32)> {
        let first = self.per_y.iter().position(|v| *v > 0.0)? as i32;
        let last = self.per_y.iter().rposition(|v| *v > 0.0)? as i32;
        Some((self.bottom() + first, self.bottom() + last))
    }

    /// Ore blocks in a whole chunk column.
    pub fn per_chunk(&self) -> f64 {
        self.per_y.iter().sum()
    }
}

/// The blocks an ore feature's configuration places, or None for a feature that isn't an ore.
pub fn ore_blocks(configured: &Value) -> Option<Vec<String>> {
    let kind = configured.get("type")?.as_str()?;
    if !matches!(kind, "minecraft:ore" | "minecraft:scattered_ore") {
        return None;
    }
    let mut blocks = Vec::new();
    for target in configured
        .get("config")?
        .get("targets")?
        .as_array()?
        .iter()
    {
        if let Some(name) = target
            .get("state")
            .and_then(|s| s.get("Name"))
            .and_then(Value::as_str)
            && !blocks.iter().any(|b| b == name)
        {
            blocks.push(name.to_owned());
        }
    }
    (!blocks.is_empty()).then_some(blocks)
}

/// The configured feature a placed feature names: its id (`minecraft:ore_iron`), or the inline feature.
pub enum FeatureRef<'a> {
    Id(&'a str),
    Inline(&'a Value),
}

pub fn feature_of(placed: &Value) -> Option<FeatureRef<'_>> {
    match placed.get("feature")? {
        Value::String(id) => Some(FeatureRef::Id(id)),
        inline @ Value::Object(_) => Some(FeatureRef::Inline(inline)),
        _ => None,
    }
}

/// The ore one placed feature makes in dimension `dimension` (an index into `DIMENSIONS`).
pub fn ore(placed: &Value, configured: &Value, dimension: usize) -> Option<Ore> {
    let blocks = ore_blocks(configured)?;
    let (_, bottom, height) = DIMENSIONS.get(dimension)?;
    let placement = placed
        .get("placement")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let size = configured
        .get("config")
        .and_then(|c| c.get("size"))
        .and_then(Value::as_f64)
        .unwrap_or(1.0);
    let veins = tries(placement);
    let blocks_per_vein = vein_blocks(size);
    let shape = height_shape(placement, (*bottom, *height));
    let per_y = shape
        .iter()
        .enumerate()
        .map(|(i, w)| veins * blocks_per_vein * w * solid(dimension, bottom + i as i32))
        .collect();
    Some(Ore {
        blocks,
        dimension,
        per_y,
    })
}

/// Ores of the same blocks in the same dimension added together (iron's upper, middle and small veins).
pub fn merge(ores: Vec<Ore>) -> Vec<Ore> {
    let mut out: Vec<Ore> = Vec::new();
    for ore in ores {
        match out
            .iter_mut()
            .find(|o| o.dimension == ore.dimension && o.blocks[0] == ore.blocks[0])
        {
            Some(o) => {
                for (a, b) in o.per_y.iter_mut().zip(&ore.per_y) {
                    *a += b;
                }
                for block in ore.blocks {
                    if !o.blocks.contains(&block) {
                        o.blocks.push(block);
                    }
                }
            }
            None => out.push(ore),
        }
    }
    out.sort_by(|a, b| (a.dimension, &a.blocks[0]).cmp(&(b.dimension, &b.blocks[0])));
    out
}

/// The placed features a biome lists, every generation step together (tags left out).
pub fn biome_features(biome: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for step in biome
        .get("features")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let ids: Vec<&str> = match step {
            Value::Array(list) => list.iter().filter_map(Value::as_str).collect(),
            Value::String(one) => vec![one.as_str()],
            _ => Vec::new(),
        };
        for id in ids {
            if !id.starts_with('#') && !out.iter().any(|o| o == id) {
                out.push(id.to_owned());
            }
        }
    }
    out
}

/// Which dimension a biome belongs to, by the `is_overworld` / `is_nether` / `is_end` tags (`in_tag`), and
/// by vanilla's names when the tags are not there.
pub fn dimension_of_biome(id: &str, in_tag: &dyn Fn(&str) -> bool) -> Option<usize> {
    if in_tag("minecraft:is_overworld") {
        return Some(0);
    }
    if in_tag("minecraft:is_nether") {
        return Some(1);
    }
    if in_tag("minecraft:is_end") {
        return Some(2);
    }
    let path = id.split_once(':').map_or(id, |(_, p)| p);
    const NETHER: [&str; 5] = [
        "nether_wastes",
        "crimson_forest",
        "warped_forest",
        "soul_sand_valley",
        "basalt_deltas",
    ];
    if NETHER.contains(&path) {
        Some(1)
    } else if path == "the_end" || path.starts_with("end_") || path == "small_end_islands" {
        Some(2)
    } else if id.starts_with("minecraft:") && path != "the_void" {
        Some(0)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mod_api::serde_json::json;

    fn diamond() -> (Value, Value) {
        let placed = json!({"feature": "minecraft:ore_diamond", "placement": [
            {"type": "minecraft:count", "count": 7},
            {"type": "minecraft:in_square"},
            {"type": "minecraft:height_range", "height": {"type": "minecraft:trapezoid",
                "min_inclusive": {"above_bottom": -80}, "max_inclusive": {"above_bottom": 80}}},
            {"type": "minecraft:biome"}
        ]});
        let configured = json!({"type": "minecraft:ore", "config": {"size": 4, "targets": [
            {"target": {"predicate_type": "minecraft:tag_match", "tag": "minecraft:stone_ore_replaceables"}, "state": {"Name": "minecraft:diamond_ore"}},
            {"target": {"predicate_type": "minecraft:tag_match", "tag": "minecraft:deepslate_ore_replaceables"}, "state": {"Name": "minecraft:deepslate_diamond_ore"}}
        ]}});
        (placed, configured)
    }

    #[test]
    fn a_trapezoid_ore_peaks_in_its_middle() {
        let (placed, configured) = diamond();
        let ore = ore(&placed, &configured, 0).expect("an ore");
        assert_eq!(
            ore.blocks,
            ["minecraft:diamond_ore", "minecraft:deepslate_diamond_ore"]
        );
        assert_eq!(ore.per_y.len(), 384);
        // The trapezoid's middle is at -64, where bedrock leaves little stone: the most ore sits just above.
        assert!((-64..=-50).contains(&ore.peak()), "{}", ore.peak());
        let (low, high) = ore.span().expect("generates");
        assert!(low <= -60);
        assert!(high <= 16);
        assert!(ore.per_chunk() > 0.0);
    }

    #[test]
    fn features_that_are_not_ores_are_skipped() {
        assert!(ore_blocks(&json!({"type": "minecraft:tree", "config": {}})).is_none());
        assert!(feature_of(&json!({"feature": 3})).is_none());
    }

    #[test]
    fn veins_of_one_ore_add_up() {
        let (placed, configured) = diamond();
        let a = ore(&placed, &configured, 0).expect("an ore");
        let total = a.per_chunk();
        let merged = merge(vec![a.clone(), a]);
        assert_eq!(merged.len(), 1);
        assert!((merged[0].per_chunk() - 2.0 * total).abs() < 1e-9);
    }

    #[test]
    fn biome_steps_flatten() {
        let biome = json!({"features": [[], ["minecraft:ore_dirt", "#minecraft:x"], ["minecraft:ore_dirt", "minecraft:ore_coal_upper"]]});
        assert_eq!(
            biome_features(&biome),
            ["minecraft:ore_dirt", "minecraft:ore_coal_upper"]
        );
    }

    #[test]
    fn biome_dimensions() {
        let no = |_: &str| false;
        assert_eq!(dimension_of_biome("minecraft:plains", &no), Some(0));
        assert_eq!(dimension_of_biome("minecraft:crimson_forest", &no), Some(1));
        assert_eq!(dimension_of_biome("minecraft:end_highlands", &no), Some(2));
        assert_eq!(dimension_of_biome("othermod:weird", &no), None);
        let nether = |t: &str| t == "minecraft:is_nether";
        assert_eq!(dimension_of_biome("othermod:weird", &nether), Some(1));
    }
}
