//! JER's pages as JEI categories (JEI for Horizon 0.6's add-on API): one category per tab, one recipe per page.
//!
//! This half is plain data with no game and no `jei` types in it, so it builds and is tested without JEI. The
//! few lines that hand it to JEI sit in `client::send_to_jei`, inside `mod_api::if_installed!`.

use crate::catalog::{self, Tab};
use crate::client::names;
use crate::conditions::{FORTUNE, LOOTING, MAYBE, PLAYER, SHEARS, SILK, STATE};
use crate::math::trim_count;
use crate::protocol::{Page, Stack};

/// What the pages need from the game: names in the player's language, and which items exist.
pub trait Words {
    fn item(&self, id: &str) -> String;
    fn entity(&self, id: &str) -> String;
    fn enchantment(&self, id: &str) -> String;
    fn dimension(&self, index: i32) -> String;
    fn profession(&self, id: &str) -> String;
    fn level(&self, level: i32) -> String;
    /// A line of the mod's language file, `%s` filled in order, or `fallback`.
    fn tr(&self, key: &str, fallback: &str, args: &[&str]) -> String;
    /// Whether the game has an item with this id (a block such as `minecraft:potatoes` may have none).
    fn has_item(&self, id: &str) -> bool;
}

/// The game's own names, for the live mod.
pub struct HostWords<'a>(pub &'a dyn mod_api::ClientHost);

impl Words for HostWords<'_> {
    fn item(&self, id: &str) -> String {
        names::item(self.0, id)
    }
    fn entity(&self, id: &str) -> String {
        names::entity(self.0, id)
    }
    fn enchantment(&self, id: &str) -> String {
        names::enchantment(self.0, id)
    }
    fn dimension(&self, index: i32) -> String {
        names::dimension(self.0, index)
    }
    fn profession(&self, id: &str) -> String {
        names::profession(self.0, id)
    }
    fn level(&self, level: i32) -> String {
        names::level(self.0, level)
    }
    fn tr(&self, key: &str, fallback: &str, args: &[&str]) -> String {
        names::tr(self.0, key, fallback, args)
    }
    fn has_item(&self, id: &str) -> bool {
        crate::client::item_key(self.0, id).is_some()
    }
}

/// One output: what it is, how likely, how many, and what it depends on.
#[derive(Clone, Debug, PartialEq)]
pub struct Out {
    pub item: String,
    pub chance: f32,
    pub min: u32,
    pub max: u32,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recipe {
    /// Each slot's alternatives, as an item id and a count.
    pub inputs: Vec<Vec<(String, u32)>>,
    pub outputs: Vec<Out>,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Category {
    /// `jer:mob`, `jer:chest`, … (`Tab::key`).
    pub id: String,
    pub title: String,
    pub icon: &'static str,
    pub recipes: Vec<Recipe>,
}

/// The id of a tab's category.
pub fn category_id(tab: Tab) -> String {
    format!("jer:{}", tab.key())
}

/// Every category, in tab order; a tab with no page has none.
pub fn categories(pages: &[Page], words: &dyn Words) -> Vec<Category> {
    Tab::ALL
        .into_iter()
        .filter_map(|tab| {
            let recipes: Vec<Recipe> = (pages.iter())
                .filter(|p| Tab::from_code(p.tab) == Some(tab))
                .map(|p| recipe(tab, p, words))
                .collect();
            (!recipes.is_empty()).then(|| Category {
                id: category_id(tab),
                title: words.tr(&format!("jer.tab.{}", tab.key()), tab.title(), &[]),
                icon: tab.icon(),
                recipes,
            })
        })
        .collect()
}

fn recipe(tab: Tab, page: &Page, words: &dyn Words) -> Recipe {
    let outputs: Vec<Out> = page.outputs.iter().map(|s| out(s, words)).collect();
    match tab {
        Tab::Mob => Recipe {
            // The spawn egg stands for the mob, when it has one.
            inputs: spawn_egg(&page.subject, words).into_iter().collect(),
            outputs,
            notes: vec![page.subject.clone()],
        },
        Tab::Chest => Recipe {
            inputs: Vec::new(),
            outputs,
            notes: vec![catalog::table_label(&page.source)],
        },
        Tab::Plant => Recipe {
            inputs: with_item(std::slice::from_ref(&page.subject), 1, words)
                .into_iter()
                .collect(),
            outputs,
            notes: Vec::new(),
        },
        Tab::Ore => Recipe {
            inputs: with_item(
                &page
                    .inputs
                    .iter()
                    .map(|s| s.item.clone())
                    .collect::<Vec<_>>(),
                1,
                words,
            )
            .into_iter()
            .collect(),
            outputs,
            notes: ore_notes(page, words),
        },
        Tab::Trade => Recipe {
            inputs: (page.inputs.iter())
                .filter_map(|s| {
                    with_item(std::slice::from_ref(&s.item), s.low.max(1) as u32, words)
                })
                .collect(),
            outputs,
            notes: trade_notes(page, words),
        },
        Tab::Enchant => Recipe {
            inputs: with_item(std::slice::from_ref(&page.subject), 1, words)
                .into_iter()
                .collect(),
            outputs: Vec::new(),
            notes: enchant_notes(page, words),
        },
    }
}

fn out(s: &Stack, words: &dyn Words) -> Out {
    let mut notes = Vec::new();
    let flags = s.flags as u8;
    for (bit, key, fallback) in [
        (PLAYER, "jer.flag.player", "Only when killed by a player"),
        (LOOTING, "jer.flag.looting", "Affected by Looting"),
        (FORTUNE, "jer.flag.fortune", "Affected by Fortune"),
        (SILK, "jer.flag.silk", "Needs Silk Touch"),
        (SHEARS, "jer.flag.shears", "Needs shears"),
        (STATE, "jer.flag.state", "Depends on the block's state"),
        (MAYBE, "jer.flag.maybe", "Depends on conditions not shown"),
    ] {
        if flags & bit != 0 {
            notes.push(words.tr(key, fallback, &[]));
        }
    }
    if s.high > s.low {
        let a = trim_count(f64::from(s.average));
        notes.push(words.tr("jer.average", &format!("Average: {a}"), &[&a]));
    }
    Out {
        item: s.item.clone(),
        chance: s.chance.clamp(0.0, 1.0),
        min: s.low.max(0) as u32,
        max: s.high.max(s.low).max(0) as u32,
        notes,
    }
}

/// A slot of the items that exist among `ids`; none when none does.
fn with_item(ids: &[String], count: u32, words: &dyn Words) -> Option<Vec<(String, u32)>> {
    let slot: Vec<(String, u32)> = (ids.iter())
        .filter(|id| !id.starts_with('#') && words.has_item(id))
        .map(|id| (id.clone(), count))
        .collect();
    (!slot.is_empty()).then_some(slot)
}

/// `minecraft:zombie` as the slot of `minecraft:zombie_spawn_egg`.
fn spawn_egg(entity: &str, words: &dyn Words) -> Option<Vec<(String, u32)>> {
    with_item(&[format!("{entity}_spawn_egg")], 1, words)
}

fn ore_notes(page: &Page, words: &dyn Words) -> Vec<String> {
    let mut notes = vec![words.dimension(page.level)];
    let peak = (page.curve.iter().enumerate())
        .filter(|(_, v)| **v > 0.0)
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(i, _)| page.bottom + i as i32);
    if let Some(y) = peak {
        let y = y.to_string();
        notes.push(words.tr("jer.peak", &format!("Peak at Y={y}"), &[&y]));
    }
    let total: f32 = page.curve.iter().sum();
    if total > 0.0 {
        let n = trim_count(f64::from(total));
        notes.push(words.tr("jer.ore_total", &format!("{n} blocks per chunk"), &[&n]));
    }
    notes
}

fn trade_notes(page: &Page, words: &dyn Words) -> Vec<String> {
    let who = words.profession(&page.subject);
    let mut notes = vec![if page.level > 0 {
        format!("{who} - {}", words.level(page.level))
    } else {
        who
    }];
    if let Some(uses) = page.notes.iter().find_map(|n| n.strip_prefix("uses:")) {
        notes.push(words.tr("jer.uses", &format!("{uses} uses"), &[uses]));
    }
    if page.notes.iter().any(|n| n == "conditional") {
        notes.push(words.tr("jer.conditional", "varies", &[]));
    }
    notes
}

fn enchant_notes(page: &Page, words: &dyn Words) -> Vec<String> {
    (page.notes.iter())
        .filter_map(|n| n.strip_prefix("enchant:"))
        .filter_map(|rest| {
            let parts: Vec<&str> = rest.split('|').collect();
            let [id, max, low, high] = parts[..] else {
                return None;
            };
            let max: u32 = max.parse().unwrap_or(1);
            let levels = if max > 1 {
                format!("I-{}", catalog::roman(max))
            } else {
                "I".to_owned()
            };
            let power = words.tr("jer.cost", &format!("power {low}-{high}"), &[low, high]);
            Some(format!("{} {levels} ({power})", words.enchantment(id)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conditions::PLAYER;

    struct Plain;
    impl Words for Plain {
        fn item(&self, id: &str) -> String {
            catalog::fallback_name(id)
        }
        fn entity(&self, id: &str) -> String {
            catalog::fallback_name(id)
        }
        fn enchantment(&self, id: &str) -> String {
            catalog::fallback_name(id)
        }
        fn dimension(&self, index: i32) -> String {
            ["Overworld", "The Nether", "The End"][index as usize].to_owned()
        }
        fn profession(&self, id: &str) -> String {
            catalog::fallback_name(id)
        }
        fn level(&self, level: i32) -> String {
            ["", "Novice", "Apprentice"][level as usize].to_owned()
        }
        fn tr(&self, _: &str, fallback: &str, _: &[&str]) -> String {
            fallback.to_owned()
        }
        fn has_item(&self, id: &str) -> bool {
            id != "minecraft:potatoes"
        }
    }

    fn stack(item: &str, chance: f32, low: i32, high: i32, flags: u8) -> Stack {
        Stack {
            item: item.into(),
            chance,
            low,
            high,
            average: (low + high) as f32 / 2.0,
            flags: i32::from(flags),
        }
    }

    fn page(tab: Tab, subject: &str) -> Page {
        Page {
            tab: tab.code(),
            source: subject.into(),
            subject: subject.into(),
            ..Page::default()
        }
    }

    #[test]
    fn mob_drops_carry_chance_range_and_flags() {
        let mut p = page(Tab::Mob, "minecraft:zombie");
        p.outputs = vec![stack("minecraft:rotten_flesh", 0.67, 0, 2, PLAYER)];
        let cats = categories(&[p], &Plain);
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0].id, "jer:mob");
        assert_eq!(cats[0].title, "Mob Drops");
        let r = &cats[0].recipes[0];
        assert_eq!(
            r.inputs,
            vec![vec![("minecraft:zombie_spawn_egg".to_owned(), 1)]]
        );
        assert_eq!(r.notes, ["minecraft:zombie"]);
        let o = &r.outputs[0];
        assert_eq!((o.min, o.max), (0, 2));
        assert!((o.chance - 0.67).abs() < 1e-6);
        assert!(o.notes.contains(&"Only when killed by a player".to_owned()));
        assert!(o.notes.iter().any(|n| n.starts_with("Average")));
    }

    #[test]
    fn plants_without_an_item_have_no_input() {
        let mut p = page(Tab::Plant, "minecraft:potatoes");
        p.outputs = vec![stack("minecraft:potato", 1.0, 1, 1, 0)];
        let mut q = page(Tab::Plant, "minecraft:wheat");
        q.outputs = vec![stack("minecraft:wheat", 1.0, 1, 1, 0)];
        let r = &categories(&[p, q], &Plain)[0].recipes;
        assert!(r[0].inputs.is_empty());
        assert_eq!(r[1].inputs, vec![vec![("minecraft:wheat".to_owned(), 1)]]);
    }

    #[test]
    fn chests_and_ores() {
        let mut c = page(Tab::Chest, "minecraft:chests/simple_dungeon");
        c.outputs = vec![stack("minecraft:diamond", 0.1, 1, 3, 0)];
        let mut o = page(Tab::Ore, "minecraft:diamond_ore");
        o.level = 0;
        o.inputs = vec![
            stack("minecraft:diamond_ore", 1.0, 1, 1, 0),
            stack("minecraft:deepslate_diamond_ore", 1.0, 1, 1, 0),
        ];
        o.outputs = vec![stack("minecraft:diamond", 1.0, 1, 1, 0)];
        o.bottom = -64;
        o.curve = vec![0.0, 0.5, 2.0, 1.0];
        let cats = categories(&[c, o], &Plain);
        assert_eq!(
            cats.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["jer:chest", "jer:ore"]
        );
        assert_eq!(cats[0].recipes[0].notes, ["Simple Dungeon"]);
        let ore = &cats[1].recipes[0];
        assert_eq!(ore.inputs[0].len(), 2);
        assert_eq!(ore.notes[0], "Overworld");
        assert_eq!(ore.notes[1], "Peak at Y=-62");
        assert_eq!(ore.notes[2], "3.5 blocks per chunk");
    }

    #[test]
    fn trades_take_costs_in_and_the_result_out() {
        let mut p = page(Tab::Trade, "minecraft:farmer");
        p.level = 1;
        p.inputs = vec![
            stack("minecraft:wheat", 1.0, 20, 20, 0),
            stack("minecraft:emerald", 1.0, 1, 1, 0),
        ];
        p.outputs = vec![stack("minecraft:bread", 1.0, 6, 6, 0)];
        p.notes = vec!["uses:16".into(), "conditional".into()];
        let r = &categories(&[p], &Plain)[0].recipes[0];
        assert_eq!(r.inputs.len(), 2);
        assert_eq!(r.inputs[0], vec![("minecraft:wheat".to_owned(), 20)]);
        assert_eq!(r.outputs[0].chance, 1.0);
        assert_eq!((r.outputs[0].min, r.outputs[0].max), (6, 6));
        assert_eq!(r.notes, ["Farmer - Novice", "16 uses", "varies"]);
    }

    #[test]
    fn enchantments_list_their_lines() {
        let mut p = page(Tab::Enchant, "minecraft:diamond_sword");
        p.notes = vec!["enchant:minecraft:sharpness|5|1|21".into(), "junk".into()];
        let r = &categories(&[p], &Plain)[0].recipes[0];
        assert!(r.outputs.is_empty());
        assert_eq!(r.notes, ["Sharpness I-V (power 1-21)"]);
    }

    #[test]
    fn no_pages_no_categories() {
        assert!(categories(&[], &Plain).is_empty());
        let stray = Page {
            tab: 99,
            ..Page::default()
        };
        assert!(categories(&[stray], &Plain).is_empty());
    }
}
