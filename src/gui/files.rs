//! File tools window: remote pane (TR-001, exec+SCP) and local pane (TR-002).
//!
//! Remote operations run on background threads (one-shot SSH connections,
//! same pattern as transfers) and report back over a channel; the UI thread
//! never blocks on the network. Destructive operations always confirm.

use crate::gui::theme::Weight;
use crate::gui::{chrome, dialogs, theme};
use crate::remote_fs::{self, RemoteEntry};
use crate::ssh_client::ConnParams;
use crate::transfer;
use egui::{Color32, Rounding};

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Messages from background file-operation threads.
enum FsMsg {
    Listing {
        dir: String,
        result: Result<Vec<RemoteEntry>, String>,
    },
    Status {
        msg: String,
        ok: bool,
    },
    /// An op finished and the remote listing should refresh.
    Refresh,
}

#[derive(Clone)]
struct LocalEntry {
    name: String,
    is_dir: bool,
    size: u64,
}

/// Pending name-input dialog.
enum NameDialog {
    None,
    NewRemoteFile(String),
    NewRemoteDir(String),
    RenameRemote { from: String, to: String },
    Chmod { path: String, mode: String },
    NewLocalFile(String),
    NewLocalDir(String),
    RenameLocal { from: String, to: String },
}

/// Pending delete confirmation: (is_remote, full path, display name).
struct ConfirmDelete {
    remote: bool,
    path: String,
    name: String,
}

pub struct FileTools {
    pub open: bool,
    remote_dir: String,
    remote_entries: Vec<RemoteEntry>,
    remote_sel: Option<usize>,
    listing: bool,
    local_dir: PathBuf,
    local_entries: Vec<LocalEntry>,
    local_sel: Option<usize>,
    local_dirty: bool,
    status: Option<(String, bool, Instant)>,
    dialog: NameDialog,
    confirm: Option<ConfirmDelete>,
    tx: Sender<FsMsg>,
    rx: Receiver<FsMsg>,
    /// Set on drop; stops all edit-watch threads.
    watch_stop: Arc<AtomicBool>,
}

impl Drop for FileTools {
    fn drop(&mut self) {
        self.watch_stop.store(true, Ordering::Relaxed);
    }
}

impl Default for FileTools {
    fn default() -> Self {
        let (tx, rx) = channel();
        let local_dir = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));
        Self {
            open: false,
            remote_dir: String::new(),
            remote_entries: Vec::new(),
            remote_sel: None,
            listing: false,
            local_dir,
            local_entries: Vec::new(),
            local_sel: None,
            local_dirty: true,
            status: None,
            dialog: NameDialog::None,
            confirm: None,
            tx,
            rx,
            watch_stop: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// A rounded, off-white panel wrapping one pane's content (local/remote).
fn pane_frame(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(Color32::from_rgb(0xfa, 0xfa, 0xfa))
        .rounding(Rounding::same(16.0))
        .inner_margin(egui::Margin::same(12.0))
        .show(ui, add_contents);
}

fn read_local(dir: &Path) -> Vec<LocalEntry> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let meta = entry.metadata().ok();
            out.push(LocalEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                is_dir: meta.as_ref().is_some_and(|m| m.is_dir()),
                size: meta.map(|m| m.len()).unwrap_or(0),
            });
        }
    }
    out.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

impl FileTools {
    fn set_status(&mut self, msg: impl Into<String>, ok: bool) {
        self.status = Some((msg.into(), ok, Instant::now()));
    }

    /// Kick off an async remote listing of `dir`.
    fn list_remote(&mut self, params: &ConnParams, dir: String) {
        self.listing = true;
        self.remote_sel = None;
        let tx = self.tx.clone();
        let params = params.clone();
        std::thread::spawn(move || {
            let result = remote_fs::list(&params, &dir);
            tx.send(FsMsg::Listing { dir, result }).ok();
        });
    }

    /// Run a remote mutation on a thread; refresh the listing afterwards.
    fn run_remote_op(
        &mut self,
        params: &ConnParams,
        done_msg: String,
        op: impl FnOnce(&ConnParams) -> Result<(), String> + Send + 'static,
    ) {
        let tx = self.tx.clone();
        let params = params.clone();
        std::thread::spawn(move || {
            match op(&params) {
                Ok(()) => {
                    tx.send(FsMsg::Status {
                        msg: done_msg,
                        ok: true,
                    })
                    .ok();
                }
                Err(e) => {
                    tx.send(FsMsg::Status { msg: e, ok: false }).ok();
                }
            }
            tx.send(FsMsg::Refresh).ok();
        });
    }

    /// Download a remote file to a temp dir, open it in the system editor,
    /// and re-upload whenever the local copy is saved (TR-001 auto-upload).
    fn edit_remote_file(&mut self, params: &ConnParams, remote: String) {
        let tx = self.tx.clone();
        let params = params.clone();
        let stop = self.watch_stop.clone();
        std::thread::spawn(move || {
            let name = remote.rsplit('/').next().unwrap_or("file").to_string();
            let dir = std::env::temp_dir().join(format!("ssh4_edit_{}", std::process::id()));
            if std::fs::create_dir_all(&dir).is_err() {
                tx.send(FsMsg::Status {
                    msg: "Could not create temp dir".into(),
                    ok: false,
                })
                .ok();
                return;
            }
            let local = dir.join(&name);
            if let Err(e) = remote_fs::download(&params, &remote, &local) {
                tx.send(FsMsg::Status { msg: e, ok: false }).ok();
                return;
            }
            open_with_system(&local);
            tx.send(FsMsg::Status {
                msg: format!("Editing {name}: saving re-uploads to {remote}"),
                ok: true,
            })
            .ok();

            let mut last = std::fs::metadata(&local).and_then(|m| m.modified()).ok();
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_secs(2));
                let now = std::fs::metadata(&local).and_then(|m| m.modified()).ok();
                if now.is_some() && now != last {
                    last = now;
                    let msg = match transfer::scp_upload(&params, &local, &remote) {
                        Ok(()) => FsMsg::Status {
                            msg: format!("Re-uploaded {name}"),
                            ok: true,
                        },
                        Err(e) => FsMsg::Status { msg: e, ok: false },
                    };
                    if tx.send(msg).is_err() {
                        break;
                    }
                }
            }
        });
    }

    fn drain_messages(&mut self, params: Option<&ConnParams>) {
        let mut refresh = false;
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                FsMsg::Listing { dir, result } => {
                    self.listing = false;
                    match result {
                        Ok(entries) => {
                            self.remote_dir = dir;
                            self.remote_entries = entries;
                        }
                        Err(e) => self.set_status(e, false),
                    }
                }
                FsMsg::Status { msg, ok } => {
                    if ok && msg.starts_with("Downloaded") {
                        self.local_dirty = true;
                    }
                    self.set_status(msg, ok);
                }
                FsMsg::Refresh => refresh = true,
            }
        }
        if refresh {
            if let Some(p) = params {
                self.list_remote(p, self.remote_dir.clone());
            }
        }
    }

    /// Render the window. `session` is the active tab's connection (if any).
    pub fn ui(&mut self, ctx: &egui::Context, session: Option<(&ConnParams, &str)>) {
        if !self.open {
            return;
        }
        self.drain_messages(session.map(|(p, _)| p));

        // First open with a live session: land in its remote working dir.
        if self.remote_dir.is_empty() && !self.listing {
            if let Some((params, remote_dir)) = session {
                let dir = if remote_dir.is_empty() {
                    "/".to_string()
                } else {
                    remote_dir.to_string()
                };
                self.remote_dir = dir.clone();
                self.list_remote(params, dir);
            }
        }
        if self.local_dirty {
            self.local_entries = read_local(&self.local_dir.clone());
            self.local_sel = None;
            self.local_dirty = false;
        }

        let t = theme::chrome();
        let mut open = self.open;
        egui::Window::new("File Tools")
            .id(egui::Id::new("file_tools_window"))
            .title_bar(false)
            .default_size([840.0, 480.0])
            .resizable(true)
            .frame(chrome::card_frame(24.0).inner_margin(egui::Margin::same(16.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("File tools")
                            .font(theme::font(Weight::SemiBold, 12.5))
                            .color(t.text_primary),
                    );
                    if let Some((params, _)) = session {
                        ui.label(
                            egui::RichText::new(format!("{}@{}", params.user, params.host))
                                .font(theme::font(Weight::Regular, 12.0))
                                .color(t.text_dim),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✕").on_hover_text("Close").clicked() {
                            open = false;
                        }
                    });
                });
                ui.add_space(10.0);
                ui.columns(2, |cols| {
                    pane_frame(&mut cols[0], |ui| self.local_pane(ui, session));
                    pane_frame(&mut cols[1], |ui| self.remote_pane(ui, session));
                });
                ui.add_space(6.0);
                if let Some((msg, ok, at)) = &self.status {
                    if at.elapsed() < Duration::from_secs(8) {
                        let color = if *ok { t.success } else { t.error };
                        ui.colored_label(color, msg);
                    }
                }
            });
        self.open = open;

        self.name_dialog(ctx, session);
        self.confirm_dialog(ctx, session);
    }

    fn local_pane(&mut self, ui: &mut egui::Ui, session: Option<(&ConnParams, &str)>) {
        theme::section_header(ui, &format!("LOCAL · {}", self.local_dir.display()));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button("⬆").on_hover_text("Parent folder").clicked() {
                if let Some(parent) = self.local_dir.parent().map(Path::to_path_buf) {
                    self.local_dir = parent;
                    self.local_dirty = true;
                }
            }
        });
        ui.add_space(4.0);

        let mut navigate: Option<PathBuf> = None;
        egui::ScrollArea::vertical()
            .id_source("local_list")
            .max_height(280.0)
            .show(ui, |ui| {
                for (i, e) in self.local_entries.iter().enumerate() {
                    let icon = if e.is_dir { "📁" } else { "📄" };
                    let label = if e.is_dir {
                        format!("{icon} {}", e.name)
                    } else {
                        format!("{icon} {}  ({})", e.name, dialogs::human_size(e.size))
                    };
                    let resp = ui.selectable_label(self.local_sel == Some(i), label);
                    if resp.clicked() {
                        self.local_sel = Some(i);
                    }
                    if resp.double_clicked() && e.is_dir {
                        navigate = Some(self.local_dir.join(&e.name));
                    }
                }
                if self.local_entries.is_empty() {
                    ui.weak("(empty)");
                }
            });
        if let Some(dir) = navigate {
            self.local_dir = dir;
            self.local_dirty = true;
        }

        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("New file").clicked() {
                self.dialog = NameDialog::NewLocalFile(String::new());
            }
            if ui.button("New folder").clicked() {
                self.dialog = NameDialog::NewLocalDir(String::new());
            }
            let sel = self
                .local_sel
                .and_then(|i| self.local_entries.get(i))
                .cloned();
            let has_sel = sel.is_some();
            if ui
                .add_enabled(has_sel, egui::Button::new("Rename"))
                .clicked()
            {
                if let Some(e) = &sel {
                    self.dialog = NameDialog::RenameLocal {
                        from: e.name.clone(),
                        to: e.name.clone(),
                    };
                }
            }
            if ui
                .add_enabled(has_sel, egui::Button::new("Copy to…"))
                .clicked()
            {
                if let Some(e) = &sel {
                    let src = self.local_dir.join(&e.name);
                    let is_dir = e.is_dir;
                    if let Some(dst_dir) = rfd::FileDialog::new().pick_folder() {
                        let dst = dst_dir.join(&e.name);
                        let r = if is_dir {
                            transfer::copy_dir(&src, &dst).map_err(|e| e.to_string())
                        } else {
                            std::fs::copy(&src, &dst)
                                .map(|_| ())
                                .map_err(|e| e.to_string())
                        };
                        match r {
                            Ok(()) => self.set_status(format!("Copied to {}", dst.display()), true),
                            Err(e) => self.set_status(format!("Copy failed: {e}"), false),
                        }
                    }
                }
            }
            if ui
                .add_enabled(has_sel, egui::Button::new("Move to…"))
                .clicked()
            {
                if let Some(e) = &sel {
                    let src = self.local_dir.join(&e.name);
                    let name = e.name.clone();
                    if let Some(dst_dir) = rfd::FileDialog::new().pick_folder() {
                        let dst = dst_dir.join(&name);
                        match std::fs::rename(&src, &dst) {
                            Ok(()) => {
                                self.set_status(format!("Moved to {}", dst.display()), true);
                                self.local_dirty = true;
                            }
                            Err(e) => self.set_status(format!("Move failed: {e}"), false),
                        }
                    }
                }
            }
            if ui
                .add_enabled(has_sel, egui::Button::new("Delete"))
                .clicked()
            {
                if let Some(e) = &sel {
                    self.confirm = Some(ConfirmDelete {
                        remote: false,
                        path: self.local_dir.join(&e.name).display().to_string(),
                        name: e.name.clone(),
                    });
                }
            }
            let can_upload = has_sel && session.is_some();
            if ui
                .add_enabled(can_upload, egui::Button::new("Upload ➡"))
                .on_hover_text("Upload to the current remote folder")
                .clicked()
            {
                if let (Some(e), Some((params, _))) = (&sel, session) {
                    let local = self.local_dir.join(&e.name);
                    let remote_dir = self.remote_dir.clone();
                    self.run_remote_op(params, format!("Uploaded {}", e.name), move |p| {
                        remote_fs::upload(p, &local, &remote_dir).map(|_| ())
                    });
                    self.set_status("Uploading…", true);
                }
            }
            if ui.button("⟳").on_hover_text("Refresh").clicked() {
                self.local_dirty = true;
            }
        });
    }

    fn remote_pane(&mut self, ui: &mut egui::Ui, session: Option<(&ConnParams, &str)>) {
        theme::section_header(ui, &format!("REMOTE · {}", self.remote_dir));
        let Some((params, _)) = session else {
            ui.add_space(8.0);
            ui.weak("Open an SSH tab to browse remote files.");
            return;
        };
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            if ui.button("⬆").on_hover_text("Parent folder").clicked() {
                let parent = remote_fs::parent(&self.remote_dir);
                self.list_remote(params, parent);
            }
            if self.listing {
                ui.spinner();
            }
        });
        ui.add_space(4.0);

        let mut navigate: Option<String> = None;
        egui::ScrollArea::vertical()
            .id_source("remote_list")
            .max_height(280.0)
            .show(ui, |ui| {
                for (i, e) in self.remote_entries.iter().enumerate() {
                    let icon = if e.is_dir {
                        "📁"
                    } else if e.is_link {
                        "🔗"
                    } else {
                        "📄"
                    };
                    let label = if e.is_dir {
                        format!("{icon} {}  {}", e.name, e.perms)
                    } else {
                        format!(
                            "{icon} {}  ({})  {}",
                            e.name,
                            dialogs::human_size(e.size),
                            e.perms
                        )
                    };
                    let resp = ui.selectable_label(self.remote_sel == Some(i), label);
                    if resp.clicked() {
                        self.remote_sel = Some(i);
                    }
                    if resp.double_clicked() && e.is_dir {
                        navigate = Some(remote_fs::join(&self.remote_dir, &e.name));
                    }
                }
                if self.remote_entries.is_empty() && !self.listing {
                    ui.weak("(empty)");
                }
            });
        if let Some(dir) = navigate {
            self.list_remote(params, dir);
        }

        ui.add_space(4.0);
        let sel = self
            .remote_sel
            .and_then(|i| self.remote_entries.get(i))
            .cloned();
        let has_sel = sel.is_some();
        ui.horizontal_wrapped(|ui| {
            if ui.button("New file").clicked() {
                self.dialog = NameDialog::NewRemoteFile(String::new());
            }
            if ui.button("New folder").clicked() {
                self.dialog = NameDialog::NewRemoteDir(String::new());
            }
            if ui
                .add_enabled(has_sel, egui::Button::new("Rename"))
                .clicked()
            {
                if let Some(e) = &sel {
                    self.dialog = NameDialog::RenameRemote {
                        from: e.name.clone(),
                        to: e.name.clone(),
                    };
                }
            }
            if ui
                .add_enabled(has_sel, egui::Button::new("Chmod"))
                .clicked()
            {
                if let Some(e) = &sel {
                    self.dialog = NameDialog::Chmod {
                        path: remote_fs::join(&self.remote_dir, &e.name),
                        mode: remote_fs::perms_to_octal(&e.perms),
                    };
                }
            }
            if ui
                .add_enabled(has_sel, egui::Button::new("Delete"))
                .clicked()
            {
                if let Some(e) = &sel {
                    self.confirm = Some(ConfirmDelete {
                        remote: true,
                        path: remote_fs::join(&self.remote_dir, &e.name),
                        name: e.name.clone(),
                    });
                }
            }
            if ui
                .add_enabled(has_sel, egui::Button::new("⬅ Download"))
                .on_hover_text("Download into the current local folder")
                .clicked()
            {
                if let Some(e) = &sel {
                    let remote = remote_fs::join(&self.remote_dir, &e.name);
                    let local = self.local_dir.join(&e.name);
                    let name = e.name.clone();
                    let tx = self.tx.clone();
                    let p = params.clone();
                    std::thread::spawn(move || {
                        let msg = match remote_fs::download(&p, &remote, &local) {
                            Ok(()) => FsMsg::Status {
                                msg: format!("Downloaded {name}"),
                                ok: true,
                            },
                            Err(e) => FsMsg::Status { msg: e, ok: false },
                        };
                        tx.send(msg).ok();
                    });
                    self.set_status("Downloading…", true);
                }
            }
            let can_edit = sel.as_ref().is_some_and(|e| !e.is_dir);
            if ui
                .add_enabled(can_edit, egui::Button::new("Edit"))
                .on_hover_text("Open locally; saving re-uploads automatically")
                .clicked()
            {
                if let Some(e) = &sel {
                    let remote = remote_fs::join(&self.remote_dir, &e.name);
                    self.edit_remote_file(params, remote);
                }
            }
            if ui.button("⟳").on_hover_text("Refresh").clicked() {
                self.list_remote(params, self.remote_dir.clone());
            }
        });
    }

    /// Name-input dialogs (create/rename/chmod).
    fn name_dialog(&mut self, ctx: &egui::Context, session: Option<(&ConnParams, &str)>) {
        if matches!(self.dialog, NameDialog::None) {
            return;
        }
        let title = match &self.dialog {
            NameDialog::NewRemoteFile(_) => "New remote file",
            NameDialog::NewRemoteDir(_) => "New remote folder",
            NameDialog::RenameRemote { .. } => "Rename remote",
            NameDialog::Chmod { .. } => "Change permissions",
            NameDialog::NewLocalFile(_) => "New local file",
            NameDialog::NewLocalDir(_) => "New local folder",
            NameDialog::RenameLocal { .. } => "Rename local",
            NameDialog::None => unreachable!(),
        };
        let mut apply = false;
        let mut cancel = false;
        egui::Window::new(title)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let value = match &mut self.dialog {
                    NameDialog::NewRemoteFile(v)
                    | NameDialog::NewRemoteDir(v)
                    | NameDialog::NewLocalFile(v)
                    | NameDialog::NewLocalDir(v) => v,
                    NameDialog::RenameRemote { to, .. } | NameDialog::RenameLocal { to, .. } => to,
                    NameDialog::Chmod { mode, .. } => mode,
                    NameDialog::None => unreachable!(),
                };
                let edit = ui.text_edit_singleline(value);
                edit.request_focus();
                if let NameDialog::Chmod { mode, .. } = &self.dialog {
                    if !remote_fs::valid_mode(mode) {
                        ui.colored_label(theme::chrome().error, "Octal mode, e.g. 644 or 755");
                    }
                }
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        apply = true;
                    }
                    if ui.button("Cancel").clicked()
                        || ui.input(|i| i.key_pressed(egui::Key::Escape))
                    {
                        cancel = true;
                    }
                });
            });
        if cancel {
            self.dialog = NameDialog::None;
            return;
        }
        if !apply {
            return;
        }
        let dialog = std::mem::replace(&mut self.dialog, NameDialog::None);
        match dialog {
            NameDialog::NewLocalFile(name) if !name.trim().is_empty() => {
                let path = self.local_dir.join(name.trim());
                if path.exists() {
                    self.set_status("Already exists", false);
                } else {
                    match std::fs::File::create(&path) {
                        Ok(_) => self.local_dirty = true,
                        Err(e) => self.set_status(format!("Create failed: {e}"), false),
                    }
                }
            }
            NameDialog::NewLocalDir(name) if !name.trim().is_empty() => {
                match std::fs::create_dir_all(self.local_dir.join(name.trim())) {
                    Ok(()) => self.local_dirty = true,
                    Err(e) => self.set_status(format!("Create failed: {e}"), false),
                }
            }
            NameDialog::RenameLocal { from, to } if !to.trim().is_empty() => {
                match std::fs::rename(self.local_dir.join(&from), self.local_dir.join(to.trim())) {
                    Ok(()) => self.local_dirty = true,
                    Err(e) => self.set_status(format!("Rename failed: {e}"), false),
                }
            }
            NameDialog::NewRemoteFile(name) if !name.trim().is_empty() => {
                if let Some((params, _)) = session {
                    let path = remote_fs::join(&self.remote_dir, name.trim());
                    self.run_remote_op(params, format!("Created {name}"), move |p| {
                        remote_fs::touch(p, &path)
                    });
                }
            }
            NameDialog::NewRemoteDir(name) if !name.trim().is_empty() => {
                if let Some((params, _)) = session {
                    let path = remote_fs::join(&self.remote_dir, name.trim());
                    self.run_remote_op(params, format!("Created {name}/"), move |p| {
                        remote_fs::mkdir(p, &path)
                    });
                }
            }
            NameDialog::RenameRemote { from, to } if !to.trim().is_empty() => {
                if let Some((params, _)) = session {
                    let a = remote_fs::join(&self.remote_dir, &from);
                    let b = remote_fs::join(&self.remote_dir, to.trim());
                    self.run_remote_op(params, format!("Renamed {from}"), move |p| {
                        remote_fs::rename(p, &a, &b)
                    });
                }
            }
            NameDialog::Chmod { path, mode } => {
                if let Some((params, _)) = session {
                    let display = format!("chmod {mode} applied");
                    self.run_remote_op(params, display, move |p| remote_fs::chmod(p, &mode, &path));
                }
            }
            _ => {}
        }
    }

    /// Delete confirmation (destructive ops always confirm).
    fn confirm_dialog(&mut self, ctx: &egui::Context, session: Option<(&ConnParams, &str)>) {
        let Some(confirm) = &self.confirm else {
            return;
        };
        let (remote, path, name) = (confirm.remote, confirm.path.clone(), confirm.name.clone());
        let mut done = false;
        egui::Window::new("Confirm Delete")
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let kind = if remote { "REMOTE" } else { "local" };
                ui.label(format!("Permanently delete {kind} path?"));
                ui.monospace(&path);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let del = egui::Button::new(
                        egui::RichText::new("Delete").color(theme::chrome().error),
                    );
                    if ui.add(del).clicked() {
                        done = true;
                        if remote {
                            if let Some((params, _)) = session {
                                self.run_remote_op(params, format!("Deleted {name}"), move |p| {
                                    remote_fs::delete(p, &path)
                                });
                            }
                        } else {
                            let p = PathBuf::from(&path);
                            let r = if p.is_dir() {
                                std::fs::remove_dir_all(&p)
                            } else {
                                std::fs::remove_file(&p)
                            };
                            match r {
                                Ok(()) => self.local_dirty = true,
                                Err(e) => self.set_status(format!("Delete failed: {e}"), false),
                            }
                        }
                    }
                    if ui.button("Cancel").clicked()
                        || ui.input(|i| i.key_pressed(egui::Key::Escape))
                    {
                        done = true;
                    }
                });
            });
        if done {
            self.confirm = None;
        }
    }
}

fn open_with_system(path: &Path) {
    #[cfg(windows)]
    std::process::Command::new("cmd")
        .args(["/C", "start", ""])
        .arg(path)
        .spawn()
        .ok();
    #[cfg(target_os = "macos")]
    std::process::Command::new("open").arg(path).spawn().ok();
    #[cfg(all(unix, not(target_os = "macos")))]
    std::process::Command::new("xdg-open")
        .arg(path)
        .spawn()
        .ok();
}
