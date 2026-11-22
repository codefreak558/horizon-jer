//! The window: tabs along the top, one screen of pages in the middle, page arrows at the bottom. Drawn in
//! `screen_render_post` over the mod's own screen, with plain shapes and the game's item icons.

use crate::catalog::{self, Tab};
use crate::client::{self, Remote, State, names};
use crate::conditions::{FORTUNE, LOOTING, MAYBE, PLAYER, SHEARS, SILK, STATE};
use crate::math::{percent, trim_count};
use crate::protocol::{Page, Stack};
use crate::view::{self, Filter};
use mod_api::{ClientHost, DrawContext, GuiEntity, GuiEntityLook, GuiEntityView, ScreenView, Text};
use std::sync::{Mutex, PoisonError};

const W: f32 = 248.0;
const H: f32 = 204.0;
const CELL_W: f32 = 20.0;
const CELL_H: f32 = 28.0;

const PANEL: u32 = 0xFFC6C6C6;
const LIGHT: u32 = 0xFFFFFFFF;
const DARK: u32 = 0xFF555555;
const SLOT: u32 = 0xFF8B8B8B;
const SLOT_DARK: u32 = 0xFF373737;
const TEXT: u32 = 0xFF404040;
const MUTED: u32 = 0xFF707070;
const BAR: u32 = 0xFF3F76E4;

/// What a click or a key lands on.
#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    Tab(Tab),
    Prev,
    Next,
    Back,
    Stack(String),
}

/// The areas drawn last frame, for the input hooks.
static HITS: Mutex<Vec<([f32; 4], Hit)>> = Mutex::new(Vec::new());

fn inside(r: [f32; 4], p: [f32; 2]) -> bool {
    (r[0]..r[0] + r[2]).contains(&p[0]) && (r[1]..r[1] + r[3]).contains(&p[1])
}

/// What is under `mouse`, by last frame's drawing.
pub fn hit(mouse: [f32; 2]) -> Option<Hit> {
    let hits = HITS.lock().unwrap_or_else(PoisonError::into_inner);
    hits.iter()
        .rev()
        .find(|(r, _)| inside(*r, mouse))
        .map(|(_, h)| h.clone())
}

/// The window's rectangle on a GUI of `size`.
pub fn frame(size: [f32; 2]) -> [f32; 4] {
    [
        ((size[0] - W) / 2.0).floor(),
        ((size[1] - H) / 2.0).floor(),
        W,
        H,
    ]
}

/// Where cell `i` of a grid `cols` wide sits.
pub fn cell(origin: [f32; 2], cols: usize, i: usize) -> [f32; 2] {
    let cols = cols.max(1);
    [
        origin[0] + (i % cols) as f32 * CELL_W,
        origin[1] + (i / cols) as f32 * CELL_H,
    ]
}

/// The Y range the ore graph spans: where the ore generates, a little margin round it, inside the curve.
pub fn graph_range(curve: &[f32], bottom: i32) -> Option<(i32, i32)> {
    let first = curve.iter().position(|v| *v > 0.0)? as i32;
    let last = curve.iter().rposition(|v| *v > 0.0)? as i32;
    let top = curve.len() as i32 - 1;
    Some((bottom + (first - 4).max(0), bottom + (last + 4).min(top)))
}

struct Painter<'a> {
    host: &'a dyn ClientHost,
    draw: &'a mut dyn DrawContext,
    mouse: [f32; 2],
    hits: Vec<([f32; 4], Hit)>,
    tooltip: Vec<String>,
}

impl Painter<'_> {
    fn bevel(&mut self, r: [f32; 4], fill: u32, top: u32, bottom: u32) {
        let [x, y, w, h] = r;
        self.draw.fill(r, fill);
        self.draw.fill([x, y, w, 1.0], top);
        self.draw.fill([x, y, 1.0, h], top);
        self.draw.fill([x, y + h - 1.0, w, 1.0], bottom);
        self.draw.fill([x + w - 1.0, y, 1.0, h], bottom);
    }

    fn text(&mut self, text: &str, at: [f32; 2], color: u32) {
        self.draw.text(text, at, color, false);
    }

    fn centred(&mut self, text: &str, centre_x: f32, y: f32, color: u32) {
        let w = self.draw.text_width(text);
        self.text(text, [(centre_x - w / 2.0).floor(), y], color);
    }

    fn hover(&self, r: [f32; 4]) -> bool {
        inside(r, self.mouse)
    }

    fn button(&mut self, r: [f32; 4], label: &str, hit: Hit) {
        let fill = if self.hover(r) {
            0xFFA8B4E8
        } else {
            0xFFA0A0A0
        };
        self.bevel(r, fill, LIGHT, DARK);
        self.centred(label, r[0] + r[2] / 2.0, r[1] + (r[3] - 8.0) / 2.0, TEXT);
        self.hits.push((r, hit));
    }

    /// An item in a slot at `at`; `label` under it (a chance), `count` over its corner (a trade's).
    fn stack(&mut self, s: &Stack, at: [f32; 2], label: Option<String>, count: Option<String>) {
        let r = [at[0], at[1], 18.0, 18.0];
        self.bevel(r, SLOT, SLOT_DARK, LIGHT);
        let drawn = s.item.starts_with('#')
            || client::item_key(self.host, &s.item)
                .is_some_and(|k| self.draw.item(k, [at[0] + 1.0, at[1] + 1.0], 16.0));
        if s.item.starts_with('#') {
            self.centred("#", at[0] + 9.0, at[1] + 5.0, LIGHT);
        } else if !drawn {
            self.centred("?", at[0] + 9.0, at[1] + 5.0, LIGHT);
        }
        if let Some(count) = count {
            let w = self.draw.text_width(&count);
            self.draw
                .text(&count, [at[0] + 18.0 - w, at[1] + 10.0], LIGHT, true);
        }
        if let Some(label) = label {
            self.centred(&label, at[0] + 9.0, at[1] + 19.0, TEXT);
        }
        if self.hover(r) {
            self.draw
                .fill([at[0] + 1.0, at[1] + 1.0, 16.0, 16.0], 0x80FFFFFF);
            self.tooltip = stack_lines(self.host, s);
        }
        self.hits.push((r, Hit::Stack(s.item.clone())));
    }

    /// Drops in a grid: chance under each.
    fn grid(&mut self, stacks: &[Stack], origin: [f32; 2], cols: usize, rows: usize) {
        let room = cols * rows;
        for (i, s) in stacks.iter().take(room).enumerate() {
            self.stack(
                s,
                cell(origin, cols, i),
                Some(percent(f64::from(s.chance))),
                None,
            );
        }
        if stacks.len() > room {
            let more = names::tr(
                self.host,
                "jer.more",
                &format!("+{} more", stacks.len() - room),
                &[&(stacks.len() - room).to_string()],
            );
            let y = origin[1] + rows as f32 * CELL_H;
            self.text(&more, [origin[0], y], MUTED);
        }
    }
}

/// A stack's tooltip: its name, chance, count and what it depends on.
fn stack_lines(host: &dyn ClientHost, s: &Stack) -> Vec<String> {
    let tr = |key: &str, fallback: &str, args: &[&str]| names::tr(host, key, fallback, args);
    let mut lines = vec![names::item(host, &s.item)];
    if s.chance < 0.9995 {
        let p = percent(f64::from(s.chance));
        lines.push(tr("jer.chance", &format!("Chance: {p}"), &[&p]));
    }
    if s.high > s.low {
        let c = format!("{}-{}", s.low, s.high);
        lines.push(tr("jer.count", &format!("Count: {c}"), &[&c]));
        let a = trim_count(f64::from(s.average));
        lines.push(tr("jer.average", &format!("Average: {a}"), &[&a]));
    } else if s.low > 1 {
        let c = s.low.to_string();
        lines.push(tr("jer.count", &format!("Count: {c}"), &[&c]));
    }
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
            lines.push(tr(key, fallback, &[]));
        }
    }
    if host.setting_bool("dev").unwrap_or(false) {
        lines.push(s.item.clone());
    }
    lines
}

/// `screen_render_post` while the window is up.
pub fn draw(host: &dyn ClientHost, draw: &mut dyn DrawContext, screen: &ScreenView<'_>) {
    if host.mod_screen() != Some(client::ID) {
        return;
    }
    let size = draw.size();
    let mut p = Painter {
        host,
        draw,
        mouse: screen.mouse,
        hits: Vec::new(),
        tooltip: Vec::new(),
    };
    client::with(|s| paint(&mut p, s, size));
    let tooltip = std::mem::take(&mut p.tooltip);
    if !tooltip.is_empty() {
        let lines: Vec<Text> = tooltip.into_iter().map(Text::literal).collect();
        let _ = p.draw.gui_tooltip(&lines, p.mouse);
    }
    *HITS.lock().unwrap_or_else(PoisonError::into_inner) = p.hits;
}

fn paint(p: &mut Painter<'_>, s: &mut State, size: [f32; 2]) {
    let [x, y, w, h] = frame(size);
    p.draw.fill([0.0, 0.0, size[0], size[1]], 0x60000000);
    p.bevel([x, y, w, h], PANEL, LIGHT, DARK);
    let host = p.host;
    let hidden = |t: Tab| client::hidden(host, t);
    // Tabs: every tab not hidden; under a lookup, only those with a page for the item.
    let tabs = match &s.nav.here.filter {
        Filter::All => Tab::ALL.into_iter().filter(|t| !hidden(*t)).collect(),
        filter => view::tabs_with_pages(&s.pages, filter, &hidden),
    };
    if !tabs.contains(&s.nav.here.tab)
        && let Some(first) = tabs.first()
    {
        s.nav.set_tab(*first);
    }
    for (i, tab) in tabs.iter().enumerate() {
        let r = [x + 6.0 + i as f32 * 26.0, y + 5.0, 24.0, 22.0];
        let on = *tab == s.nav.here.tab;
        p.bevel(r, if on { 0xFFE0E0E0 } else { 0xFF9A9A9A }, LIGHT, DARK);
        if let Some(k) = client::item_key(host, tab.icon()) {
            p.draw.item(k, [r[0] + 4.0, r[1] + 3.0], 16.0);
        }
        if p.hover(r) {
            p.tooltip = vec![names::tr(
                host,
                &format!("jer.tab.{}", tab.key()),
                tab.title(),
                &[],
            )];
        }
        p.hits.push((r, Hit::Tab(*tab)));
    }
    if !s.nav.history.is_empty() {
        let label = names::tr(host, "jer.back", "Back", &[]);
        let bw = p.draw.text_width(&label) + 10.0;
        p.button([x + w - bw - 6.0, y + 8.0, bw, 16.0], &label, Hit::Back);
    }

    let screens = client::current_screens(s);
    s.nav.clamp(screens.len());
    let content = [x + 8.0, y + 32.0, w - 16.0, h - 54.0];
    match screens.get(s.nav.here.screen) {
        Some(screen) => {
            let pages: Vec<&Page> = screen.iter().filter_map(|i| s.pages.get(*i)).collect();
            page(p, s.nav.here.tab, &pages, content);
        }
        None => empty(p, s, content),
    }

    // Footer: arrows and the screen number.
    let fy = y + h - 20.0;
    if screens.len() > 1 {
        p.button([x + 6.0, fy, 18.0, 14.0], "<", Hit::Prev);
        p.button([x + w - 24.0, fy, 18.0, 14.0], ">", Hit::Next);
    }
    let n = format!(
        "{} / {}",
        (s.nav.here.screen + 1).min(screens.len()),
        screens.len()
    );
    p.centred(&n, x + w / 2.0, fy + 3.0, TEXT);
    if s.remote == Remote::Asked {
        let loading = names::tr(host, "jer.loading", "Loading...", &[]);
        p.text(&loading, [x + 30.0, fy + 3.0], MUTED);
    }
}

fn empty(p: &mut Painter<'_>, s: &State, content: [f32; 4]) {
    let host = p.host;
    let line = match (s.remote, s.nav.here.tab) {
        (_, Tab::Enchant) => names::tr(host, "jer.empty", "Nothing to show here.", &[]),
        (Remote::Absent, _) => names::tr(
            host,
            "jer.absent",
            "This server does not run JER: only enchantments can be shown.",
            &[],
        ),
        (Remote::Idle | Remote::Asked, _) => names::tr(host, "jer.loading", "Loading...", &[]),
        (Remote::Complete, _) => names::tr(host, "jer.empty", "Nothing to show here.", &[]),
    };
    wrap(p, &line, [content[0], content[1] + 20.0], content[2], TEXT);
}

/// Text broken into lines that fit `width`; returns the y under it.
fn wrap(p: &mut Painter<'_>, text: &str, at: [f32; 2], width: f32, color: u32) -> f32 {
    let mut y = at[1];
    let mut line = String::new();
    for word in text.split_whitespace() {
        let next = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if p.draw.text_width(&next) > width && !line.is_empty() {
            p.text(&line, [at[0], y], color);
            y += 10.0;
            line = word.to_owned();
        } else {
            line = next;
        }
    }
    if !line.is_empty() {
        p.text(&line, [at[0], y], color);
        y += 10.0;
    }
    y
}

fn title(p: &mut Painter<'_>, text: &str, content: [f32; 4]) {
    let mut text = text.to_owned();
    while p.draw.text_width(&text) > content[2] && text.chars().count() > 4 {
        text.pop();
    }
    p.text(&text, [content[0], content[1]], TEXT);
}

fn page(p: &mut Painter<'_>, tab: Tab, pages: &[&Page], content: [f32; 4]) {
    let Some(first) = pages.first() else {
        return;
    };
    let host = p.host;
    let [cx, cy, cw, ch] = content;
    let body = [cx, cy + 14.0, cw, ch - 14.0];
    match tab {
        Tab::Mob => {
            title(p, &names::entity(host, &first.subject), content);
            mob(p, first, body);
        }
        Tab::Chest => {
            title(p, &catalog::table_label(&first.subject), content);
            let cols = (cw / CELL_W) as usize;
            let rows = ((body[3] - 10.0) / CELL_H) as usize;
            p.grid(&first.outputs, [body[0], body[1] + 2.0], cols, rows);
        }
        Tab::Plant => {
            title(p, &names::item(host, &first.subject), content);
            let plant = Stack {
                item: first.subject.clone(),
                chance: 1.0,
                low: 1,
                high: 1,
                average: 1.0,
                flags: 0,
            };
            p.stack(&plant, [body[0], body[1] + 2.0], None, None);
            let arrow_x = body[0] + 24.0;
            p.text("->", [arrow_x, body[1] + 7.0], TEXT);
            let origin = [body[0] + 42.0, body[1] + 2.0];
            let cols = ((cw - 42.0) / CELL_W) as usize;
            let rows = ((body[3] - 10.0) / CELL_H) as usize;
            p.grid(&first.outputs, origin, cols, rows);
        }
        Tab::Ore => {
            let name = names::item(host, &first.subject);
            let dim = names::dimension(host, first.level);
            title(p, &format!("{name} - {dim}"), content);
            ore(p, first, body);
        }
        Tab::Trade => {
            let who = names::profession(host, &first.subject);
            let text = if first.level > 0 {
                format!("{who} - {}", names::level(host, first.level))
            } else {
                who
            };
            title(p, &text, content);
            trade_rows(p, pages, body);
        }
        Tab::Enchant => {
            title(p, &names::item(host, &first.subject), content);
            enchant(p, first, body);
        }
    }
}

/// Large mobs drawn smaller, so they fit the box.
fn mob_size(entity: &str) -> f32 {
    match entity.split_once(':').map_or(entity, |(_, p)| p) {
        "ender_dragon" => 5.0,
        "giant" => 5.0,
        "ghast" | "happy_ghast" => 7.0,
        "elder_guardian" | "wither" | "ravager" | "sniffer" => 14.0,
        "iron_golem" | "warden" | "camel" => 18.0,
        "squid" | "glow_squid" | "guardian" | "horse" | "skeleton_horse" | "zombie_horse" => 22.0,
        _ => 30.0,
    }
}

fn mob(p: &mut Painter<'_>, page: &Page, body: [f32; 4]) {
    let host = p.host;
    let [bx, by, bw, bh] = body;
    let box_r = [bx, by + 2.0, 72.0, 96.0];
    p.bevel(box_r, 0xFF2A2A2A, SLOT_DARK, LIGHT);
    let view = GuiEntityView::new(
        [
            box_r[0] + 2.0,
            box_r[1] + 2.0,
            box_r[0] + box_r[2] - 2.0,
            box_r[1] + box_r[3] - 2.0,
        ],
        mob_size(&page.subject),
        0.0625,
        GuiEntityLook::At(p.mouse),
    );
    let _ = p.draw.entity(&GuiEntity::kind(page.subject.clone()), view);

    // Spawn biomes under the box: a count, the list in the tooltip.
    let biomes: Vec<&str> = page
        .notes
        .iter()
        .filter_map(|n| n.strip_prefix("biome:"))
        .collect();
    let ny = box_r[1] + box_r[3] + 4.0;
    if !biomes.is_empty() {
        let n = biomes.len().to_string();
        let line = names::tr(host, "jer.biomes", &format!("Spawns in {n} biomes"), &[&n]);
        let r = [bx, ny, p.draw.text_width(&line), 9.0];
        p.text(&line, [bx, ny], MUTED);
        if p.hover(r) {
            let mut lines: Vec<String> = biomes
                .iter()
                .take(24)
                .map(|b| names::biome(host, b))
                .collect();
            if biomes.len() > 24 {
                lines.push(format!("+{}", biomes.len() - 24));
            }
            p.tooltip = lines;
        }
    }
    let origin = [bx + 80.0, by + 2.0];
    let cols = ((bw - 80.0) / CELL_W) as usize;
    let rows = ((bh - 10.0) / CELL_H) as usize;
    p.grid(&page.outputs, origin, cols, rows);
}

fn ore(p: &mut Painter<'_>, page: &Page, body: [f32; 4]) {
    let host = p.host;
    let [bx, by, bw, _] = body;
    for (i, block) in page.inputs.iter().enumerate() {
        p.stack(block, [bx + i as f32 * 20.0, by + 2.0], None, None);
    }
    let g = [bx, by + 24.0, bw, 60.0];
    p.bevel(g, 0xFF1E1E1E, SLOT_DARK, LIGHT);
    if let Some((low, high)) = graph_range(&page.curve, page.bottom) {
        let max = page
            .curve
            .iter()
            .copied()
            .fold(0.0f32, f32::max)
            .max(f32::EPSILON);
        let inner = g[2] - 2.0;
        let span = (high - low).max(1) as f32;
        for px in 0..inner as i32 {
            let yy = low + (px as f32 / inner * span).round() as i32;
            let v = usize::try_from(yy - page.bottom)
                .ok()
                .and_then(|i| page.curve.get(i))
                .copied()
                .unwrap_or(0.0);
            let bar = (v / max * (g[3] - 4.0)).round();
            if bar > 0.0 {
                p.draw.fill(
                    [g[0] + 1.0 + px as f32, g[1] + g[3] - 2.0 - bar, 1.0, bar],
                    BAR,
                );
            }
        }
        if p.hover(g) {
            let yy = low + ((p.mouse[0] - g[0] - 1.0) / inner * span).round() as i32;
            let v = usize::try_from(yy - page.bottom)
                .ok()
                .and_then(|i| page.curve.get(i))
                .copied()
                .unwrap_or(0.0);
            let per = format!("{v:.2}");
            p.tooltip = vec![
                format!("Y {yy}"),
                names::tr(host, "jer.per_chunk", &format!("{per} per chunk"), &[&per]),
            ];
        }
        let ly = g[1] + g[3] + 2.0;
        p.text(&low.to_string(), [g[0], ly], MUTED);
        let hs = high.to_string();
        let hw = p.draw.text_width(&hs);
        p.text(&hs, [g[0] + g[2] - hw, ly], MUTED);
        let total: f32 = page.curve.iter().sum();
        let t = format!("{total:.1}");
        let line = names::tr(
            host,
            "jer.ore_total",
            &format!("{t} blocks per chunk"),
            &[&t],
        );
        p.centred(&line, g[0] + g[2] / 2.0, ly, MUTED);
    }
    if !page.outputs.is_empty() {
        let dy = g[1] + g[3] + 14.0;
        let mined = names::tr(host, "jer.mined", "Mined:", &[]);
        p.text(&mined, [bx, dy + 5.0], TEXT);
        let mw = p.draw.text_width(&mined) + 6.0;
        let cols = ((bw - mw) / CELL_W) as usize;
        p.grid(&page.outputs, [bx + mw, dy], cols, 1);
    }
}

fn trade_rows(p: &mut Painter<'_>, pages: &[&Page], body: [f32; 4]) {
    let host = p.host;
    let [bx, by, bw, _] = body;
    for (row, page) in pages.iter().enumerate() {
        let ry = by + 2.0 + row as f32 * 22.0;
        let mut x = bx;
        for side in &page.inputs {
            p.stack(side, [x, ry], None, Some(count_text(side)));
            x += 20.0;
        }
        p.text("->", [bx + 44.0, ry + 5.0], TEXT);
        if let Some(out) = page.outputs.first() {
            p.stack(out, [bx + 60.0, ry], None, Some(count_text(out)));
        }
        let mut notes = Vec::new();
        if let Some(uses) = page.notes.iter().find_map(|n| n.strip_prefix("uses:")) {
            notes.push(names::tr(
                host,
                "jer.uses",
                &format!("{uses} uses"),
                &[uses],
            ));
        }
        if page.notes.iter().any(|n| n == "conditional") {
            notes.push(names::tr(host, "jer.conditional", "varies", &[]));
        }
        let line = notes.join(", ");
        let lw = p.draw.text_width(&line);
        p.text(&line, [bx + bw - lw, ry + 5.0], MUTED);
    }
}

fn count_text(s: &Stack) -> String {
    match (s.low, s.high) {
        (1, 1) => String::new(),
        (a, b) if a == b => a.to_string(),
        (a, b) => format!("{a}-{b}"),
    }
}

fn enchant(p: &mut Painter<'_>, page: &Page, body: [f32; 4]) {
    let host = p.host;
    let [bx, by, bw, _] = body;
    if let Some(item) = page.outputs.first() {
        p.stack(item, [bx, by + 2.0], None, None);
    }
    for (i, note) in page.notes.iter().enumerate() {
        let Some(rest) = note.strip_prefix("enchant:") else {
            continue;
        };
        let parts: Vec<&str> = rest.split('|').collect();
        let [id, max, low, high] = parts[..] else {
            continue;
        };
        let max: u32 = max.parse().unwrap_or(1);
        let levels = if max > 1 {
            format!("I-{}", catalog::roman(max))
        } else {
            "I".to_owned()
        };
        let ly = by + 2.0 + i as f32 * 11.0;
        let name = format!("{} {levels}", names::enchantment(host, id));
        p.text(&name, [bx + 24.0, ly], TEXT);
        let cost = names::tr(
            host,
            "jer.cost",
            &format!("power {low}-{high}"),
            &[low, high],
        );
        let cw = p.draw.text_width(&cost);
        p.text(&cost, [bx + bw - cw, ly], MUTED);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_is_centred() {
        assert_eq!(frame([480.0, 270.0]), [116.0, 33.0, W, H]);
    }

    #[test]
    fn grid_cells() {
        assert_eq!(cell([10.0, 20.0], 4, 0), [10.0, 20.0]);
        assert_eq!(cell([10.0, 20.0], 4, 5), [10.0 + CELL_W, 20.0 + CELL_H]);
        assert_eq!(cell([0.0, 0.0], 0, 1), [0.0, CELL_H]);
    }

    #[test]
    fn graph_ranges() {
        let mut curve = vec![0.0; 100];
        curve[50] = 1.0;
        curve[60] = 2.0;
        assert_eq!(graph_range(&curve, -64), Some((-64 + 46, -64 + 64)));
        assert_eq!(graph_range(&[0.0; 10], 0), None);
        curve[0] = 1.0;
        curve[99] = 1.0;
        assert_eq!(graph_range(&curve, 0), Some((0, 99)));
    }

    #[test]
    fn hits_find_the_last_drawn() {
        *HITS.lock().unwrap() = vec![
            ([0.0, 0.0, 100.0, 100.0], Hit::Prev),
            ([10.0, 10.0, 10.0, 10.0], Hit::Next),
        ];
        assert_eq!(hit([12.0, 12.0]), Some(Hit::Next));
        assert_eq!(hit([50.0, 50.0]), Some(Hit::Prev));
        assert_eq!(hit([150.0, 50.0]), None);
    }

    #[test]
    fn counts_and_sizes() {
        let s = |low, high| Stack {
            low,
            high,
            ..Stack::default()
        };
        assert_eq!(count_text(&s(1, 1)), "");
        assert_eq!(count_text(&s(3, 3)), "3");
        assert_eq!(count_text(&s(2, 5)), "2-5");
        assert!(mob_size("minecraft:ender_dragon") < mob_size("minecraft:zombie"));
    }
}
