//! Clicks, scrolling and keys: on the window, and the open key over another screen's slot.

use crate::client::{self, OPEN, Pending};
use crate::window::{self, Hit};
use mod_api::{ClientHost, ScreenView};

fn ours(host: &dyn ClientHost) -> bool {
    host.on(client::ID) && host.mod_screen() == Some(client::ID)
}

fn screen_count() -> usize {
    client::with(|s| client::current_screens(s).len())
}

/// Whether `key` (`key.keyboard.j`) is the one bound to the open mapping.
fn is_open_key(host: &dyn ClientHost, key: &str) -> bool {
    host.mod_key(client::ID, OPEN).is_some_and(|(k, _)| k == key)
}

/// A look-up of `item` from inside the window.
fn look_up(host: &dyn ClientHost, item: &str) {
    if item.starts_with('#') {
        return;
    }
    client::ensure_enchant_pages(host);
    client::with(|s| {
        let pages = std::mem::take(&mut s.pages);
        s.nav.look_up(&pages, item, &|t| client::hidden(host, t));
        s.pages = pages;
    });
}

/// True when the click was the window's.
pub fn mouse_pressed(host: &dyn ClientHost, screen: &ScreenView<'_>, button: i32) -> bool {
    if !ours(host) {
        return false;
    }
    let Some(hit) = window::hit(screen.mouse) else {
        return true;
    };
    let count = screen_count();
    match hit {
        Hit::Tab(tab) => client::with(|s| s.nav.set_tab(tab)),
        Hit::Prev => client::with(|s| s.nav.turn(-1, count)),
        Hit::Next => client::with(|s| s.nav.turn(1, count)),
        Hit::Back => {
            client::with(|s| s.nav.back());
        }
        Hit::Stack(item) if button == 0 => look_up(host, &item),
        Hit::Stack(_) => {}
    }
    true
}

pub fn mouse_scrolled(host: &dyn ClientHost, scroll: [f64; 2]) -> bool {
    if !ours(host) {
        return false;
    }
    let count = screen_count();
    if scroll[1] > 0.0 {
        client::with(|s| s.nav.turn(-1, count));
    } else if scroll[1] < 0.0 {
        client::with(|s| s.nav.turn(1, count));
    }
    true
}

/// Keys: on the window, arrows turn pages, Backspace goes back and the open key looks up the item under the
/// pointer; over another screen's slot, the open key opens the window on that item.
pub fn key_pressed(host: &dyn ClientHost, screen: &ScreenView<'_>, key: &str) -> bool {
    if !host.on(client::ID) {
        return false;
    }
    if ours(host) {
        let count = screen_count();
        match key {
            "key.keyboard.left" | "key.keyboard.page.up" => client::with(|s| s.nav.turn(-1, count)),
            "key.keyboard.right" | "key.keyboard.page.down" => {
                client::with(|s| s.nav.turn(1, count))
            }
            "key.keyboard.backspace" => {
                client::with(|s| s.nav.back());
            }
            _ if is_open_key(host, key) => {
                if let Some(Hit::Stack(item)) = window::hit(screen.mouse) {
                    look_up(host, &item);
                }
            }
            _ => return false,
        }
        return true;
    }
    if !is_open_key(host, key) {
        return false;
    }
    let Some(item) = host.screen_hovered_item() else {
        return false;
    };
    let item = item.id(host);
    client::with(|s| s.pending = Some(Pending::LookUp(item)));
    true
}
