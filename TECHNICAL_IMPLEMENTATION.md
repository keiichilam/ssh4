# Technical Implementation Guide - ssh4 Redevelopment

Last updated: 2026-06-10

This guide gives practical engineering instructions for rebuilding `ssh4` from the current repository.

## 1. Development Environment

Required:

- Rust toolchain with Cargo.
- PowerShell on Windows for `install.ps1`.
- Network access to remote SSH hosts for manual integration testing.
- A local clipboard provider supported by `arboard`.

Build commands:

```powershell
cargo fmt
cargo check
cargo test
cargo build --release
```

Run commands:

```powershell
# GUI mode
cargo run
cargo run -- --gui

# CLI mode
cargo run -- user@example.com
cargo run -- user@example.com:2222
cargo run -- -P home

# One-shot clipboard image upload
cargo run -- user@example.com --once --remote-dir /tmp
```

Windows installer/build script:

```powershell
.\install.ps1
```

## 2. Current Repository Map

```text
Cargo.toml
Cargo.lock
README.md
HELP.md
ISSUES.md
TODO.md
design-system-audit.md
install.ps1
src/
  main.rs
  gui.rs
vendor/
  ssh-rs-0.3.3/
```

Important notes:

- `ssh-rs` is a path dependency in `Cargo.toml`.
- `src/main.rs` currently owns both CLI behavior and shared helpers.
- `src/gui.rs` imports helpers from `main.rs` and owns the entire GUI.
- There are existing tests at the bottom of `src/main.rs` for terminal output processing.
- The worktree can contain unrelated edits; avoid reverting unrelated changes during redevelopment.

## 3. Recommended Module Split

Use the current implementation as the behavior reference, but split ownership as follows.

### 3.1 `config.rs`

Own:

- `Profile`
- `Snippet`
- `Config`
- config path resolution
- config load/save
- default profile name
- config migration helpers

Rules:

- Keep `~/.ssh4.toml` compatible.
- On Unix, write config with owner-only permissions.
- Do not store passwords by default in new workflows.

### 3.2 `auth.rs`

Own:

- host string parser
- default key discovery
- secure CLI password prompt
- username fallback rules
- credential resolution from CLI/profile/GUI form

Testing:

- Parse `host`, `host:port`, `user@host`, `user@host:port`.
- Invalid port falls back to 22 if preserving current behavior.
- Default key ordering is stable.

### 3.3 `ssh_client.rs`

Own:

- `ConnParams`
- `connect_session`
- shell open helper
- timeout configuration
- password zeroing or secret wrapper
- common SSH errors mapped to user-facing messages

Rules:

- GUI callers must receive an error instead of triggering stdin prompts.
- CLI callers may prompt for password where appropriate.

### 3.4 `terminal/input.rs`

Own:

- crossterm key to byte mapping
- egui event to byte mapping
- xterm arrow modifier sequence helper
- SGR mouse encoding
- paste payload builder

Testing:

- Ctrl letters.
- Alt prefix.
- Shift+Tab.
- Ctrl/Shift/Ctrl+Shift arrows.
- Function keys.
- Bracketed paste payload.

### 3.5 `terminal/output.rs`

Own:

- remote output event parser
- terminal query response generation
- CPR request handling
- CPR response stripping
- OSC color query parsing
- mouse mode scanning
- bracketed paste mode scanning

Testing:

- Split escape sequences.
- Coalesced query sequences.
- Unknown escape passthrough.
- OSC BEL and ST terminators.
- Oversized truncated sequences.

### 3.6 `terminal/selection.rs`

Own:

- terminal grid selection model
- normalized selection range
- selection text extraction
- CLI text screen buffer if retained

### 3.7 `transfer.rs`

Own:

- clipboard image capture to temp PNG
- one-shot SCP upload
- GUI clipboard image upload thread helper
- dropped file staging and upload
- temp cleanup utilities
- remote path construction

Rules:

- Cleanup must run on success and failure.
- Transfer should report status through channels, not direct UI mutation.
- Avoid logging file contents.

### 3.8 `cli.rs`

Own:

- `Cli` struct from `clap`
- CLI mode dispatch
- interactive terminal loop
- reconnect prompt
- session logger
- print banner and stats

Rules:

- Raw mode cleanup must be guaranteed.
- Mouse capture cleanup must be guaranteed.
- Signal-like keys must be forwarded, not handled by the local process.

### 3.9 `gui/`

Suggested modules:

- `gui/app.rs`: top-level `App` and eframe lifecycle.
- `gui/session.rs`: `Session`, `PendingConn`, SSH worker state.
- `gui/render.rs`: terminal canvas and cell painting — the only place that
  reads `theme::current()` (the switchable terminal theme); everything
  else in `gui/` reads the fixed chrome palette via `theme::chrome()`.
- `gui/dock.rs`: the FlashLearn-redesign dock rail (New / Hosts / Snippets
  / Files / Theme / Session / Help) and its contextual flyout panel;
  replaces the old fixed sidebar.
- `gui/chrome.rs`: shared chrome primitives reused across the dock,
  flyout, terminal card, command palette, and bottom sheets — the brand
  gradient (baked texture, since egui has no native linear-gradient fill
  or rounded-mesh clipping), a themed card frame, the logo mark, and the
  terminal card's status header.
- `gui/icons.rs`: hand-drawn dock/flyout icon glyphs (procedural
  `egui::Shape` paths approximating the handoff's Lucide-style icons).
- `gui/tabs.rs`: floating pill tab switcher, tab color menu, tab search.
- `gui/dialogs.rs`: connection form, paste dialog and upload confirmation
  (bottom sheets), add snippet, command palette (TR-003). The Help
  shortcut reference lives in `gui/dock.rs`'s Help flyout instead of a
  standalone overlay.
- `gui/search.rs`: search state and match navigation.
- `gui/theme.rs`: theme system (TR-004) — named color roles, built-in
  theme list (Lavender / Amber Phosphor / Green Phosphor), runtime
  switching, fonts (bundled Inter for chrome, platform monospace for the
  terminal canvas). `chrome()` always resolves to the fixed Lavender
  palette regardless of `current()`/`set_current()` — only the terminal
  canvas re-themes when the user picks a different theme.
- `gui/files.rs`: File Tools window — remote pane (TR-001) and local pane
  (TR-002); background threads for all remote operations.

Additional shared modules added during redevelopment:

- `terminal/brackets.rs`: bracket matching over the visible grid (FR-015),
  pure text logic with unit tests.
- `remote_fs.rs`: remote file operations for TR-001 — POSIX `sh_quote`
  single-quote escaping for every remote path, `ls -la` parsing,
  chmod mode validation (all unit-tested), and exec/SCP wrappers
  (`list`, `mkdir`, `touch`, `rename`, `delete`, `chmod`, `download`,
  `upload`) using one-shot SSH connections.

## 4. Threading Model

### 4.1 GUI session threading

For each active GUI SSH session:

```text
egui UI thread
  |
  | GuiToSsh::Data / GuiToSsh::Resize
  v
SSH worker thread
  |
  | SshMsg::Connected / Data / Heartbeat / Disconnected / Error
  v
egui UI thread
```

Rules:

- UI thread never blocks on network I/O.
- SSH worker never touches GUI state directly.
- Worker may request repaint through an `egui::Context` clone.
- Stop flag is an `Arc<AtomicBool>`.
- Dropping a `Session` sets the stop flag.

### 4.2 Output batching

Keep these concepts:

- Accumulate remote output in worker thread.
- Flush to UI when buffer reaches threshold or interval expires.
- Limit per-frame message and byte processing in the UI.
- Request another repaint if frame limits are hit.

This prevents large output from freezing the GUI.

### 4.3 Input batching

Keep these concepts:

- Drain a bounded number of GUI input messages per worker tick.
- Split large writes into chunks.
- Coalesce rapid resize events.
- Apply resize before and after reads to reduce lag during window drags.

## 5. Terminal Protocol Details

### 5.1 Device query responses

The app currently answers these remote queries:

| Query | Reply |
| --- | --- |
| `ESC[c` | `ESC[?62;1;6c` |
| `ESC[0c` | `ESC[?62;1;6c` |
| `ESC[>c` | `ESC[>1;0;0c` |
| `ESC[>0c` | `ESC[>1;0;0c` |
| `ESC[5n` | `ESC[0n` |
| `ESC[6n` | Ordered CPR event, answered with current cursor position |

### 5.2 OSC color query responses

The app answers:

- OSC 10 foreground color query.
- OSC 11 background color query.
- OSC 12 cursor color query.

Both BEL and ST terminators must be supported.

### 5.3 CPR handling

Requirements:

- Strip remote CPR responses of the form `ESC[digits;digitsR`.
- Preserve CPR request ordering relative to screen data.
- Buffer split sequences across reads.
- Do not buffer unbounded incomplete sequences.

### 5.4 Mouse mode scanning

Detect enable:

- `ESC[?1000h`
- `ESC[?1002h`
- `ESC[?1003h`
- `ESC[?1006h`

Detect disable:

- `ESC[?1000l`
- `ESC[?1002l`
- `ESC[?1003l`
- `ESC[?1006l`

### 5.5 Bracketed paste scanning

Detect:

- `ESC[?2004h`
- `ESC[?2004l`

## 6. GUI Implementation Notes

### 6.1 eframe setup

Current `gui::run()` creates an eframe native window. The redevelopment should preserve:

- Theme application before first draw.
- Reasonable initial window size.
- Default GUI startup when no CLI target is present.
- Console hiding on Windows when launched as a GUI app.

### 6.2 Terminal metrics

The GUI measures monospace glyph dimensions using egui fonts. PTY dimensions are calculated from available panel size:

```text
cols = floor(terminal_width / cell_width)
rows = floor(terminal_height / cell_height)
```

Bounds should prevent unusable dimensions:

- minimum cols around 40
- minimum rows around 15
- maximum cols around 320
- maximum rows around 160

Current display controls support:

- UI zoom.
- terminal font size.
- optional locked cols/rows with font auto-adjust.
- reset display settings.

### 6.3 Terminal renderer

Renderer pipeline:

1. Poll SSH messages.
2. Process remote output events and terminal responses.
3. Feed clean data into `vt100::Parser`.
4. Update search cache, line timestamps, bracketed paste, mouse mode, upload status.
5. Paint terminal canvas from `vt100::Screen`.
6. Handle pointer interactions against cell coordinates.

Rendering should be deterministic and mostly pure. Keep side effects such as opening URLs, copying paths, or sending input outside the low-level paint loop where possible.

CJK / wide-character handling:

- The GUI appends platform CJK fallback fonts (YaHei/JhengHei/Yu Gothic/MS Gothic/Malgun on Windows, PingFang/Hiragino/Apple SD Gothic on macOS, Noto CJK / WenQuanYi on Linux) after the monospace face so Han, kana, and hangul render instead of tofu.
- Wide cells (`vt100::Cell::is_wide`) are painted individually, centered in a two-cell rect, so batched narrow runs never drift from the grid; the cursor block widens to two cells over a wide glyph.
- All column math (selection extraction, search match positions, link detection) uses `unicode-width` display cells, not char counts. Selection includes a wide char when either of its cells is touched; zero-width combining marks travel with their base char.
- CLI mode forces the Windows console to the UTF-8 codepage (65001) on startup, and `terminal::output::Utf8Carry` withholds an incomplete trailing UTF-8 sequence between stdout writes so a CJK char split across SSH reads is never written in two pieces (legacy conhost renders split sequences as mojibake). The vt100 parser itself handles split UTF-8 statefully via vte.

### 6.4 Modal ownership

When any modal or overlay is open, terminal input must not be forwarded.

Input owners:

- Paste dialog.
- Add snippet dialog.
- Upload confirmation dialog.
- Help overlay.
- Tab search overlay.
- Find bar.

### 6.5 Search

Search should operate over visible screen and scrollback, using stable distance-from-bottom match IDs so navigation survives scroll changes.

New terminal data invalidates cached matches.

### 6.6 Links and paths

Detected link/path spans should be derived from row text. Actions:

- Ctrl-click URL: open default browser.
- Ctrl-click path: copy path to clipboard.
- Search selected text online: percent-encode selection and open chosen search URL.

## 7. CLI Implementation Notes

### 7.1 Raw mode lifecycle

Use a guard object or equivalent to guarantee cleanup:

- `enable_raw_mode`
- `EnableMouseCapture`
- on drop: `DisableMouseCapture`
- on drop: `disable_raw_mode`

Avoid cleanup paths that depend on normal loop termination only.

### 7.2 Input thread

The CLI input thread should:

- Poll crossterm events.
- Convert key events into terminal bytes.
- Send resize events.
- Send mouse events.
- Trigger clipboard upload or paste behavior.
- Stop quickly when requested.

### 7.3 Output loop

The CLI main loop should:

- Read remote output.
- Process remote output events.
- Write clean bytes to stdout.
- Write query responses back to remote shell.
- Update mouse and bracketed paste flags.
- Update text screen buffer for selection.
- Write raw bytes to optional logger.

## 8. Transfer Implementation Notes

### 8.1 Clipboard image

Implementation steps:

1. Create `Clipboard`.
2. Get image data.
3. Convert to `image::ImageBuffer<Rgba<u8>, Vec<u8>>`.
4. Save to temp PNG path.
5. Open SSH session.
6. Open SCP channel.
7. Upload local temp PNG.
8. Copy remote path to clipboard.
9. Remove temp PNG.

Use a cleanup guard or closure pattern so deletion runs on failures.

### 8.2 Dropped files

Implementation steps:

1. Receive dropped paths from egui input.
2. Show confirmation dialog with path sizes.
3. Copy files and folders into a temp staging directory.
4. Upload staging directory through SCP.
5. Report remote directory path to UI.
6. Remove staging directory.

The current implementation uploads staging folders to `/tmp`; redevelopment should decide whether to preserve that or use each profile's `remote_dir`.

### 8.3 Future remote file tools

For future remote file tools:

- Prefer SFTP for listing, chmod, rename, upload, download, delete if a reliable crate is selected.
- If using SSH commands, quote paths safely and avoid ad hoc command strings.
- Confirm destructive actions.
- Surface command output and errors clearly.

## 9. Testing Strategy

### 9.1 Unit tests

Required unit tests:

- Host parsing.
- Default profile naming.
- Terminal key mappings.
- Mouse SGR encoding.
- Bracketed paste payload.
- Remote output parser.
- OSC color query parser.
- Config serialize/deserialize.
- Transfer remote path generation.
- Temp cleanup behavior.

### 9.2 Integration tests

Recommended integration tests:

- Start a local test SSH server or container.
- Connect with password auth.
- Connect with key auth.
- Open shell and echo known output.
- Exercise CPR/OSC query handling with scripted output.
- Upload a test file by SCP.
- Upload a clipboard-generated PNG where supported.

### 9.3 Manual GUI tests

Manual test checklist:

- Launch GUI with no arguments.
- Connect by host.
- Connect by saved profile.
- Open multiple tabs.
- Close tabs.
- Toggle focus mode.
- Toggle sync input.
- Use Ctrl+Shift+F search.
- Use Ctrl+Shift+P tab search.
- Select and copy terminal text.
- Paste single-line text.
- Paste multiline text and cancel.
- Paste multiline text and send.
- Upload clipboard image.
- Drag and drop file.
- Run snippet.
- Toggle debug log.
- Resize window and reconnect.
- Use vim or tmux to verify mouse forwarding.

### 9.4 Regression fixtures

Keep byte-sequence fixtures for terminal protocol tests. Regression tests should include sequences that are split at every possible byte boundary for key escape sequences.

## 10. Security Checklist

Before release:

- No passwords written to debug logs.
- No private key contents written to logs.
- Temp upload files removed on failure.
- CLI raw mode restored on panic where practical.
- Config permissions restricted on Unix.
- Remote delete operations require confirmation.
- Remote shell command paths are quoted or passed through structured APIs.
- Clipboard path writes are intentional and documented.
- Debug log location is visible to the user.

## 11. Performance Checklist

Before release:

- Large `cat` output does not freeze GUI.
- Terminal output still paints incrementally during long-running commands.
- Clipboard paste of large text does not block UI indefinitely.
- SSH worker does not busy-spin on timeouts.
- Closing tabs stops worker threads.
- Terminal image list is capped.
- Search cache invalidates on new output.
- Repaint requests are bounded.

## 12. Documentation Checklist

Keep these documents synchronized:

- `README.md`: user-facing overview and examples.
- `HELP.md`: concise quick reference.
- `docs/SOFTWARE_DESIGN.md`: architecture and design.
- `docs/PRODUCT_REQUIREMENTS.md`: functional and nonfunctional requirements.
- `docs/TECHNICAL_IMPLEMENTATION.md`: rebuild guide.
- `docs/REDEVELOPMENT_ROADMAP.md`: sequence and milestone tracking.

When behavior changes, update the docs in the same pull request as the code.
