# ssh4

A Rust SSH client with two user interfaces:

- **GUI mode** (default): tabbed SSH terminal sessions built on eframe/egui with
  saved profiles, snippets, terminal search, clipboard image upload,
  drag-and-drop file upload over SCP, a dual-pane file manager (remote over
  exec+SCP, local), a command palette, and selectable themes.
- **CLI mode**: a raw-mode terminal client (crossterm) selected automatically
  when a host or profile is given.

## Usage

```powershell
# GUI mode (default with no arguments)
ssh4
ssh4 --gui

# CLI mode
ssh4 user@example.com
ssh4 user@example.com:2222
ssh4 -P home                      # connect from a saved profile

# One-shot clipboard image upload over SCP
ssh4 user@example.com --once --remote-dir /tmp

# Other CLI flags
ssh4 user@host --key C:\keys\id_ed25519
ssh4 user@host --log session.log  # append raw output to a log file
ssh4 user@host --save-profile home
ssh4 user@host --no-reconnect
```

## Key bindings

| Key | CLI | GUI |
| --- | --- | --- |
| `Ctrl+C` | send interrupt | copy selection, else interrupt |
| `Ctrl+D` | send EOF | send EOF |
| `Ctrl+V` | paste clipboard text | paste (multiline opens a dialog) |
| `Ctrl+P` | upload clipboard image, type remote path | same |
| `Ctrl+Shift+F` | — | find in scrollback |
| `Ctrl+Shift+P` | — | search open tabs |
| `Ctrl+Shift+K` | — | command palette |
| `F11` | — | focus mode (hide sidebar) |
| `Enter` | copy active selection | copy active selection |
| `Shift+drag` | local selection while a TUI owns the mouse | same |
| `Alt+hover` | — | show when the hovered row last changed |

Mouse events are forwarded to remote applications (vim, tmux, htop, …) with
the SGR protocol when they enable DEC mouse mode; bracketed paste is honored
when the remote enables it.

In the GUI, right-click opens a context menu with Copy, Paste, and — when text
is selected — Search online (opens the selection as a web search in the
default browser). The mouse cursor hides over the terminal while typing and
reappears on movement. When the terminal cursor sits on a bracket
(`()[]{}<>`), its match is outlined on screen.

## File tools

`Ctrl+Shift+K → Open file tools` (or the sidebar button) opens a dual-pane
window:

- **Local pane**: browse, create file/folder, rename, copy/move to another
  folder, delete (with confirmation), and upload the selected path to the
  current remote folder.
- **Remote pane** (requires a connected tab): browse via `ls`, create
  file/folder, rename, chmod (octal editor pre-filled from the listing),
  delete (always confirms; runs `rm -rf`), download into the current local
  folder, and **Edit** — downloads the file, opens it in the system editor,
  and re-uploads automatically every time it is saved.

All remote paths are shell-quoted; remote operations run on background
threads over one-shot SSH connections, so the terminal session is untouched.

## Keep alive

The sidebar **Session → Keep alive** toggle sends a transport-level
`SSH_MSG_IGNORE` probe every 30 seconds on all connected tabs, so idle
sessions are not dropped by NAT or firewall timeouts. The setting persists
(`keep_alive` in `~/.ssh4.toml`), applies to live sessions immediately, and
is off by default.

## Themes

Three built-in themes — Amber Phosphor (default), Green Phosphor, and Paper
(light) — selectable from the sidebar Display section or the command palette,
persisted to `~/.ssh4.toml` as `theme = "..."`.

## Configuration

Profiles, snippets, and display preferences persist to `~/.ssh4.toml`
(`USERPROFILE` on Windows, `HOME` elsewhere):

```toml
ui_zoom = 1.0
auto_fit = false
theme = "Amber Phosphor"
keep_alive = false

[profiles.home]
host = "example.com"
port = 22
user = "keith"
key_path = "/home/keith/.ssh/id_ed25519"
remote_dir = "/tmp"

[[snippets]]
name = "disk usage"
command = "df -h"
```

Passwords are never persisted unless explicitly stored in a profile by hand.
Default keys are discovered in `~/.ssh` in this order: `id_ed25519`, `id_rsa`,
`id_ecdsa`, `id_ed25519_sk`, `id_ecdsa_sk`.

## Building

```powershell
cargo build --release    # binary at target/release/ssh4.exe
.\install.ps1            # build and install to %LOCALAPPDATA%\ssh4
```

The SSH transport is a vendored copy of `ssh-rs` 0.5.0
(`vendor/ssh-rs-0.5.0`) with local modifications: `window_change` (RFC 4254
§6.7 PTY resize) and `keepalive` (RFC 4253 §11.2 SSH_MSG_IGNORE probe) on the
shell channel, plus transport hardening — resumable packet reads (a short read
timeout no longer drops partial bytes and desyncs the stream), key
re-exchanges run under a 30 s timeout floor with failures fatal, EOF mid-read
errors instead of spinning, and `TCP_NODELAY` on the connection.

## Development

Design and planning documents:

- `PRODUCT_REQUIREMENTS.md` — functional requirements
- `SOFTWARE_DESIGN.md` — architecture
- `TECHNICAL_IMPLEMENTATION.md` — module ownership and protocol details
- `REDEVELOPMENT_ROADMAP.md` — milestone tracking

```powershell
cargo fmt
cargo check
cargo test        # terminal protocol, config, auth, transfer unit tests
```
# ssh4
