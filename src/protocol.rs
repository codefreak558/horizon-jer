//! The payloads between the two halves: the client asks once per world, and the server answers with the
//! pages in batches. Both channels are optional, so a server without the mod is simply silent.

use mod_api::{PayloadCodec, ToClient, ToServer};
use std::sync::OnceLock;

/// The payload channels' version: a change to these types raises it.
pub const VERSION: &str = "1";
pub const ASK: &str = "jer:ask";
pub const PAGES: &str = "jer:pages";

/// "Send me your pages." `version` is this protocol's.
#[derive(PayloadCodec, Clone, Debug, Default, PartialEq)]
pub struct Ask {
    pub version: i32,
}

/// An item on a page.
#[derive(PayloadCodec, Clone, Debug, Default, PartialEq)]
pub struct Stack {
    /// Item id, or `#tag`.
    pub item: String,
    /// Chance of at least one, 0 to 1 (1 for a trade's sides).
    pub chance: f32,
    pub low: i32,
    pub high: i32,
    /// Average count, misses included.
    pub average: f32,
    /// `conditions::PLAYER` and the other bits.
    pub flags: i32,
}

/// One page.
#[derive(PayloadCodec, Clone, Debug, Default, PartialEq)]
pub struct Page {
    /// `catalog::Tab::code`.
    pub tab: i32,
    /// The loot table, placed feature or trade set it was read from.
    pub source: String,
    /// What it is about: an entity type, a block, a chest table, a profession.
    pub subject: String,
    /// A villager level (0 for none), or an ore's dimension index.
    pub level: i32,
    /// What it takes: a trade's costs.
    pub inputs: Vec<Stack>,
    /// What it gives: drops, ore blocks, a trade's result.
    pub outputs: Vec<Stack>,
    /// An ore's blocks per chunk at each Y from `bottom` up.
    pub curve: Vec<f32>,
    pub bottom: i32,
    /// Extra lines (spawn biomes, trades a set gives).
    pub notes: Vec<String>,
}

/// A batch of pages. `last` closes the answer.
#[derive(PayloadCodec, Clone, Debug, Default, PartialEq)]
pub struct Pages {
    pub pages: Vec<Page>,
    pub last: bool,
}

pub static ASK_TOKEN: OnceLock<ToServer<Ask>> = OnceLock::new();
pub static PAGES_TOKEN: OnceLock<ToClient<Pages>> = OnceLock::new();

/// Pages split into batches of about `budget` bytes, so no packet comes near the protocol's limits.
pub fn batches(pages: Vec<Page>, budget: usize) -> Vec<Pages> {
    let mut out = Vec::new();
    let mut current = Vec::new();
    let mut size = 0;
    for page in pages {
        let cost = estimate(&page);
        if !current.is_empty() && (size + cost > budget || current.len() >= 256) {
            out.push(Pages {
                pages: std::mem::take(&mut current),
                last: false,
            });
            size = 0;
        }
        size += cost;
        current.push(page);
    }
    out.push(Pages {
        pages: current,
        last: true,
    });
    out
}

fn estimate(page: &Page) -> usize {
    let stacks = page.inputs.iter().chain(&page.outputs);
    64 + page.source.len()
        + page.subject.len()
        + stacks.map(|s| s.item.len() + 24).sum::<usize>()
        + page.curve.len() * 4
        + page.notes.iter().map(|n| n.len() + 2).sum::<usize>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batches_split_and_close() {
        let page = Page {
            curve: vec![0.0; 384],
            ..Page::default()
        };
        let out = batches(vec![page; 100], 16 * 1024);
        assert!(out.len() > 1);
        assert_eq!(out.iter().map(|b| b.pages.len()).sum::<usize>(), 100);
        assert!(out.last().is_some_and(|b| b.last));
        assert!(out[..out.len() - 1].iter().all(|b| !b.last));
        let empty = batches(Vec::new(), 1024);
        assert_eq!(empty.len(), 1);
        assert!(empty[0].last && empty[0].pages.is_empty());
    }
}
