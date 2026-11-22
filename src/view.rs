//! Which pages the window shows: by tab, or every page about one item (a lookup), split into screens.
//! Nothing here touches the game.

use crate::catalog::Tab;
use crate::protocol::Page;

/// Trades of one profession and level shown together, this many to a screen.
pub const TRADES_PER_SCREEN: usize = 6;

/// What the window is browsing.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Filter {
    /// Every page of the tab.
    #[default]
    All,
    /// The pages where this item is dropped, generated, given or wanted, or which are about it.
    Item(String),
}

/// Whether a page has to do with `item`: it gives it, takes it, or is about it.
pub fn mentions(page: &Page, item: &str) -> bool {
    page.subject == item
        || page.outputs.iter().any(|s| s.item == item)
        || page.inputs.iter().any(|s| s.item == item)
}

/// The screens of one tab under a filter: each is the indices of the pages drawn on it. Trades of one
/// profession and level share screens; every other page has a screen of its own.
pub fn screens(pages: &[Page], tab: Tab, filter: &Filter) -> Vec<Vec<usize>> {
    let shown = pages.iter().enumerate().filter(|(_, p)| {
        p.tab == tab.code()
            && match filter {
                Filter::All => true,
                Filter::Item(item) => mentions(p, item),
            }
    });
    if tab != Tab::Trade {
        return shown.map(|(i, _)| vec![i]).collect();
    }
    let mut out: Vec<Vec<usize>> = Vec::new();
    let mut key: Option<(&str, i32)> = None;
    for (i, p) in shown {
        let this = (p.subject.as_str(), p.level);
        match out.last_mut() {
            Some(last) if key == Some(this) && last.len() < TRADES_PER_SCREEN => last.push(i),
            _ => out.push(vec![i]),
        }
        key = Some(this);
    }
    out
}

/// The tabs with something to show, in order.
pub fn tabs_with_pages(pages: &[Page], filter: &Filter, hidden: &dyn Fn(Tab) -> bool) -> Vec<Tab> {
    Tab::ALL
        .into_iter()
        .filter(|t| !hidden(*t) && !screens(pages, *t, filter).is_empty())
        .collect()
}

/// Where the window is: tab, filter and screen. `back` returns to the place before a lookup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub tab: Tab,
    pub filter: Filter,
    pub screen: usize,
}

impl Default for Place {
    fn default() -> Self {
        Place {
            tab: Tab::Mob,
            filter: Filter::All,
            screen: 0,
        }
    }
}

/// The window's history: where it is, and where it was.
#[derive(Clone, Debug, Default)]
pub struct Nav {
    pub here: Place,
    pub history: Vec<Place>,
}

impl Nav {
    /// Looks `item` up, from the first tab that has a page for it; false (and nothing changes) when no
    /// page mentions it.
    pub fn look_up(&mut self, pages: &[Page], item: &str, hidden: &dyn Fn(Tab) -> bool) -> bool {
        let filter = Filter::Item(item.to_owned());
        let Some(tab) = tabs_with_pages(pages, &filter, hidden).first().copied() else {
            return false;
        };
        let next = Place {
            tab,
            filter,
            screen: 0,
        };
        if next != self.here {
            self.history.push(std::mem::replace(&mut self.here, next));
        }
        true
    }

    /// Back to the place before the last lookup; false when there is none.
    pub fn back(&mut self) -> bool {
        match self.history.pop() {
            Some(place) => {
                self.here = place;
                true
            }
            None => false,
        }
    }

    pub fn set_tab(&mut self, tab: Tab) {
        if self.here.tab != tab {
            self.here.tab = tab;
            self.here.screen = 0;
        }
    }

    /// One screen on (`by` 1) or back (`by` -1), wrapping round `count` screens.
    pub fn turn(&mut self, by: i32, count: usize) {
        if count == 0 {
            self.here.screen = 0;
            return;
        }
        let n = count as i64;
        self.here.screen = (self.here.screen as i64 + i64::from(by)).rem_euclid(n) as usize;
    }

    /// Keeps the screen inside `count` screens after the pages changed.
    pub fn clamp(&mut self, count: usize) {
        if self.here.screen >= count {
            self.here.screen = count.saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Stack;

    fn page(tab: Tab, subject: &str, level: i32, outputs: &[&str]) -> Page {
        Page {
            tab: tab.code(),
            subject: subject.into(),
            level,
            outputs: outputs
                .iter()
                .map(|o| Stack {
                    item: (*o).into(),
                    ..Stack::default()
                })
                .collect(),
            ..Page::default()
        }
    }

    fn sample() -> Vec<Page> {
        let mut pages = vec![
            page(Tab::Mob, "minecraft:zombie", 0, &["minecraft:rotten_flesh", "minecraft:iron_ingot"]),
            page(Tab::Mob, "minecraft:skeleton", 0, &["minecraft:bone"]),
            page(Tab::Chest, "minecraft:chests/x", 0, &["minecraft:iron_ingot"]),
        ];
        for _ in 0..8 {
            pages.push(page(Tab::Trade, "minecraft:armorer", 1, &["minecraft:iron_helmet"]));
        }
        pages.push(page(Tab::Trade, "minecraft:armorer", 2, &["minecraft:bell"]));
        pages
    }

    #[test]
    fn tabs_and_lookups() {
        let pages = sample();
        let none = |_: Tab| false;
        assert_eq!(screens(&pages, Tab::Mob, &Filter::All).len(), 2);
        let iron = Filter::Item("minecraft:iron_ingot".into());
        assert_eq!(
            tabs_with_pages(&pages, &iron, &none),
            [Tab::Mob, Tab::Chest]
        );
        let hide_mobs = |t: Tab| t == Tab::Mob;
        assert_eq!(tabs_with_pages(&pages, &iron, &hide_mobs), [Tab::Chest]);
    }

    #[test]
    fn trades_share_screens_by_level() {
        let pages = sample();
        let s = screens(&pages, Tab::Trade, &Filter::All);
        assert_eq!(
            s.iter().map(Vec::len).collect::<Vec<_>>(),
            [TRADES_PER_SCREEN, 8 - TRADES_PER_SCREEN, 1]
        );
    }

    #[test]
    fn navigation() {
        let pages = sample();
        let none = |_: Tab| false;
        let mut nav = Nav::default();
        nav.set_tab(Tab::Chest);
        assert!(nav.look_up(&pages, "minecraft:bone", &none));
        assert_eq!(nav.here.tab, Tab::Mob);
        assert!(!nav.look_up(&pages, "minecraft:dirt", &none));
        assert!(nav.back());
        assert_eq!(nav.here.tab, Tab::Chest);
        assert!(!nav.back());
        nav.turn(-1, 3);
        assert_eq!(nav.here.screen, 2);
        nav.turn(1, 3);
        assert_eq!(nav.here.screen, 0);
        nav.here.screen = 5;
        nav.clamp(2);
        assert_eq!(nav.here.screen, 1);
    }
}
