//! Top-level eframe application: tabs, routing, modals, transfers.

use crate::auth;
use crate::config::{Config, Profile, Snippet};
use crate::gui::dialogs::{self, Modal, PaletteAction, PasteAction, UploadAction};
use crate::gui::files::FileTools;
use crate::gui::render;
use crate::gui::search::SearchState;
use crate::gui::session::{PendingConn, Session};
use crate::gui::sidebar::{self, SidebarAction, SidebarState};
use crate::gui::tabs::{tab_bar, TabAction, TabInfo};
use crate::gui::theme;
use crate::ssh_client::{ConnParams, Secret};
use crate::terminal::input::paste_payload;
use crate::transfer;

use std::io::Write as _;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

pub fn run() -> Result<(), String> {
    install_panic_logger();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([700.0, 450.0])
            .with_title("ssh4"),
        ..Default::default()
    };
    eframe::run_native(
        "ssh4",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)) as Box<dyn eframe::App>)),
    )
    .map_err(|e| e.to_string())
}

fn debug_log_path() -> std::path::PathBuf {
    std::env::temp_dir().join("ssh4-debug.log")
}

fn install_panic_logger() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(debug_log_path())
        {
            writeln!(
                f,
                "[{}] PANIC: {info}",
                chrono::Local::now().format("%F %T")
            )
            .ok();
        }
        default(info);
    }));
}

enum TabState {
    Form(PendingConn),
    Active(Session),
}

struct Tab {
    id: u64,
    color: Option<egui::Color32>,
    state: TabState,
    search: SearchState,
    profile_saved: bool,
}

impl Tab {
    fn title(&self) -> String {
        match &self.state {
            TabState::Form(_) => "New connection".to_string(),
            TabState::Active(s) => s.identity.clone(),
        }
    }
}

struct TransferResult {
    tab_id: u64,
    result: Result<String, String>,
    /// Type the remote path into the shell on success.
    type_path: bool,
}

pub struct App {
    config: Config,
    tabs: Vec<Tab>,
    active: usize,
    next_id: u64,
    focus_mode: bool,
    sync_input: bool,
    modal: Modal,
    sidebar: SidebarState,
    transfer_tx: Sender<TransferResult>,
    transfer_rx: Receiver<TransferResult>,
    file_tools: FileTools,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let config = Config::load();
        if let Some(i) = config.theme.as_deref().and_then(theme::index_by_name) {
            theme::set_current(i, &cc.egui_ctx);
        }
        theme::apply(&cc.egui_ctx);
        cc.egui_ctx.set_zoom_factor(config.ui_zoom.clamp(0.7, 2.0));
        let (transfer_tx, transfer_rx) = std::sync::mpsc::channel();
        let sidebar = SidebarState {
            debug_log: false,
            font_size: 14.0,
            ui_zoom: config.ui_zoom,
        };
        let mut app = App {
            config,
            tabs: Vec::new(),
            active: 0,
            next_id: 0,
            focus_mode: false,
            sync_input: false,
            modal: Modal::None,
            sidebar,
            transfer_tx,
            transfer_rx,
            file_tools: FileTools::default(),
        };
        app.new_tab(PendingConn::default());
        app
    }

    fn set_theme(&mut self, index: usize, ctx: &egui::Context) {
        theme::set_current(index, ctx);
        self.config.theme = Some(theme::current().name.to_string());
        self.config.save().ok();
    }

    fn debug_log(&self, msg: &str) {
        if !self.sidebar.debug_log {
            return;
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(debug_log_path())
        {
            writeln!(f, "[{}] {msg}", chrono::Local::now().format("%F %T")).ok();
        }
    }

    fn new_tab(&mut self, form: PendingConn) {
        self.next_id += 1;
        self.tabs.push(Tab {
            id: self.next_id,
            color: None,
            state: TabState::Form(form),
            search: SearchState::default(),
            profile_saved: false,
        });
        self.active = self.tabs.len() - 1;
    }

    fn close_tab(&mut self, i: usize) {
        if i < self.tabs.len() {
            self.tabs.remove(i); // Session::drop sets the worker stop flag
        }
        if self.tabs.is_empty() {
            self.new_tab(PendingConn::default());
        }
        self.active = self.active.min(self.tabs.len() - 1);
    }

    /// Build connection params from a form and spawn a session in-place.
    fn connect_from_form(&mut self, tab_index: usize, ctx: &egui::Context) {
        let Some(tab) = self.tabs.get_mut(tab_index) else {
            return;
        };
        let TabState::Form(form) = &mut tab.state else {
            return;
        };
        let spec = auth::parse_host(form.host.trim());
        if spec.host.is_empty() {
            form.error = Some("Enter a host".to_string());
            return;
        }
        let user = if spec.user.is_empty() {
            auth::local_username()
        } else {
            spec.user.clone()
        };
        let key_path = if form.key_path.trim().is_empty() {
            auth::discover_default_key().map(|p| p.to_string_lossy().to_string())
        } else {
            Some(form.key_path.trim().to_string())
        };
        let password = if form.password.is_empty() {
            None
        } else {
            Some(Secret::new(form.password.clone()))
        };
        if key_path.is_none() && password.is_none() {
            form.error = Some("No password given and no default key found".to_string());
            return;
        }
        let params = ConnParams {
            host: spec.host,
            port: spec.port,
            user,
            password,
            key_path,
            timeout_secs: 15,
            verbose: false,
        };
        form.connecting = true;
        form.error = None;
        let session = Session::connect(
            params,
            "/tmp".to_string(),
            80,
            24,
            self.config.keep_alive,
            ctx.clone(),
        );
        let identity = session.identity.clone();
        tab.state = TabState::Active(session);
        tab.profile_saved = false;
        tab.search = SearchState::default();
        self.debug_log(&format!("connecting to {identity}"));
    }

    fn connect_profile(&mut self, name: &str, ctx: &egui::Context) {
        let Some(p) = self.config.profiles.get(name) else {
            return;
        };
        let params = ConnParams {
            host: p.host.clone(),
            port: p.port,
            user: p.user.clone(),
            password: p.password.clone().map(Secret::new),
            key_path: p.key_path.clone(),
            timeout_secs: 15,
            verbose: false,
        };
        let remote_dir = p.remote_dir.clone().unwrap_or_else(|| "/tmp".to_string());
        let session = Session::connect(
            params,
            remote_dir,
            80,
            24,
            self.config.keep_alive,
            ctx.clone(),
        );
        let form = PendingConn {
            host: format!("{}@{}:{}", p.user, p.host, p.port),
            key_path: p.key_path.clone().unwrap_or_default(),
            save_as: name.to_string(),
            ..Default::default()
        };
        self.new_tab(form);
        let tab = self.tabs.last_mut().unwrap();
        tab.state = TabState::Active(session);
        tab.profile_saved = true; // already a profile
    }

    /// Auto-save a profile after the first successful connect (FR-004).
    fn maybe_save_profile(&mut self, tab_index: usize) {
        let Some(tab) = self.tabs.get_mut(tab_index) else {
            return;
        };
        if tab.profile_saved {
            return;
        }
        let TabState::Active(session) = &tab.state else {
            return;
        };
        if !session.connected {
            return;
        }
        tab.profile_saved = true;
        let name = Config::default_profile_name(&session.params.user, &session.params.host);
        self.config.profiles.insert(
            name,
            Profile {
                host: session.params.host.clone(),
                port: session.params.port,
                user: session.params.user.clone(),
                key_path: session.params.key_path.clone(),
                remote_dir: Some(session.remote_dir.clone()),
                password: None, // explicit opt-in only
            },
        );
        self.config.save().ok();
    }

    /// Move a disconnected session back to a pre-filled connection form.
    fn handle_disconnects(&mut self) {
        for tab in &mut self.tabs {
            let TabState::Active(session) = &tab.state else {
                continue;
            };
            if !session.disconnected {
                continue;
            }
            let p = &session.params;
            let form = PendingConn {
                host: format!("{}@{}:{}", p.user, p.host, p.port),
                key_path: p.key_path.clone().unwrap_or_default(),
                error: session
                    .error
                    .clone()
                    .or_else(|| Some(format!("Disconnected from {}", session.identity))),
                ..Default::default()
            };
            tab.state = TabState::Form(form);
        }
    }

    fn active_session(&mut self) -> Option<&mut Session> {
        match self.tabs.get_mut(self.active).map(|t| &mut t.state) {
            Some(TabState::Active(s)) => Some(s),
            _ => None,
        }
    }

    fn upload_clipboard_image(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let tab_id = tab.id;
        let TabState::Active(session) = &tab.state else {
            return;
        };
        let params = session.params.clone();
        let dir = session.remote_dir.clone();
        let tx = self.transfer_tx.clone();
        std::thread::spawn(move || {
            let result = transfer::upload_clipboard_image(&params, &dir);
            tx.send(TransferResult {
                tab_id,
                result,
                type_path: true,
            })
            .ok();
        });
        if let Some(s) = self.active_session() {
            s.set_status("Uploading clipboard image…", true);
        }
    }

    fn upload_dropped(&mut self, paths: Vec<std::path::PathBuf>, folder: String) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let tab_id = tab.id;
        let TabState::Active(session) = &tab.state else {
            return;
        };
        let params = session.params.clone();
        let tx = self.transfer_tx.clone();
        std::thread::spawn(move || {
            // Dropped files target /tmp, matching current shipped behavior.
            let result = transfer::upload_dropped_files(&params, &paths, &folder, "/tmp");
            tx.send(TransferResult {
                tab_id,
                result,
                type_path: true,
            })
            .ok();
        });
        if let Some(s) = self.active_session() {
            s.set_status("Uploading dropped files…", true);
        }
    }

    fn drain_transfer_results(&mut self) {
        while let Ok(res) = self.transfer_rx.try_recv() {
            let Some(tab) = self.tabs.iter_mut().find(|t| t.id == res.tab_id) else {
                continue;
            };
            let TabState::Active(session) = &mut tab.state else {
                continue;
            };
            match res.result {
                Ok(remote) => {
                    session.set_status(format!("Uploaded: {remote} (copied)"), true);
                    if res.type_path {
                        session.send_input(remote.into_bytes());
                    }
                }
                Err(e) => session.set_status(format!("Upload failed: {e}"), false),
            }
        }
    }

    fn broadcast_input(&mut self, from_tab: usize, bytes: &[Vec<u8>]) {
        if !self.sync_input || bytes.is_empty() {
            return;
        }
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            if i == from_tab {
                continue;
            }
            if let TabState::Active(s) = &mut tab.state {
                if s.connected {
                    for b in bytes {
                        s.send_input(b.clone());
                    }
                }
            }
        }
    }

    fn status_bar(ui: &mut egui::Ui, session: &Session) {
        ui.horizontal(|ui| {
            let dot = if session.connected { "●" } else { "○" };
            let color = if session.connected {
                theme::current().success
            } else {
                theme::current().text_dim
            };
            ui.colored_label(color, dot);
            ui.label(&session.identity);
            ui.weak(format!("{}×{}", session.cols, session.rows));
            ui.weak(format!(
                "↑{} ↓{}",
                dialogs::human_size(session.sent),
                dialogs::human_size(session.received)
            ));
            ui.weak(format!(
                "idle {}s",
                session.last_data_at.elapsed().as_secs()
            ));
            if let Some(hb) = &session.heartbeat {
                ui.weak(format!(
                    "hb {}s ago · q{} · buf{} · to{}",
                    hb.at.elapsed().as_secs(),
                    hb.pending_chunks,
                    hb.buffered_output,
                    hb.read_timeouts
                ));
            }
            if let Some((msg, is_ok, at)) = &session.status {
                if at.elapsed() < Duration::from_secs(6) {
                    let c = if *is_ok {
                        theme::current().success
                    } else {
                        theme::current().error
                    };
                    ui.colored_label(c, msg);
                }
            }
        });
    }

    fn find_bar(ui: &mut egui::Ui, search: &mut SearchState, session: &mut Session) {
        ui.horizontal(|ui| {
            ui.label("Find:");
            let edit = ui.add(egui::TextEdit::singleline(&mut search.query).desired_width(200.0));
            if search.open && !edit.has_focus() && search.query.is_empty() {
                edit.request_focus();
            }
            search.refresh(session);
            let shift = ui.input(|i| i.modifiers.shift);
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                if shift {
                    search.prev();
                } else {
                    search.next();
                }
                search.reveal_active(session);
            }
            if ui.button("◀").clicked() {
                search.prev();
                search.reveal_active(session);
            }
            if ui.button("▶").clicked() {
                search.next();
                search.reveal_active(session);
            }
            let mut cs = search.case_sensitive;
            if ui
                .checkbox(&mut cs, "Aa")
                .on_hover_text("Case sensitive")
                .clicked()
            {
                search.case_sensitive = cs;
                search.invalidate();
            }
            if search.matches.is_empty() {
                ui.weak(if search.query.is_empty() {
                    String::new()
                } else {
                    "No matches".to_string()
                });
            } else {
                ui.weak(format!("{}/{}", search.active + 1, search.matches.len()));
            }
            if ui.button("✕").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                search.open = false;
            }
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll sessions; request another repaint when frame limits were hit.
        let mut hit_limit = false;
        for tab in &mut self.tabs {
            if let TabState::Active(s) = &mut tab.state {
                if s.poll() {
                    hit_limit = true;
                }
            }
        }
        if hit_limit {
            ctx.request_repaint();
        }
        for i in 0..self.tabs.len() {
            self.maybe_save_profile(i);
        }
        self.handle_disconnects();
        self.drain_transfer_results();

        // Global shortcuts.
        let modal_open = self.modal.is_open();
        if ctx.input(|i| i.key_pressed(egui::Key::F11)) {
            self.focus_mode = !self.focus_mode;
        }
        if ctx.input(|i| i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::F)) {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                tab.search.open = !tab.search.open;
            }
        }
        if ctx.input(|i| i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::P)) {
            self.modal = Modal::TabSearch {
                query: String::new(),
            };
        }
        if ctx.input(|i| i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::K)) {
            self.modal = Modal::Palette {
                query: String::new(),
                selected: 0,
            };
        }

        // Dropped files open the upload confirmation.
        let dropped: Vec<std::path::PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if !dropped.is_empty() && !modal_open && self.active_session().is_some() {
            self.modal = Modal::UploadConfirm {
                paths: dropped,
                folder: format!("upload_{}", chrono::Local::now().format("%H%M%S")),
            };
        }

        // Sidebar.
        if !self.focus_mode {
            let mut action = SidebarAction::None;
            egui::SidePanel::left("sidebar")
                .resizable(true)
                .default_width(200.0)
                .max_width(400.0)
                .show(ctx, |ui| {
                    action = sidebar::sidebar_ui(ui, &self.config, &mut self.sidebar);
                });
            match action {
                SidebarAction::ConnectProfile(name) => self.connect_profile(&name, ctx),
                SidebarAction::DeleteProfile(name) => {
                    self.config.profiles.remove(&name);
                    self.config.save().ok();
                }
                SidebarAction::RunSnippet(cmd) => {
                    if let Some(s) = self.active_session() {
                        let mut bytes = cmd.into_bytes();
                        bytes.push(b'\n');
                        s.send_input(bytes);
                    }
                }
                SidebarAction::DeleteSnippet(i) => {
                    if i < self.config.snippets.len() {
                        self.config.snippets.remove(i);
                        self.config.save().ok();
                    }
                }
                SidebarAction::AddSnippet => {
                    self.modal = Modal::AddSnippet {
                        name: String::new(),
                        command: String::new(),
                    };
                }
                SidebarAction::OpenHelp => self.modal = Modal::Help,
                SidebarAction::ToggleDebugLog => {
                    self.sidebar.debug_log = !self.sidebar.debug_log;
                }
                SidebarAction::OpenLogFolder => {
                    #[cfg(windows)]
                    std::process::Command::new("explorer")
                        .arg(std::env::temp_dir())
                        .spawn()
                        .ok();
                }
                SidebarAction::OpenFileTools => self.file_tools.open = true,
                SidebarAction::ToggleKeepAlive => {
                    self.config.keep_alive = !self.config.keep_alive;
                    self.config.save().ok();
                    let on = self.config.keep_alive;
                    for tab in &mut self.tabs {
                        if let TabState::Active(s) = &mut tab.state {
                            s.set_keep_alive(on);
                        }
                    }
                }
                SidebarAction::SetTheme(i) => self.set_theme(i, ctx),
                SidebarAction::None => {}
            }
            // Persist display prefs when they change.
            if (self.sidebar.ui_zoom - self.config.ui_zoom).abs() > f32::EPSILON {
                self.config.ui_zoom = self.sidebar.ui_zoom;
                ctx.set_zoom_factor(self.sidebar.ui_zoom);
                self.config.save().ok();
            }
        }

        // Tab bar.
        let infos: Vec<TabInfo> = self
            .tabs
            .iter()
            .map(|t| TabInfo {
                title: t.title(),
                color: t.color,
                connected: matches!(&t.state, TabState::Active(s) if s.connected),
            })
            .collect();
        let mut tab_action = TabAction::None;
        egui::TopBottomPanel::top("tabbar").show(ctx, |ui| {
            tab_action = tab_bar(ui, &infos, self.active, self.sync_input, self.focus_mode);
        });
        match tab_action {
            TabAction::Select(i) => self.active = i,
            TabAction::New => self.new_tab(PendingConn::default()),
            TabAction::Close(i) => self.close_tab(i),
            TabAction::CloseOthers(i) => {
                let keep = self.tabs.remove(i);
                self.tabs.clear();
                self.tabs.push(keep);
                self.active = 0;
            }
            TabAction::CloseRight(i) => {
                self.tabs.truncate(i + 1);
                self.active = self.active.min(i);
            }
            TabAction::SetColor(i, c) => {
                if let Some(t) = self.tabs.get_mut(i) {
                    t.color = c;
                }
            }
            TabAction::ToggleSyncInput => self.sync_input = !self.sync_input,
            TabAction::ToggleFocusMode => self.focus_mode = !self.focus_mode,
            TabAction::OpenTabSearch => {
                self.modal = Modal::TabSearch {
                    query: String::new(),
                }
            }
            TabAction::None => {}
        }

        // Central panel: connection form or terminal.
        let active = self.active;
        let modal_open = self.modal.is_open();
        let mut connect_requested = false;
        let mut term_out = render::TermOutput::default();
        let font_size = self.sidebar.font_size;

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(theme::current().surface_bg))
            .show(ctx, |ui| {
                let Some(tab) = self.tabs.get_mut(active) else {
                    return;
                };
                match &mut tab.state {
                    TabState::Form(form) => {
                        connect_requested = dialogs::connection_form(ui, form);
                    }
                    TabState::Active(session) => {
                        Self::status_bar(ui, session);
                        if tab.search.open {
                            Self::find_bar(ui, &mut tab.search, session);
                        }
                        ui.separator();
                        let input_enabled = !modal_open && !tab.search.open;
                        term_out = render::terminal_ui(
                            ui,
                            session,
                            &mut tab.search,
                            font_size,
                            input_enabled,
                        );
                    }
                }
            });

        if connect_requested {
            self.connect_from_form(active, ctx);
        }
        self.broadcast_input(active, &term_out.input_sent);
        if term_out.upload_image {
            self.upload_clipboard_image();
        }
        if let Some(text) = term_out.paste_dialog {
            self.modal = Modal::Paste { text };
        }

        // Modals.
        match &mut self.modal {
            Modal::None => {}
            Modal::Help => {
                if !dialogs::help_overlay(ctx) {
                    self.modal = Modal::None;
                }
            }
            Modal::AddSnippet { name, command } => {
                let mut cancelled = false;
                let result = dialogs::add_snippet_dialog(ctx, name, command, &mut cancelled);
                if let Some((name, command)) = result {
                    self.config.snippets.push(Snippet { name, command });
                    self.config.save().ok();
                    self.modal = Modal::None;
                } else if cancelled {
                    self.modal = Modal::None;
                }
            }
            Modal::Paste { text } => {
                let action = dialogs::paste_dialog(ctx, text);
                let text = text.clone();
                match action {
                    PasteAction::Send => {
                        if let Some(s) = self.active_session() {
                            let payload = paste_payload(&text, s.out_parser.bracketed_paste);
                            s.send_input(payload);
                        }
                        self.modal = Modal::None;
                    }
                    PasteAction::SendLineByLine => {
                        if let Some(s) = self.active_session() {
                            for line in text.lines() {
                                let mut bytes = line.as_bytes().to_vec();
                                bytes.push(b'\r');
                                s.send_input(bytes);
                            }
                        }
                        self.modal = Modal::None;
                    }
                    PasteAction::Cancel => self.modal = Modal::None,
                    PasteAction::None => {}
                }
            }
            Modal::UploadConfirm { paths, folder } => {
                let action = dialogs::upload_confirm_dialog(ctx, paths, folder);
                match action {
                    UploadAction::Confirm => {
                        let paths = paths.clone();
                        let folder = folder.clone();
                        self.modal = Modal::None;
                        self.upload_dropped(paths, folder);
                    }
                    UploadAction::Cancel => self.modal = Modal::None,
                    UploadAction::None => {}
                }
            }
            Modal::TabSearch { query } => {
                let mut close = false;
                let mut select: Option<usize> = None;
                egui::Window::new("Search Tabs")
                    .collapsible(false)
                    .anchor(egui::Align2::CENTER_TOP, [0.0, 60.0])
                    .show(ctx, |ui| {
                        let edit = ui.text_edit_singleline(query);
                        edit.request_focus();
                        let q = query.to_lowercase();
                        let matches: Vec<(usize, String)> = infos
                            .iter()
                            .enumerate()
                            .filter(|(_, t)| q.is_empty() || t.title.to_lowercase().contains(&q))
                            .map(|(i, t)| (i, t.title.clone()))
                            .collect();
                        for (i, title) in &matches {
                            if ui.selectable_label(false, title).clicked() {
                                select = Some(*i);
                            }
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            select = matches.first().map(|(i, _)| *i);
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            close = true;
                        }
                    });
                if let Some(i) = select {
                    self.active = i;
                    close = true;
                }
                if close {
                    self.modal = Modal::None;
                }
            }
            Modal::Palette { query, selected } => {
                let action = dialogs::command_palette(ctx, &self.config, query, selected);
                match action {
                    PaletteAction::None => {}
                    PaletteAction::Close => self.modal = Modal::None,
                    PaletteAction::NewTab => {
                        self.modal = Modal::None;
                        self.new_tab(PendingConn::default());
                    }
                    PaletteAction::CloseTab => {
                        self.modal = Modal::None;
                        self.close_tab(self.active);
                    }
                    PaletteAction::SearchTabs => {
                        self.modal = Modal::TabSearch {
                            query: String::new(),
                        }
                    }
                    PaletteAction::ToggleFocus => {
                        self.focus_mode = !self.focus_mode;
                        self.modal = Modal::None;
                    }
                    PaletteAction::ToggleSync => {
                        self.sync_input = !self.sync_input;
                        self.modal = Modal::None;
                    }
                    PaletteAction::OpenHelp => self.modal = Modal::Help,
                    PaletteAction::OpenFileTools => {
                        self.file_tools.open = true;
                        self.modal = Modal::None;
                    }
                    PaletteAction::SetTheme(i) => {
                        self.set_theme(i, ctx);
                        self.modal = Modal::None;
                    }
                    PaletteAction::RunSnippet(i) => {
                        self.modal = Modal::None;
                        let cmd = self.config.snippets.get(i).map(|s| s.command.clone());
                        if let Some(cmd) = cmd {
                            if let Some(s) = self.active_session() {
                                let mut bytes = cmd.into_bytes();
                                bytes.push(b'\n');
                                s.send_input(bytes);
                            }
                        }
                    }
                }
            }
        }

        // File tools window (TR-001 remote / TR-002 local).
        let session_info = match self.tabs.get(self.active).map(|t| &t.state) {
            Some(TabState::Active(s)) if s.connected => {
                Some((s.params.clone(), s.remote_dir.clone()))
            }
            _ => None,
        };
        self.file_tools
            .ui(ctx, session_info.as_ref().map(|(p, d)| (p, d.as_str())));

        // Keep painting while sessions are live so output stays fresh.
        if self
            .tabs
            .iter()
            .any(|t| matches!(&t.state, TabState::Active(s) if s.connected))
        {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
        let _ = Instant::now();
    }
}
