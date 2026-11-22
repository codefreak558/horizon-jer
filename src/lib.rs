//! Just Enough Resources for Horizon: where things come from, in a window of its own. An unofficial port of
//! way2muchnoise's Just Enough Resources (see NOTICE).
//!
//! The server half (`server`) keeps a copy of every loot table and reads the world's biome, feature and
//! trade files; a client asks for the pages once per world (`protocol`) and draws them (`window`). The pure
//! parts (`loot`, `worldgen`, `trades`, `catalog`, `view`) have no game in them and carry the unit tests.
pub mod catalog;
pub mod conditions;
pub mod loot;
pub mod math;
pub mod protocol;
pub mod trades;
pub mod view;
pub mod worldgen;

mod client;
mod input;
mod server;
mod settings;
mod window;

use client::{ID, OPEN};
use mod_api::{
    ClientHost, ClientPlayerLoggingOutEvent, ClientTickPostEvent, DataLoadContext, Flow, Hooks,
    KeyRegistration, LootTableLoadEvent, PayloadId, Registrar, RegistrationContext,
    ScreenKeyPressedPreEvent, ScreenMouseButtonPressedPreEvent, ScreenMouseScrolledPreEvent,
    ScreenRenderPostEvent,
};
use protocol::{ASK, ASK_TOKEN, Ask, PAGES, PAGES_TOKEN, Pages};

fn flow(cancel: bool) -> Flow {
    if cancel { Flow::Cancel } else { Flow::Continue }
}

#[derive(Default)]
struct Jer;

#[mod_api::horizon_mod]
impl Hooks for Jer {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        for setting in settings::settings() {
            // Only a malformed spec is refused, and the tests check every spec.
            let _ = r.register_setting(setting);
        }
        r.register_key_mapping(KeyRegistration::new(OPEN, "key.keyboard.j").label("jer.key.open"));
        let mut payloads = r.payload_handlers();
        let mut channels = payloads.registrar(protocol::VERSION).optional();
        let ask = channels.play_to_server_typed::<Ask>(PayloadId(ASK.into()), server::answer);
        let pages = channels.play_to_client_typed::<Pages>(PayloadId(PAGES.into()), |batch, _| {
            client::received(batch);
            Ok(())
        });
        let _ = ASK_TOKEN.set(ask);
        let _ = PAGES_TOKEN.set(pages);
    }

    /// A read-only copy of each loot table; the table is not changed.
    fn loot_table_load(&self, _: &dyn DataLoadContext, event: &mut LootTableLoadEvent<'_>) {
        server::table_loaded(event.id, event.table);
    }

    fn client_tick_post(&self, host: &mut dyn ClientHost, _: &mut ClientTickPostEvent) {
        client::tick(host);
    }

    fn client_player_logging_out(
        &self,
        _: &mut dyn ClientHost,
        _: &mut ClientPlayerLoggingOutEvent,
    ) {
        client::reset();
    }

    fn screen_render_post(&self, host: &dyn ClientHost, event: &mut ScreenRenderPostEvent<'_>) {
        if host.on(ID) {
            window::draw(host, &mut *event.draw, event.screen);
        }
    }

    fn screen_mouse_button_pressed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseButtonPressedPreEvent<'_>,
    ) -> Flow {
        flow(input::mouse_pressed(host, event.screen, event.button))
    }

    fn screen_mouse_scrolled_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseScrolledPreEvent<'_>,
    ) -> Flow {
        flow(input::mouse_scrolled(host, event.scroll))
    }

    fn screen_key_pressed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenKeyPressedPreEvent<'_>,
    ) -> Flow {
        flow(input::key_pressed(host, event.screen, event.key))
    }
}
