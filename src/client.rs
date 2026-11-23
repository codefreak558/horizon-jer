//! The client half: asks the server for its pages once per world, keeps them, builds the enchantment pages
//! itself, and runs what the window's input left for the next tick (opening the window needs a tick).

use crate::catalog::{self, Tab};
use crate::protocol::{self, ASK_TOKEN, Ask, Page, Pages, Stack};
use crate::view::{self, Nav};
use mod_api::{ClientHost, Identifier, ItemKey, PayloadSendError, RegistryEntry};
use std::sync::{Mutex, MutexGuard, PoisonError};

pub const ID: &str = "jer";
/// The key that opens the window, and looks up the item under the pointer.
pub const OPEN: &str = "open";

/// Ticks between two asks while the server hasn't answered or the connection isn't up.
const RETRY_TICKS: u32 = 100;
/// Asks answered "the server doesn't have the channel" before the mod is taken as absent there: right after
/// joining, the server's channel list may not be in yet.
const ABSENT_AFTER: u32 = 6;
/// Ticks to wait for the last batch of an answer before asking again.
const ANSWER_TICKS: u32 = 400;
/// Ticks the window keeps trying to open after the screen under it closed.
const OPEN_RETRIES: u32 = 10;
/// Enchantment lines on one enchantment page.
const ENCHANTS_PER_PAGE: usize = 12;

/// Where the server's answer stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Remote {
    /// Not asked yet in this world.
    #[default]
    Idle,
    /// Asked, batches arriving.
    Asked,
    /// Every page is in.
    Complete,
    /// The server doesn't run the mod: only the client's own pages show.
    Absent,
}

#[derive(Default)]
pub struct State {
    pub pages: Vec<Page>,
    pub remote: Remote,
    /// The enchantment pages, built on first use.
    pub enchant_built: bool,
    /// The pages went to JEI (this world).
    bridged: bool,
    pub nav: Nav,
    /// Ticks until the next ask.
    wait: u32,
    /// Asks the server answered with "no such channel".
    refused: u32,
    /// Ticks since the last ask, while its answer is still coming.
    waited: u32,
    /// Ticks left to retry opening the window (the screen it replaces may take a tick to close).
    open_retries: u32,
    /// What the input hooks asked of the next tick.
    pub pending: Option<Pending>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pending {
    /// Open the window where it was.
    Open,
    /// Close the screen that's up and open the window on this item's pages.
    LookUp(String),
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

pub fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut guard: MutexGuard<'_, Option<State>> =
        STATE.lock().unwrap_or_else(PoisonError::into_inner);
    f(guard.get_or_insert_with(State::default))
}

/// `jer:pages` arrived.
pub fn received(batch: Pages) {
    with(|s| {
        s.pages.extend(batch.pages);
        if batch.last {
            s.remote = Remote::Complete;
            // Pages that come after JEI was given the client's own (a late answer) go to JEI too.
            s.bridged = false;
            // The server's pages replace a world's earlier ones; keep the client's enchantment pages.
            s.pages.sort_by_key(|p| p.tab);
        }
    });
}

/// The world closed: forget its pages.
pub fn reset() {
    with(|s| *s = State::default());
}

/// Whether a tab is hidden on the settings page.
pub fn hidden(host: &dyn ClientHost, tab: Tab) -> bool {
    !host
        .setting_bool(&format!("tab_{}", tab.key()))
        .unwrap_or(true)
}

pub fn item_key(host: &dyn ClientHost, item: &str) -> Option<ItemKey> {
    host.item_key(&Identifier::parse(item).ok()?)
}

/// Each tick: ask the server once per world, then run what the input hooks left.
pub fn tick(host: &mut dyn ClientHost) {
    if !host.on(ID) {
        return;
    }
    ask(host);
    send_to_jei(host);
    if host.playing() && host.consume_click(ID, OPEN) {
        with(|s| s.pending = Some(Pending::Open));
    }
    let Some(pending) = with(|s| s.pending.take()) else {
        return;
    };
    if let Pending::LookUp(item) = &pending {
        ensure_enchant_pages(host);
        let found = with(|s| {
            let pages = std::mem::take(&mut s.pages);
            let found = s.nav.look_up(&pages, item, &|t| hidden(host, t));
            s.pages = pages;
            found
        });
        if !found {
            let name = names::item(host, item);
            let _ = host.chat_line(mod_api::Text::literal(names::tr(
                host,
                "jer.nothing",
                &format!("JER knows nothing about {name}"),
                &[&name],
            )));
            return;
        }
        // A container screen is up (the key was pressed over one of its slots): close it first, as Escape
        // would, since the window opens only over the world.
        if host.mod_screen() != Some(ID) && !host.playing() {
            host.close_screen();
        }
    }
    if host.mod_screen() != Some(ID) {
        if host.mod_screen_open(ID) {
            with(|s| s.open_retries = 0);
        } else {
            with(|s| {
                if s.open_retries < OPEN_RETRIES {
                    s.open_retries += 1;
                    s.pending = Some(Pending::Open);
                } else {
                    s.open_retries = 0;
                }
            });
        }
    }
    if with(|s| s.nav.here.tab == Tab::Enchant) {
        ensure_enchant_pages(host);
    }
}

/// Once the pages are all in (or the server has none) and the enchantment pages are built, hands them to JEI
/// as one category per tab. Without JEI installed the block doesn't run, and the window (J) is all there is.
fn send_to_jei(host: &mut dyn ClientHost) {
    let ready = with(|s| !s.bridged && matches!(s.remote, Remote::Complete | Remote::Absent));
    if !ready || !host.playing() {
        return;
    }
    ensure_enchant_pages(host);
    mod_api::if_installed!(host, jei, {
        let categories =
            with(|s| crate::bridge::categories(&s.pages, &crate::bridge::HostWords(&*host)));
        for category in categories {
            jei::register_category(
                &category.id,
                &category.title,
                category.icon,
                jei::Layout::Chance,
            );
            let recipes = category.recipes.iter().map(to_jei).collect();
            jei::set_chance_recipes(&category.id, recipes);
        }
    });
    with(|s| s.bridged = true);
}

/// A recipe as JEI's add-on API takes it.
#[cfg(feature = "jei")]
fn to_jei(recipe: &crate::bridge::Recipe) -> jei::ChanceRecipe {
    let stack = |item: &str, count: u32| jei::Stack {
        count,
        ..jei::Stack::item(item)
    };
    jei::ChanceRecipe {
        inputs: (recipe.inputs.iter())
            .map(|slot| slot.iter().map(|(item, n)| stack(item, *n)).collect())
            .collect(),
        outputs: (recipe.outputs.iter())
            .map(|o| jei::Chance {
                stack: stack(&o.item, 1),
                chance: o.chance,
                min: o.min,
                max: o.max,
                notes: o.notes.clone(),
            })
            .collect(),
        notes: recipe.notes.clone(),
    }
}

/// The world closed: JEI drops its recipes from JER's categories (it keeps the tabs registered).
pub fn clear_jei(host: &mut dyn ClientHost) {
    mod_api::if_installed!(host, jei, {
        for tab in Tab::ALL {
            jei::clear_category(&crate::bridge::category_id(tab));
        }
    });
}

fn ask(host: &mut dyn ClientHost) {
    let due = with(|s| {
        match s.remote {
            Remote::Idle => {
                if s.wait == 0 {
                    s.wait = RETRY_TICKS;
                    return true;
                }
                s.wait -= 1;
            }
            // An answer that never finished (lost on a reconnect): drop what came and ask again.
            Remote::Asked => {
                s.waited += 1;
                if s.waited > ANSWER_TICKS {
                    s.pages.retain(|p| p.tab == Tab::Enchant.code());
                    s.remote = Remote::Idle;
                    s.wait = 0;
                }
            }
            Remote::Complete | Remote::Absent => {}
        }
        false
    });
    if !due || !host.playing() {
        return;
    }
    let Some(token) = ASK_TOKEN.get() else {
        return;
    };
    let version = protocol::VERSION_NUMBER;
    match token.send(host, &Ask { version }) {
        Ok(_) => with(|s| {
            s.remote = Remote::Asked;
            s.waited = 0;
        }),
        Err(PayloadSendError::UnsupportedPeer(_)) => with(|s| {
            s.refused += 1;
            if s.refused >= ABSENT_AFTER {
                s.remote = Remote::Absent;
            }
        }),
        // Not connected yet, or the channel is busy: the next due tick asks again.
        Err(_) => {}
    }
}

/// The enchantment pages: for every item an enchanting table can enchant, the enchantments it can get.
pub fn ensure_enchant_pages(host: &dyn ClientHost) {
    if with(|s| s.enchant_built) {
        return;
    }
    let pages = enchant_pages(host);
    with(|s| {
        s.pages.extend(pages);
        s.pages.sort_by_key(|p| p.tab);
        s.enchant_built = true;
    });
}

fn enchant_pages(host: &dyn ClientHost) -> Vec<Page> {
    let registry = Identifier::parse("minecraft:enchantment").ok();
    let table: Vec<_> = host
        .enchantments()
        .into_iter()
        .filter(|e| {
            let entry = registry
                .clone()
                .zip(Identifier::parse(&e.id).ok())
                .map(|(r, i)| RegistryEntry::new(r, i));
            entry.is_none_or(|entry| {
                host.registry_entry_in_tag(&entry, "minecraft:in_enchanting_table")
                    .unwrap_or(true)
            })
        })
        .collect();
    let mut pages = Vec::new();
    for key in host.item_keys() {
        let item = key.id(host);
        let lines: Vec<String> = table
            .iter()
            .filter(|e| host.enchantment_supports(&e.id, key))
            .map(|e| {
                let low = host.enchantment_min_cost(&e.id, 1).unwrap_or(0);
                let high = host.enchantment_min_cost(&e.id, e.max_level).unwrap_or(low);
                format!("enchant:{}|{}|{low}|{high}", e.id, e.max_level)
            })
            .collect();
        for chunk in lines.chunks(ENCHANTS_PER_PAGE) {
            pages.push(Page {
                tab: Tab::Enchant.code(),
                source: item.clone(),
                subject: item.clone(),
                outputs: vec![Stack {
                    item: item.clone(),
                    chance: 1.0,
                    low: 1,
                    high: 1,
                    average: 1.0,
                    flags: 0,
                }],
                notes: chunk.to_vec(),
                ..Page::default()
            });
        }
    }
    pages
}

/// The screens of the window's tab, after the pages or the settings changed.
pub fn current_screens(s: &State) -> Vec<Vec<usize>> {
    view::screens(&s.pages, s.nav.here.tab, &s.nav.here.filter)
}

/// Names of things in the player's language, with English fallbacks.
pub mod names {
    use super::*;

    /// A line of the mod's language file, `%s` filled in order; `fallback` when the language has none.
    pub fn tr(host: &dyn ClientHost, key: &str, fallback: &str, args: &[&str]) -> String {
        let Some(mut text) = host.translate(key) else {
            return fallback.to_owned();
        };
        for arg in args {
            text = text.replacen("%s", arg, 1);
        }
        text
    }

    fn lookup(host: &dyn ClientHost, keys: &[String], id: &str) -> String {
        keys.iter()
            .find_map(|k| host.translate(k))
            .unwrap_or_else(|| catalog::fallback_name(id))
    }

    pub fn item(host: &dyn ClientHost, id: &str) -> String {
        lookup(host, &catalog::name_keys("item", id), id)
    }

    pub fn entity(host: &dyn ClientHost, id: &str) -> String {
        lookup(host, &catalog::name_keys("entity", id)[..1], id)
    }

    pub fn biome(host: &dyn ClientHost, id: &str) -> String {
        lookup(host, &catalog::name_keys("biome", id)[..1], id)
    }

    pub fn enchantment(host: &dyn ClientHost, id: &str) -> String {
        lookup(host, &catalog::name_keys("enchantment", id)[..1], id)
    }

    /// A villager profession (`entity.minecraft.villager.armorer`), or the wandering trader.
    pub fn profession(host: &dyn ClientHost, id: &str) -> String {
        let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
        let keys = [
            format!("entity.{namespace}.villager.{path}"),
            format!("entity.{namespace}.{path}"),
        ];
        lookup(host, &keys, id)
    }

    /// A villager level's name (`merchant.level.1`, "Novice").
    pub fn level(host: &dyn ClientHost, level: i32) -> String {
        host.translate(&format!("merchant.level.{level}"))
            .unwrap_or_else(|| format!("Level {level}"))
    }

    pub fn dimension(host: &dyn ClientHost, index: i32) -> String {
        let (id, fallback) = match index {
            1 => ("minecraft:the_nether", "The Nether"),
            2 => ("minecraft:the_end", "The End"),
            _ => ("minecraft:overworld", "Overworld"),
        };
        tr(host, &format!("jer.dim.{id}"), fallback, &[])
    }
}
