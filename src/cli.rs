//! CLI mode: argument parsing, credential resolution, and the interactive
//! raw-mode terminal loop.

use crate::auth;
use crate::config::{Config, Profile};
use crate::ssh_client::{self, ConnParams, Secret, READ_TIMEOUT, SSH_WRITE_CHUNK};
use crate::terminal::input::{key_to_bytes, paste_payload};
use crate::terminal::mouse::{self, MouseButton};
use crate::terminal::output::{cpr_response, OutputParser, RemoteOutputEvent, Utf8Carry};
use crate::terminal::selection::{CellPos, Selection};
use crate::transfer;

use clap::Parser;
use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    MouseEventKind,
};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Parser, Debug, Clone)]
#[command(
    name = "ssh4",
    about = "Rust SSH client with GUI and CLI modes",
    version
)]
pub struct Cli {
    /// Remote target: [user@]host[:port]
    pub host: Option<String>,

    /// Connect using a saved profile from ~/.ssh4.toml
    #[arg(short = 'P', long)]
    pub profile: Option<String>,

    /// Username (overrides user@host and profile)
    #[arg(short, long)]
    pub user: Option<String>,

    /// Private key path
    #[arg(short, long)]
    pub key: Option<String>,

    /// Password (warning: visible in process list and shell history)
    #[arg(long)]
    pub password: Option<String>,

    /// Remote directory for uploads
    #[arg(long, default_value = "/tmp")]
    pub remote_dir: String,

    /// Upload clipboard image over SCP and exit
    #[arg(long)]
    pub once: bool,

    /// Append raw remote output to a log file
    #[arg(long)]
    pub log: Option<String>,

    /// Do not prompt to reconnect after disconnect
    #[arg(long)]
    pub no_reconnect: bool,

    /// Save this connection as a named profile
    #[arg(long)]
    pub save_profile: Option<String>,

    /// Connection timeout in seconds
    #[arg(long, default_value_t = 15)]
    pub timeout: u64,

    /// Force GUI mode
    #[arg(long)]
    pub gui: bool,

    /// Verbose connection output
    #[arg(short, long)]
    pub verbose: bool,
}

/// Resolve CLI args + profile + discovery + prompts into connection params.
fn resolve_params(args: &Cli, config: &Config) -> Result<(ConnParams, String), String> {
    let mut params = ConnParams {
        timeout_secs: args.timeout,
        verbose: args.verbose,
        ..Default::default()
    };
    let mut remote_dir = args.remote_dir.clone();

    if let Some(name) = &args.profile {
        let p = config
            .profiles
            .get(name)
            .ok_or_else(|| format!("Profile '{name}' not found in ~/.ssh4.toml"))?;
        params.host = p.host.clone();
        params.port = p.port;
        params.user = p.user.clone();
        params.key_path = p.key_path.clone();
        params.password = p.password.clone().map(Secret::new);
        if let Some(dir) = &p.remote_dir {
            remote_dir = dir.clone();
        }
    }

    if let Some(host) = &args.host {
        let spec = auth::parse_host(host);
        params.host = spec.host;
        params.port = spec.port;
        if !spec.user.is_empty() {
            params.user = spec.user;
        }
    }

    if let Some(u) = &args.user {
        params.user = u.clone();
    }
    if let Some(k) = &args.key {
        params.key_path = Some(k.clone());
    }
    if let Some(pw) = &args.password {
        eprintln!("warning: --password is visible to other processes; prefer keys or the prompt");
        params.password = Some(Secret::new(pw.clone()));
    }

    if params.host.is_empty() {
        return Err("No host given (use HOST or -P PROFILE)".to_string());
    }
    if params.user.is_empty() {
        params.user = auth::prompt_line(&format!("Username [{}]: ", auth::local_username()))
            .unwrap_or_default();
        if params.user.is_empty() {
            params.user = auth::local_username();
        }
    }
    if params.key_path.is_none() && params.password.is_none() {
        if let Some(key) = auth::discover_default_key() {
            if args.verbose {
                eprintln!("Using default key: {}", key.display());
            }
            params.key_path = Some(key.to_string_lossy().to_string());
        } else {
            let pw =
                auth::prompt_password(&format!("Password for {}@{}: ", params.user, params.host))
                    .map_err(|e| e.to_string())?;
            params.password = Some(Secret::new(pw));
        }
    }

    Ok((params, remote_dir))
}

/// Switch the console to the UTF-8 codepage so CJK output renders instead of
/// mojibake. Windows Terminal already defaults to UTF-8; legacy conhost often
/// sits on a locale codepage like 936/932.
#[cfg(windows)]
fn enable_utf8_console() {
    #[link(name = "kernel32")]
    extern "system" {
        fn SetConsoleOutputCP(codepage: u32) -> i32;
        fn SetConsoleCP(codepage: u32) -> i32;
    }
    const CP_UTF8: u32 = 65001;
    unsafe {
        SetConsoleOutputCP(CP_UTF8);
        SetConsoleCP(CP_UTF8);
    }
}

pub fn run(args: Cli) -> Result<(), String> {
    #[cfg(windows)]
    enable_utf8_console();
    let mut config = Config::load();
    let (params, remote_dir) = resolve_params(&args, &config)?;

    if let Some(name) = &args.save_profile {
        config.profiles.insert(
            name.clone(),
            Profile {
                host: params.host.clone(),
                port: params.port,
                user: params.user.clone(),
                key_path: params.key_path.clone(),
                remote_dir: Some(remote_dir.clone()),
                password: None, // never persist passwords from CLI flow
            },
        );
        config
            .save()
            .map_err(|e| format!("Could not save profile: {e}"))?;
        println!("Saved profile '{name}'");
    }

    if args.once {
        println!(
            "Uploading clipboard image to {}:{remote_dir} ...",
            params.host
        );
        let remote = transfer::upload_clipboard_image(&params, &remote_dir)?;
        println!("Uploaded: {remote} (path copied to clipboard)");
        return Ok(());
    }

    loop {
        let summary = run_session(&params, &remote_dir, args.log.as_deref())?;
        println!(
            "\nDisconnected from {} after {} (sent {} B, received {} B)",
            params.identity(),
            format_duration(summary.duration),
            summary.sent,
            summary.received
        );
        if args.no_reconnect {
            break;
        }
        let answer = auth::prompt_line("Reconnect? [Y/n]: ").unwrap_or_default();
        if answer.eq_ignore_ascii_case("n") || answer.eq_ignore_ascii_case("no") {
            break;
        }
    }
    Ok(())
}

fn format_duration(d: Duration) -> String {
    let s = d.as_secs();
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

struct SessionSummary {
    duration: Duration,
    sent: u64,
    received: u64,
}

/// Guard that restores the local terminal even on early returns or panics.
struct RawModeGuard;

impl RawModeGuard {
    fn enable() -> Result<Self, String> {
        crossterm::terminal::enable_raw_mode().map_err(|e| e.to_string())?;
        crossterm::execute!(std::io::stdout(), EnableMouseCapture).map_err(|e| e.to_string())?;
        Ok(RawModeGuard)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        crossterm::execute!(std::io::stdout(), DisableMouseCapture).ok();
        crossterm::terminal::disable_raw_mode().ok();
    }
}

enum CliEvent {
    Input(Vec<u8>),
    Resize(u16, u16),
    Mouse(crossterm::event::MouseEvent),
    PasteRequest,
    UploadRequest,
}

fn spawn_input_thread(tx: Sender<CliEvent>, stop: Arc<AtomicBool>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            match crossterm::event::poll(Duration::from_millis(50)) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(_) => break,
            }
            let Ok(ev) = crossterm::event::read() else {
                break;
            };
            let sent = match ev {
                Event::Key(k) if k.kind != KeyEventKind::Release => {
                    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
                    match k.code {
                        KeyCode::Char('v') if ctrl => tx.send(CliEvent::PasteRequest),
                        KeyCode::Char('p') if ctrl => tx.send(CliEvent::UploadRequest),
                        _ => match key_to_bytes(&k) {
                            Some(bytes) => tx.send(CliEvent::Input(bytes)),
                            None => Ok(()),
                        },
                    }
                }
                Event::Resize(c, r) => tx.send(CliEvent::Resize(c, r)),
                Event::Mouse(m) => tx.send(CliEvent::Mouse(m)),
                Event::Paste(text) => tx.send(CliEvent::Input(text.into_bytes())),
                _ => Ok(()),
            };
            if sent.is_err() {
                break;
            }
        }
    })
}

fn run_session(
    params: &ConnParams,
    remote_dir: &str,
    log_path: Option<&str>,
) -> Result<SessionSummary, String> {
    println!("Connecting to {} ...", params.identity());
    let mut session = ssh_client::connect_session(params)?;
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    if params.verbose {
        println!("Connected; opening {cols}x{rows} PTY shell");
    }
    let mut shell = ssh_client::open_shell(&mut session, cols, rows)?;
    session.set_timeout(Some(READ_TIMEOUT));

    let mut logger = match log_path {
        Some(p) => Some(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
                .map_err(|e| format!("Could not open log file: {e}"))?,
        ),
        None => None,
    };

    let _guard = RawModeGuard::enable()?;
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx): (Sender<CliEvent>, Receiver<CliEvent>) = std::sync::mpsc::channel();
    let input_thread = spawn_input_thread(tx.clone(), stop.clone());

    let start = Instant::now();
    let mut sent: u64 = 0;
    let mut received: u64 = 0;
    let mut parser = OutputParser::new();
    // Defers a CJK char split across SSH reads so stdout never sees half of
    // a UTF-8 sequence (legacy conhost renders split sequences as garbage).
    let mut utf8_carry = Utf8Carry::default();
    // Mirror of the remote screen used for CPR answers and mouse selection.
    let mut screen = vt100::Parser::new(rows, cols, 0);
    let mut selection = Selection::default();
    let mut stdout = std::io::stdout();

    let result: Result<(), String> = (|| {
        loop {
            // Drain local input.
            while let Ok(ev) = rx.try_recv() {
                match ev {
                    CliEvent::Input(bytes) => {
                        if selection.is_active() && bytes == b"\r" {
                            // Enter copies an active selection instead.
                            copy_selection(&selection, &screen);
                            selection.clear();
                            continue;
                        }
                        if bytes == [0x1b] && selection.is_active() {
                            selection.clear();
                            continue;
                        }
                        write_chunked(&mut shell, &bytes)?;
                        sent += bytes.len() as u64;
                    }
                    CliEvent::Resize(c, r) => {
                        shell
                            .window_change(ssh::TerminalSize::from(c as u32, r as u32))
                            .map_err(|e| ssh_client::friendly_error(&e))?;
                        screen.set_size(r, c);
                    }
                    CliEvent::Mouse(m) => {
                        if let Some(bytes) =
                            handle_mouse(&m, parser.mouse_mode, &mut selection, &screen)
                        {
                            write_chunked(&mut shell, &bytes)?;
                            sent += bytes.len() as u64;
                        }
                    }
                    CliEvent::PasteRequest => {
                        if let Ok(mut cb) = arboard::Clipboard::new() {
                            if let Ok(text) = cb.get_text() {
                                let payload = paste_payload(&text, parser.bracketed_paste);
                                write_chunked(&mut shell, &payload)?;
                                sent += payload.len() as u64;
                            }
                        }
                    }
                    CliEvent::UploadRequest => {
                        notify(&mut stdout, "Uploading clipboard image...", true);
                        match transfer::upload_clipboard_image(params, remote_dir) {
                            Ok(remote) => {
                                notify(&mut stdout, &format!("Uploaded: {remote}"), true);
                                // Type the remote path into the shell.
                                write_chunked(&mut shell, remote.as_bytes())?;
                                sent += remote.len() as u64;
                            }
                            Err(e) => notify(&mut stdout, &format!("Upload failed: {e}"), false),
                        }
                    }
                }
            }

            // Read remote output.
            match shell.read() {
                Ok(data) if data.is_empty() => {}
                Ok(data) => {
                    received += data.len() as u64;
                    if let Some(log) = logger.as_mut() {
                        log.write_all(&data).ok();
                    }
                    selection.clear();
                    for event in parser.process(&data) {
                        match event {
                            RemoteOutputEvent::Data(bytes) => {
                                // The vt100 mirror handles split UTF-8 itself;
                                // only the stdout write is boundary-buffered.
                                screen.process(&bytes);
                                let writable = utf8_carry.complete(&bytes);
                                stdout.write_all(&writable).map_err(|e| e.to_string())?;
                            }
                            RemoteOutputEvent::Response(bytes) => {
                                write_chunked(&mut shell, &bytes)?;
                            }
                            RemoteOutputEvent::Cpr => {
                                let (r, c) = screen.screen().cursor_position();
                                write_chunked(&mut shell, &cpr_response(r + 1, c + 1))?;
                            }
                        }
                    }
                    stdout.flush().ok();
                }
                Err(ssh::SshError::TimeoutError) => {}
                Err(_) => break, // remote closed
            }
            if shell.closed() {
                break;
            }
        }
        Ok(())
    })();

    stop.store(true, Ordering::Relaxed);
    input_thread.join().ok();
    shell.close().ok();
    session.close();
    result?;

    Ok(SessionSummary {
        duration: start.elapsed(),
        sent,
        received,
    })
}

fn write_chunked(shell: &mut ssh_client::Shell, bytes: &[u8]) -> Result<(), String> {
    for chunk in bytes.chunks(SSH_WRITE_CHUNK) {
        shell
            .write(chunk)
            .map_err(|e| ssh_client::friendly_error(&e))?;
    }
    Ok(())
}

fn notify(stdout: &mut std::io::Stdout, msg: &str, is_ok: bool) {
    let color = if is_ok { "\x1b[32m" } else { "\x1b[31m" };
    write!(stdout, "\r\n{color}[ssh4] {msg}\x1b[0m\r\n").ok();
    stdout.flush().ok();
}

fn copy_selection(selection: &Selection, screen: &vt100::Parser) {
    let text = selection.extract_text(|row| {
        if row < 0 {
            return String::new();
        }
        screen
            .screen()
            .contents_between(row as u16, 0, row as u16 + 1, 0)
    });
    if !text.is_empty() {
        if let Ok(mut cb) = arboard::Clipboard::new() {
            cb.set_text(text).ok();
        }
    }
}

/// Returns bytes to forward to the remote (SGR mouse), or None for local handling.
fn handle_mouse(
    m: &crossterm::event::MouseEvent,
    remote_mouse: bool,
    selection: &mut Selection,
    _screen: &vt100::Parser,
) -> Option<Vec<u8>> {
    let shift = m.modifiers.contains(KeyModifiers::SHIFT);
    let col = m.column + 1;
    let row = m.row + 1;
    // Shift forces local selection even when the remote wants mouse events.
    if remote_mouse && !shift {
        return match m.kind {
            MouseEventKind::Down(b) => Some(mouse::sgr_button(btn(b), col, row, true)),
            MouseEventKind::Up(b) => Some(mouse::sgr_button(btn(b), col, row, false)),
            MouseEventKind::Drag(b) => Some(mouse::sgr_drag(btn(b), col, row)),
            MouseEventKind::ScrollUp => Some(mouse::sgr_scroll(true, col, row)),
            MouseEventKind::ScrollDown => Some(mouse::sgr_scroll(false, col, row)),
            _ => None,
        };
    }
    // Local selection.
    let pos = CellPos {
        row: m.row as i32,
        col: m.column as i32,
    };
    match m.kind {
        MouseEventKind::Down(crossterm::event::MouseButton::Left) => selection.start(pos),
        MouseEventKind::Drag(crossterm::event::MouseButton::Left) => selection.drag(pos),
        _ => {}
    }
    None
}

fn btn(b: crossterm::event::MouseButton) -> MouseButton {
    match b {
        crossterm::event::MouseButton::Left => MouseButton::Left,
        crossterm::event::MouseButton::Middle => MouseButton::Middle,
        crossterm::event::MouseButton::Right => MouseButton::Right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_parses_host_and_flags() {
        let cli = Cli::parse_from(["ssh4", "user@example.com:2222", "--log", "out.log"]);
        assert_eq!(cli.host.as_deref(), Some("user@example.com:2222"));
        assert_eq!(cli.log.as_deref(), Some("out.log"));
        assert!(!cli.once);
    }

    #[test]
    fn cli_parses_profile() {
        let cli = Cli::parse_from(["ssh4", "-P", "home", "--no-reconnect"]);
        assert_eq!(cli.profile.as_deref(), Some("home"));
        assert!(cli.no_reconnect);
    }

    #[test]
    fn resolve_uses_profile_values() {
        let mut config = Config::default();
        config.profiles.insert(
            "home".into(),
            Profile {
                host: "example.com".into(),
                port: 2200,
                user: "keith".into(),
                key_path: Some("k".into()),
                remote_dir: Some("/data".into()),
                password: None,
            },
        );
        let cli = Cli::parse_from(["ssh4", "-P", "home"]);
        let (params, remote_dir) = resolve_params(&cli, &config).unwrap();
        assert_eq!(params.host, "example.com");
        assert_eq!(params.port, 2200);
        assert_eq!(params.user, "keith");
        assert_eq!(params.key_path.as_deref(), Some("k"));
        assert_eq!(remote_dir, "/data");
    }

    #[test]
    fn resolve_missing_profile_errors() {
        let cli = Cli::parse_from(["ssh4", "-P", "nope"]);
        assert!(resolve_params(&cli, &Config::default())
            .unwrap_err()
            .contains("not found"));
    }

    #[test]
    fn duration_formatting() {
        assert_eq!(format_duration(Duration::from_secs(3725)), "01:02:05");
    }
}
