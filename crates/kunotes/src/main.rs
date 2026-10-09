//! KuNotes entry point: sets up logging and starts the GPUI app.
//! Everything else happens in `app.rs`.

mod actions;
mod app;
mod assets;
mod platform;
mod settings_store;
mod ui;
mod vault_store;
#[cfg(test)]
mod vault_store_tests;
mod watcher;

fn main() {
    // Default to warnings only; use RUST_LOG=kunotes=debug for more detail.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    gpui_kit::application()
        .with_assets(assets::AppAssets)
        .run(app::run);
}
