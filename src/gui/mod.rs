//! GUI mode: eframe/egui app with tabbed SSH sessions.

mod app;
mod chrome;
mod dialogs;
mod dock;
mod files;
mod icons;
mod render;
mod search;
mod session;
mod tabs;
mod theme;

pub fn run() -> Result<(), String> {
    app::run()
}
