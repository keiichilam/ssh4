# Software Design - ssh4

Last updated: 2026-06-10

This document describes the current `ssh4` application and the design to preserve during redevelopment. It is based on the current repository contents, especially `src/main.rs`, `src/gui.rs`, `Cargo.toml`, `README.md`, `HELP.md`, `TODO.md`, and `design-system-audit.md`.

## 1. Purpose

`ssh4` is a Rust SSH client with two user interfaces:

- A GUI terminal client, launched by default with no arguments.
- A CLI terminal client, launched when a host or saved profile is provided.

The application provides SSH terminal access, profile persistence, terminal key and mouse translation, clipboard text paste, clipboard image upload over SCP, drag-and-drop file upload in the GUI, terminal search, tabbed GUI sessions, snippets, debug logging, and visual terminal intelligence such as link detection and search highlighting.

The redevelopment goal is to rebuild the application with the same functional behavior, while separating the current monolithic implementation into smaller modules with clearer ownership.

## 2. System Context

```text
User
  |
  | keyboard, mouse, files, clipboard
  v
ssh4
  |
  | SSH shell, SCP upload
  v
Remote SSH Server

Local filesystem:
- ~/.ssh4.toml for profiles, snippets, display preferences
- temp directory for clipboard image and dropped-file staging
- ssh4-debug.log for GUI debug logging
- optional CLI session log file
```

## 3. Runtime Modes

### 3.1 GUI mode

GUI mode is the default when the binary is launched with no host and no profile. The `--gui` flag forces GUI mode even if CLI-style arguments are supplied.

Primary GUI responsibilities:

- Connection form and saved profile sidebar.
- Multi-tab terminal sessions.
- SSH I/O on one worker thread per active session.
- Terminal rendering through `vt100::Parser` plus custom `egui` painting.
- Clipboard text paste and image upload.
- Drag-and-drop file upload.
- In-terminal search, tab search, help overlay, paste dialog, upload confirmation dialog.
- Debug logging toggle.

### 3.2 CLI mode

CLI mode is selected when the user supplies a host or profile and does not force `--gui`.

Primary CLI responsibilities:

- Parse connection flags.
- Resolve profile values.
- Detect default SSH keys.
- Prompt for username or password when needed.
- Open an SSH shell using `ssh-rs`.
- Forward crossterm keyboard and mouse input to the remote PTY.
- Render remote output in the local terminal.
- Support one-shot clipboard image SCP upload.
- Optionally append raw remote output to a session log.
- Prompt for reconnect after disconnect unless `--no-reconnect` is set.

## 4. Source Layout

Current layout:

```text
Cargo.toml
Cargo.lock
install.ps1
src/
  main.rs       CLI entry point, shared config/auth/SSH helpers, CLI terminal
  gui.rs        eframe/egui app, GUI sessions, terminal renderer, GUI SSH worker
vendor/
  ssh-rs-0.3.3/ vendored SSH library used by Cargo path dependency
```

The current implementation is intentionally compact but large. `src/main.rs` is over 2,000 lines and `src/gui.rs` is over 5,800 lines. Redevelopment should split these areas into modules without changing behavior.

Recommended redevelopment layout:

```text
src/
  main.rs
  cli.rs
  config.rs
  auth.rs
  ssh_client.rs
  transfer.rs
  remote_fs.rs    (TR-001: quoting, ls parsing, exec/SCP file operations)
  terminal/
    mod.rs
    input.rs
    output.rs
    mouse.rs
    selection.rs
    brackets.rs
  gui/
    mod.rs
    app.rs
    session.rs
    render.rs
    dock.rs       (dock rail + contextual flyout, replaces the old sidebar)
    chrome.rs     (shared gradient/card/logo/header chrome primitives)
    icons.rs      (hand-drawn dock/flyout icons)
    dialogs.rs
    tabs.rs
    search.rs
    theme.rs
    files.rs      (TR-001/TR-002 File Tools window)
```

## 5. Core Dependencies

| Crate | Current version | Purpose |
| --- | --- | --- |
| `ssh-rs` | vendored 0.5.0 | Pure Rust SSH client and SCP support (local mods: `window_change` PTY resize, `keepalive` SSH_MSG_IGNORE probe, resumable packet reads, rekey timeout floor, EOF-as-error, `TCP_NODELAY`) |
| `eframe` / `egui` | 0.28 | Native GUI framework |
| `vt100` | 0.15 | Terminal parser and screen model for GUI rendering |
| `crossterm` | 0.28.1 | CLI raw mode, keyboard, mouse, local terminal control |
| `arboard` | 3.4.0 | Clipboard text and image access |
| `image` | 0.25.0 | Encode clipboard image data as PNG |
| `clap` | 4.4 | CLI argument parser |
| `serde` + `toml` | 1 / 0.8 | Profile, snippet, and UI preference persistence |
| `chrono` | 0.4 | Timestamped filenames and log messages |
| `rfd` | 0.14 | Native file picker in GUI |

## 6. Data Model

### 6.1 Persistent configuration

Configuration lives at:

```text
~/.ssh4.toml
```

Current schema:

```toml
[profiles.home]
host = "example.com"
port = 22
user = "keith"
key_path = "/home/keith/.ssh/id_ed25519"
remote_dir = "/tmp"
password = "optional"

[[snippets]]
name = "disk usage"
command = "df -h"

ui_zoom = 1.0
auto_fit = false
```

Rust types:

- `Profile`: host, port, user, key path, remote directory, optional password.
- `Snippet`: name and command.
- `Config`: profile map, snippet list, UI zoom, auto-fit flag.

Security note: current config can persist passwords if provided through GUI/profile flow. A redevelopment should prefer not storing passwords by default and should make any password persistence explicit.

### 6.2 Connection parameters

`ConnParams` is the canonical in-memory connection bundle:

- `host`
- `port`
- `user`
- `password`
- `key_path`
- `timeout_secs`
- `verbose`

The current `Drop` implementation overwrites password bytes before release. Keep that behavior or replace it with a dedicated secret-handling crate.

### 6.3 GUI application state

The GUI state centers on `App`, `Tab`, `TabState`, and `Session`.

`TabState`:

- `ConnectionForm`: pending or editable connection form.
- `Active`: live terminal session plus local terminal selection.

`Session` contains:

- `vt100::Parser` terminal state.
- GUI-to-SSH sender and SSH-to-GUI receiver.
- stop flag shared with SSH worker thread.
- terminal dimensions.
- connection identity.
- byte counters.
- connection params for transfer actions.
- remote upload directory.
- bracketed paste flag.
- scrollback offset.
- output buffer for split escape sequence handling.
- Kitty graphics buffers and rendered terminal images.
- line timestamp ring buffer.

### 6.4 Channel messages

GUI to SSH worker:

- `Data(Vec<u8>)`: write raw bytes to remote PTY.
- `Resize { cols, rows }`: notify remote PTY of terminal resize.

SSH worker to GUI:

- `Connected { cols, rows }`
- `Data(Vec<u8>)`
- `Heartbeat { pending_chunks, buffered_output, read_timeouts }`
- `Disconnected`
- `Error(String)`

CLI terminal events:

- `Input(Vec<u8>)`
- `Resize(u16, u16)`
- `Mouse(crossterm::event::MouseEvent)`
- `Notify { msg, is_ok }`

## 7. Terminal Protocol Design

### 7.1 Input translation

Both UI modes translate local input to VT-compatible byte sequences.

Required mappings:

- Printable text to UTF-8 bytes.
- `Enter` to `\r`.
- `Backspace` to `0x7f`.
- `Tab` to `\t`.
- `Shift+Tab` to `ESC[Z`.
- `Esc` to `0x1b`.
- Arrows to `ESC[A/B/C/D`.
- `Ctrl+Arrow` and `Shift+Arrow` to xterm modifier sequences.
- `Alt+key` to ESC-prefixed key sequence.
- Function keys `F1` through `F12`.
- Navigation keys: Home, End, Page Up, Page Down, Delete, Insert.
- `Ctrl+C`, `Ctrl+D`, `Ctrl+V`, `Ctrl+P`, `Ctrl+X` according to context.

### 7.2 Output filtering

Remote output is not passed blindly to the renderer. The application intercepts terminal queries that can break local Windows terminals or block remote TUIs.

Handled output sequences include:

- Device Attributes queries.
- Device Status Report query.
- Cursor Position Report requests.
- Cursor Position Report responses.
- OSC 10, 11, and 12 color queries.
- DEC private mode mouse enable/disable.
- DEC private mode bracketed paste enable/disable.

In GUI mode, CPR requests are returned as ordered `RemoteOutputEvent::Cpr` markers so the renderer can update the parser with preceding bytes before answering with the current cursor position.

### 7.3 Mouse behavior

When the remote application enables mouse mode through DEC private mode sequences, mouse events are forwarded using SGR mouse protocol. Holding Shift forces local selection behavior.

Local selection behavior:

- Click-drag selects terminal cells.
- `Ctrl+C`, `Enter`, or context menu Copy copies selected text.
- Selection clears when new remote output arrives.

### 7.4 Bracketed paste

When the remote enables bracketed paste (`ESC[?2004h`), pasted content is wrapped as:

```text
ESC[200~<payload>ESC[201~
```

The GUI also opens a paste dialog for multiline text before sending it.

## 8. SSH and Transfer Design

### 8.1 Connection flow

1. Resolve host, port, user, key, password, timeout, remote directory.
2. Build an `ssh-rs` session.
3. Try configured password and/or key.
4. If key auth fails in CLI verbose flow, prompt for password.
5. Run a local SSH session.
6. Open a remote shell terminal with the requested PTY dimensions.

### 8.2 GUI worker thread

Each active GUI tab has one SSH worker thread. The thread:

- Opens SSH session and PTY shell.
- Sends `Connected` to GUI.
- Drains input messages into a pending write queue.
- Chunks large writes using `SSH_WRITE_CHUNK`.
- Coalesces resize messages and applies only the latest pending resize.
- Reads remote output with a short timeout.
- Buffers output until enough bytes arrive or the repaint interval elapses.
- Sends heartbeats every five seconds.
- Flushes remaining output and sends `Disconnected` on shutdown.

### 8.3 Clipboard image upload

Current flow:

1. Read image from local clipboard with `arboard`.
2. Encode as PNG in the OS temp directory.
3. Open a separate SSH/SCP connection.
4. Upload to remote directory as `clip_YYYYMMDD_HHMMSS.png`.
5. Copy the remote path to local clipboard.
6. Optionally type the remote path into the active shell.
7. Delete the local temp file.

### 8.4 Drag-and-drop file upload

Current GUI flow:

1. User drops files or folders on an active terminal.
2. GUI opens a confirmation dialog.
3. User edits the folder name.
4. Files/folders are copied into a temporary staging folder.
5. Staging folder is uploaded via SCP to `/tmp`.
6. Remote path is copied to clipboard and typed into shell.
7. Staging folder is removed.

Note: the current dropped-file upload always targets `/tmp` in implementation. If redevelopment wants profile-specific remote directories for dropped files, make that an explicit requirement and test it.

## 9. GUI Design

### 9.1 Visual structure

Primary panels:

- Sidebar: profiles, snippets, help, debug logging, display controls.
- Top tab bar: session tabs, tab actions, sync input, focus mode, tab search.
- Connection form: host, key path, password, save-as profile name.
- Terminal top status bar: identity, dimensions, byte counters, heartbeat counters, upload status.
- Terminal canvas: custom painted vt100 screen.
- Modal overlays: help, add snippet, paste dialog, upload confirmation, tab search.

### 9.2 Terminal renderer

The terminal renderer should keep using a real terminal parser instead of manual ANSI parsing for screen state. Current renderer responsibilities:

- Paint terminal background.
- Map vt100 cells to glyphs and colors.
- Draw ANSI 16-color and RGB colors.
- Draw selected text.
- Draw search matches and active match.
- Draw bracket pair highlight.
- Draw detected links and paths.
- Draw cursor and outline matching brackets at the cursor.
- Kitty graphics protocol: dropped from scope (see PRD FR-015) — the vt100
  cell grid has no image-placement model.
- Draw scrollbar and support scrollback.
- Attach line timestamp hover tooltips (Alt+hover; per-row change stamps).

### 9.3 Theme

Implemented in `src/gui/theme.rs` as a `Theme` struct of named color roles with three built-in themes (Lavender — light, default — Amber Phosphor, Green Phosphor). The active theme is selected at runtime (the dock's Theme flyout or command palette) and persisted to `~/.ssh4.toml` (`theme = "..."`). Terminal-canvas painting code (`render.rs`) reads colors via `theme::current()`; every other UI surface (dock, flyout, tab switcher, terminal-card frame, dialogs, file tools) reads the fixed FlashLearn chrome palette via `theme::chrome()`, which always resolves to Lavender regardless of the active terminal theme — only the terminal canvas itself re-themes.

`design-system-audit.md` notes token leakage and should be treated as a redevelopment input. A rebuild should centralize:

- Surface scale.
- Text scale.
- Accent states.
- Semantic colors.
- Selection colors.
- Link color.
- Overlay colors.
- Radius scale.
- Font scale.
- Spacing scale.

## 10. CLI Design

The CLI terminal loop uses `crossterm` raw mode and mouse capture.

Responsibilities:

- Enable ANSI VT processing on Windows.
- Read local terminal size.
- Open SSH shell with matching PTY size.
- Spawn an input thread for keyboard, mouse, resize, and clipboard events.
- Forward remote output to stdout after filtering.
- Forward terminal query responses back to the shell.
- Detect remote mouse and bracketed paste modes.
- Maintain a plain text screen buffer for mouse selection.
- Log raw remote output when `--log` is provided.
- Cleanly disable raw mode and mouse capture on exit.

## 11. Security Design

Required security behaviors:

- Do not shell out to the local `ssh` binary for core SSH operations.
- Warn when passwords are passed through CLI flags.
- Avoid storing passwords unless the user explicitly chooses that behavior.
- Zero password buffers when practical.
- Use restrictive permissions for config on Unix (`0600`).
- Remove temp files after uploads, including failure paths.
- Treat remote file operations as dangerous if they execute shell commands.
- Never log plaintext passwords.
- Ensure debug logs do not include private key contents or clipboard image bytes.

## 12. Performance Design

Current important limits:

- SSH write chunk: 4096 bytes.
- GUI write chunks per tick: 4.
- GUI input drain per tick: 64 messages.
- SSH read timeout: 20 ms.
- GUI output flush threshold: 64 KiB.
- GUI repaint output interval: 16 ms.
- GUI max SSH messages per frame: 128.
- GUI max SSH bytes per frame: 256 KiB.
- vt100 scrollback: 1000 lines in GUI sessions.
- line timestamp cap: 4096 timestamps.
- terminal image cap: 64 images.

Redevelopment should preserve these backpressure ideas even if constants are retuned.

## 13. Observability

Current observability:

- GUI debug log at `ssh4-debug.log`, controlled by sidebar toggle.
- Panic logger writes to the debug log.
- SSH worker heartbeats report pending chunks, buffered output, and read timeout count.
- GUI status bar shows sent and received bytes, idle time, heartbeat age, pending queue, buffer size, and timeouts.
- CLI session summary prints host, duration, sent bytes, and received bytes on disconnect.

Redevelopment should keep these signals because they are useful when diagnosing SSH stalls, rendering stalls, and upload failures.

## 14. Known Constraints

- The app depends on vendored `ssh-rs` 0.5.0 (`vendor/ssh-rs-0.5.0`). Local modifications (all commented `ssh4 local modification` in source): `ChannelShell::window_change` (RFC 4254 §6.7 PTY resize), `ChannelShell::keepalive` (RFC 4253 §11.2 SSH_MSG_IGNORE probe), resumable packet reads so the short interactive read timeout cannot drop partial bytes and desynchronize the transport, a ≥30 s timeout floor for (re-)key exchanges with timeout treated as a fatal `KexError`, mid-read EOF surfaced as `UnexpectedEof` instead of spinning, and `TCP_NODELAY` on connect. Audit these before replacing the vendored copy.
- SSH agent forwarding, port forwarding, and X11 forwarding are out of scope for current behavior.
- SCP supports file transfer but not listing, rename, chmod, or deletion; richer remote file management requires SSH commands or SFTP.
- GUI and CLI share logic through `main.rs` today. Redevelopment should extract shared terminal and SSH protocol code to avoid divergence.
- Some docs mention richer SCP tools than the current GUI source implements. Treat those as target requirements unless verified in code.

## 15. Redevelopment Principles

- Keep terminal fidelity ahead of UI polish.
- Preserve channel-based separation between UI and network I/O.
- Make all remote protocol handling testable without a real SSH server.
- Keep current CLI and GUI parity for key mappings, mouse mode, paste mode, output filtering, and uploads.
- Avoid adding a remote file explorer until core PTY behavior is stable.
- Add abstractions only where the current monolith has already proven a boundary: config, SSH session, terminal protocol, transfer, GUI session state, rendering, dialogs.
