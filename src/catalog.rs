//! Which page a loot table belongs on, and the names the pages show. Nothing here touches the game.

use crate::math::title_case;

/// The window's tabs, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Tab {
    Mob,
    Chest,
    Plant,
    Ore,
    Trade,
    Enchant,
}

impl Tab {
    pub const ALL: [Tab; 6] = [
        Tab::Mob,
        Tab::Chest,
        Tab::Plant,
        Tab::Ore,
        Tab::Trade,
        Tab::Enchant,
    ];

    /// The number the payloads carry.
    pub fn code(self) -> i32 {
        self as i32
    }

    pub fn from_code(code: i32) -> Option<Tab> {
        Tab::ALL.get(usize::try_from(code).ok()?).copied()
    }

    /// The setting that hides it, and its translation key.
    pub fn key(self) -> &'static str {
        match self {
            Tab::Mob => "mob",
            Tab::Chest => "chest",
            Tab::Plant => "plant",
            Tab::Ore => "ore",
            Tab::Trade => "trade",
            Tab::Enchant => "enchant",
        }
    }

    /// The item drawn on the tab.
    pub fn icon(self) -> &'static str {
        match self {
            Tab::Mob => "minecraft:zombie_head",
            Tab::Chest => "minecraft:chest",
            Tab::Plant => "minecraft:wheat_seeds",
            Tab::Ore => "minecraft:diamond_ore",
            Tab::Trade => "minecraft:emerald",
            Tab::Enchant => "minecraft:enchanting_table",
        }
    }

    /// The English title, used when the language has no line for it.
    pub fn title(self) -> &'static str {
        match self {
            Tab::Mob => "Mob Drops",
            Tab::Chest => "Chest Loot",
            Tab::Plant => "Plant Drops",
            Tab::Ore => "World Generation",
            Tab::Trade => "Villager Trades",
            Tab::Enchant => "Enchantments",
        }
    }
}

/// Plant blocks outside the `minecraft:crops` tag whose drops JER's plant page shows.
const PLANTS: [&str; 9] = [
    "short_grass",
    "tall_grass",
    "fern",
    "large_fern",
    "nether_wart",
    "cocoa",
    "sweet_berry_bush",
    "melon",
    "pumpkin",
];

/// The page a loot table is shown on, and what it is about (an entity type, a block), or None for a table
/// the window doesn't list (sub-tables, gameplay, equipment, …). `is_crop` says whether a block is in the
/// `minecraft:crops` tag.
pub fn classify(table: &str, is_crop: &dyn Fn(&str) -> bool) -> Option<(Tab, String)> {
    let (namespace, path) = table.split_once(':').unwrap_or(("minecraft", table));
    if let Some(mob) = path.strip_prefix("entities/") {
        // `entities/sheep/white` is a part of the sheep's own table.
        return (!mob.contains('/')).then(|| (Tab::Mob, format!("{namespace}:{mob}")));
    }
    if path.starts_with("chests/") || path.starts_with("archaeology/") {
        return Some((Tab::Chest, table.to_owned()));
    }
    if let Some(block) = path.strip_prefix("blocks/") {
        let id = format!("{namespace}:{block}");
        if is_crop(&id) || (namespace == "minecraft" && PLANTS.contains(&block)) {
            return Some((Tab::Plant, id));
        }
    }
    None
}

/// A chest table's name: `minecraft:chests/village/village_weaponsmith` as "Village Weaponsmith".
pub fn table_label(table: &str) -> String {
    let path = table.split_once(':').map_or(table, |(_, p)| p);
    let path = path
        .strip_prefix("chests/")
        .or_else(|| path.strip_prefix("archaeology/"))
        .unwrap_or(path);
    let mut words: Vec<String> = Vec::new();
    for word in title_case(path).split(' ') {
        if words.last().is_none_or(|w| w != word) {
            words.push(word.to_owned());
        }
    }
    words.join(" ")
}

/// The translation keys a game id may be named by, most likely first (`item.minecraft.bone`, then
/// `block.minecraft.bone`).
pub fn name_keys(kind: &str, id: &str) -> [String; 2] {
    let (namespace, path) = id
        .trim_start_matches('#')
        .split_once(':')
        .unwrap_or(("minecraft", id));
    let path = path.replace('/', ".");
    let other = if kind == "item" { "block" } else { "item" };
    [
        format!("{kind}.{namespace}.{path}"),
        format!("{other}.{namespace}.{path}"),
    ]
}

/// A name for an id when the language has none: `minecraft:iron_ore` as "Iron Ore".
pub fn fallback_name(id: &str) -> String {
    let tag = id.starts_with('#');
    let path = id
        .trim_start_matches('#')
        .split_once(':')
        .map_or(id, |(_, p)| p);
    let name = title_case(path);
    if tag { format!("#{name}") } else { name }
}

/// A villager trade set's profession and level: `minecraft:armorer/level_1` as ("minecraft:armorer", 1).
/// The wandering trader's sets (`minecraft:wandering_trader/buying`, …) come out as level 0.
pub fn trade_set_owner(set: &str) -> Option<(String, i32)> {
    let (namespace, path) = set.split_once(':').unwrap_or(("minecraft", set));
    let (owner, rest) = path.split_once('/')?;
    let level = rest
        .strip_prefix("level_")
        .and_then(|l| l.parse().ok())
        .unwrap_or(0);
    Some((format!("{namespace}:{owner}"), level))
}

/// Roman numerals for enchantment levels and villager levels, as the game writes them up to 10.
pub fn roman(n: u32) -> String {
    const NUMERALS: [&str; 10] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"];
    match n {
        1..=10 => NUMERALS[n as usize - 1].to_owned(),
        _ => n.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wheat_only(id: &str) -> bool {
        id == "minecraft:wheat"
    }

    #[test]
    fn tables_go_to_their_pages() {
        assert_eq!(
            classify("minecraft:entities/zombie", &wheat_only),
            Some((Tab::Mob, "minecraft:zombie".into()))
        );
        assert_eq!(classify("minecraft:entities/sheep/white", &wheat_only), None);
        assert_eq!(
            classify("minecraft:chests/simple_dungeon", &wheat_only).map(|c| c.0),
            Some(Tab::Chest)
        );
        assert_eq!(
            classify("minecraft:blocks/wheat", &wheat_only),
            Some((Tab::Plant, "minecraft:wheat".into()))
        );
        assert_eq!(
            classify("minecraft:blocks/short_grass", &wheat_only).map(|c| c.0),
            Some(Tab::Plant)
        );
        assert_eq!(classify("minecraft:blocks/stone", &wheat_only), None);
        assert_eq!(classify("minecraft:gameplay/fishing", &wheat_only), None);
    }

    #[test]
    fn labels() {
        assert_eq!(
            table_label("minecraft:chests/village/village_weaponsmith"),
            "Village Weaponsmith"
        );
        assert_eq!(table_label("minecraft:chests/simple_dungeon"), "Simple Dungeon");
        assert_eq!(fallback_name("minecraft:iron_ore"), "Iron Ore");
        assert_eq!(fallback_name("#minecraft:logs"), "#Logs");
        assert_eq!(
            name_keys("item", "minecraft:bone"),
            ["item.minecraft.bone".to_owned(), "block.minecraft.bone".to_owned()]
        );
    }

    #[test]
    fn trade_sets() {
        assert_eq!(
            trade_set_owner("minecraft:armorer/level_3"),
            Some(("minecraft:armorer".into(), 3))
        );
        assert_eq!(
            trade_set_owner("minecraft:wandering_trader/buying"),
            Some(("minecraft:wandering_trader".into(), 0))
        );
        assert_eq!(trade_set_owner("minecraft:loose"), None);
    }

    #[test]
    fn tabs_round_trip() {
        for tab in Tab::ALL {
            assert_eq!(Tab::from_code(tab.code()), Some(tab));
        }
        assert_eq!(Tab::from_code(99), None);
        assert_eq!(roman(4), "IV");
        assert_eq!(roman(12), "12");
    }
}
