//! GUI mode: eframe/egui app with tabbed SSH sessions.

mod app;
mod dialogs;
mod files;
mod render;
mod search;
mod session;
mod sidebar;
mod tabs;
mod theme;

pub fn run() -> Result<(), String> {
    app::run()
}
