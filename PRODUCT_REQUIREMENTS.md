# Product Requirements - ssh4 Redevelopment

Last updated: 2026-06-10

This document defines the product surface to rebuild. It separates current shipped behavior from planned redevelopment targets where the repository documentation and source code differ.

## 1. Product Summary

`ssh4` is a desktop and terminal SSH client optimized for interactive shell work with strong clipboard and transfer workflows. It must work as a normal command-line SSH client and as a GUI application with tabs, saved profiles, terminal search, snippets, and graphical terminal conveniences.

## 2. Users

Primary users:

- Developers and operators who frequently SSH into Linux or Unix-like hosts.
- Windows users who want a Rust-native SSH client with full ANSI behavior.
- Users who need quick clipboard image uploads to remote machines.
- Users who run the same commands across multiple SSH sessions.

Secondary users:

- Users who prefer GUI tabs and saved profiles over command-line invocations.
- Users who need quick remote command snippets.
- Developers maintaining or extending the application.

## 3. Current Product Scope

Current implemented product areas:

- GUI mode by default with connection form.
- CLI mode when a host or profile is supplied.
- Saved connection profiles in `~/.ssh4.toml`.
- Command snippets in `~/.ssh4.toml`.
- SSH shell sessions through vendored `ssh-rs`.
- SCP clipboard image upload.
- GUI drag-and-drop upload.
- Terminal key forwarding.
- Mouse forwarding when remote apps enable mouse mode.
- Local terminal text selection.
- Bracketed paste.
- GUI multiline paste dialog.
- GUI tabs, tab colors, tab search, focus mode, sync input.
- GUI find bar across terminal scrollback.
- Link and path detection in GUI output.
- Visual highlights for failures, warnings, and successes.
- Debug log toggle and panic logging.
- CLI raw terminal mode with reconnect prompt and optional session logging.

Planned or partially documented target areas:

- Rich SCP/SFTP tools: remote browser, download, auto-upload-on-save, rename, chmod, delete, new file, new folder.
- Integrated local file manager.
- Command palette.
- Explorer pane and shell pane.
- Free type mode.
- Split views, folding, outlining.
- Color scheme/theme switching.
- Window transparency.
- Local and remote modes with vim-style navigation.

## 4. Functional Requirements

### FR-001 Launch behavior

The application shall open GUI mode when launched with no host and no profile.

The application shall open CLI mode when launched with a host or saved profile unless `--gui` is specified.

Acceptance:

- `ssh4` opens the GUI.
- `ssh4 --gui user@host` opens the GUI.
- `ssh4 user@host` opens a CLI SSH session.
- `ssh4 -P home` opens a CLI SSH session from the saved profile.

### FR-002 Host parsing

The application shall parse host strings in these forms:

- `host`
- `host:port`
- `user@host`
- `user@host:port`

Port shall default to `22`.

### FR-003 Authentication

The application shall support:

- Username from host string.
- Username override through `--user`.
- Private key override through `--key`.
- Password override through `--password`.
- Default key discovery in `~/.ssh`.
- Secure password prompt in CLI mode when no key is available or key auth fails.
- GUI password field when key authentication is not desired.

Acceptance:

- Default key discovery checks `id_ed25519`, `id_rsa`, `id_ecdsa`, `id_ed25519_sk`, and `id_ecdsa_sk`.
- CLI warns when `--password` is used.
- GUI never blocks on stdin for password prompt.

### FR-004 Configuration persistence

The application shall load and save configuration at `~/.ssh4.toml`.

Configuration shall support:

- Named profiles.
- Snippets.
- UI zoom.
- Auto-fit preference.

Acceptance:

- Successful GUI connections are auto-saved as profiles.
- CLI `--save-profile NAME` saves the connection.
- Deleted profiles are removed from the config file.
- Snippet add/delete operations persist.

### FR-005 CLI shell terminal

The CLI mode shall provide an interactive SSH terminal with:

- Raw keyboard forwarding.
- Mouse forwarding.
- Local mouse selection.
- Resize handling.
- Clipboard text paste.
- Clipboard image upload.
- Optional raw output logging.
- Reconnect prompt after disconnect unless disabled.

Acceptance:

- `Ctrl+C` sends interrupt instead of killing the local process.
- `Ctrl+D` sends EOF.
- `Ctrl+V` pastes clipboard text.
- `Ctrl+P` uploads clipboard image by SCP and types remote path.
- Local raw mode is restored after exit.

### FR-006 GUI connection form

The GUI shall provide fields for:

- Host.
- Private key path.
- Password.
- Save-as profile name.

Acceptance:

- Enter or Connect starts connection.
- Browse opens a file picker for key path.
- Connection errors are shown in the form.
- Disconnected tabs return to a connection form pre-filled with previous values.

### FR-007 GUI tabs

The GUI shall support multiple session tabs.

Required tab behavior:

- Add new tab.
- Close current tab.
- Close tabs to the right.
- Close other tabs.
- Assign a tab accent color.
- Search open tabs.

Acceptance:

- Active tab title reflects `user@host`.
- Closing a tab stops its SSH worker.
- Tab search filters by title and switches with Enter.

### FR-008 GUI focus mode

The GUI shall provide a focus mode that hides the sidebar and gives the terminal the full window width.

Acceptance:

- F11 toggles focus mode.
- Sidebar toggle button changes state.
- New connections use terminal dimensions computed from the visible layout.

### FR-009 GUI sync input

The GUI shall support broadcasting input from the active terminal to all active sessions.

Acceptance:

- Sync input can be toggled from the tab bar.
- Key input sent to the active session is also sent to other active sessions.
- Modal dialogs, search inputs, and tab search are not broadcast to sessions.

### FR-010 Terminal key mapping

Both GUI and CLI shall translate keys to VT-compatible byte sequences.

Required:

- Printable Unicode text.
- Ctrl letter combos.
- Alt key prefix.
- Shift+Tab.
- Arrow keys.
- Ctrl/Shift/Ctrl+Shift arrows.
- Home, End, Page Up, Page Down.
- Delete, Insert.
- F1 through F12.

Acceptance:

- Vim, tmux, shell line editing, and common TUIs receive expected sequences.

### FR-011 Remote output handling

The application shall filter or answer terminal control queries that interfere with local terminals or remote TUIs.

Required:

- Device Attributes replies.
- Device Status Report replies.
- Ordered CPR replies.
- CPR response stripping.
- OSC foreground, background, and cursor color query replies.
- Mouse mode detection.
- Bracketed paste mode detection.

Acceptance:

- Split escape sequences across SSH packets are buffered correctly.
- Unknown escape sequences pass through to the renderer.
- Tests cover split and coalesced terminal query sequences.

### FR-012 Bracketed paste

The application shall detect remote bracketed paste mode and wrap pasted text when enabled.

Acceptance:

- Single-line paste sends immediately.
- Multiline GUI paste opens a confirmation dialog.
- Dialog supports Send, Send line-by-line, and Cancel.

### FR-013 Text selection

The application shall support local text selection.

GUI acceptance:

- Click-drag selects terminal cells.
- `Ctrl+C` copies selection instead of sending interrupt.
- Enter copies selection.
- Right-click menu offers Copy.
- Selection clears when remote output changes.

CLI acceptance:

- Click-drag selects text.
- Enter copies selected text.
- Escape cancels selection.
- Shift-click forces local selection when remote mouse mode is enabled.

### FR-014 Mouse forwarding

The application shall forward mouse events to the remote app when remote mouse mode is enabled.

Acceptance:

- DEC private mode sequences enable and disable forwarding.
- SGR mouse protocol is used.
- Shift overrides forwarding and forces local selection.

### FR-015 GUI terminal rendering

The GUI shall render terminal output with:

- Monospace grid.
- ANSI colors.
- Cursor.
- Selection.
- Search match highlights.
- Bracket matching.
- Link/path highlights.
- Scrollback.
- Scrollbar controls.
- Line timestamp hover tooltip (Alt+hover over a row).
- Inline Kitty graphics: **not retained** in the redevelopment. The renderer
  is a `vt100` cell grid with no image-placement model; supporting the Kitty
  APC protocol would require anchoring decoded images to scrolling rows and
  risks the terminal-correctness invariants. Kitty sequences pass through
  unrendered.

Acceptance:

- Text remains cell-aligned.
- Long output does not block the UI permanently.
- Scrollback does not appear while alternate-screen apps own the terminal.

### FR-016 Search

The GUI shall support terminal scrollback search.

Acceptance:

- `Ctrl+Shift+F` toggles find bar.
- Enter goes to next match.
- Shift+Enter goes to previous match.
- Case-sensitive toggle is available.
- Match counter displays current and total.
- New terminal output invalidates cached matches.

### FR-017 Link and path actions

The GUI shall detect URLs and likely file paths in terminal rows.

Acceptance:

- Detected URLs are visually highlighted.
- Ctrl-click URL opens default browser.
- Ctrl-click path copies path to clipboard.
- Selected text can be searched online through configured search engines.

### FR-018 Clipboard image upload

The application shall upload the clipboard image to the remote machine using SCP.

Acceptance:

- `Ctrl+P` captures clipboard image.
- Image is encoded as PNG in temp storage.
- Image is uploaded as `clip_YYYYMMDD_HHMMSS.png`.
- Remote path is copied to local clipboard.
- In paste mode, remote path is typed into the shell.
- Temp file is deleted on success and failure.
- No image in clipboard produces a user-visible message.

### FR-019 Drag-and-drop upload

The GUI shall support dropping files or folders onto an active terminal.

Acceptance:

- Confirmation dialog lists dropped items and sizes.
- User can edit remote folder name.
- Upload occurs in a background thread.
- Remote folder path is copied to clipboard.
- Remote folder path is typed into shell.
- Temp staging directory is removed on success and failure.

### FR-020 Snippets

The GUI shall support saved command snippets.

Acceptance:

- User can add a snippet with name and command.
- User can delete snippets.
- Clicking a snippet sends the command plus newline to the active session.
- Snippets persist to `~/.ssh4.toml`.

### FR-021 Debug logging

The GUI shall support a debug log toggle.

Acceptance:

- Toggle is visible in sidebar.
- Log path is shown on hover.
- Open log folder button appears when enabled.
- Panic information is written even if debug logging is off.

### FR-022 CLI session logging

The CLI shall support `--log FILE` to append raw terminal output to a file.

Acceptance:

- Log file is appended, not overwritten.
- Logging survives reconnects.
- Log contents preserve raw ANSI bytes.

### FR-023 Help and discoverability

The GUI shall include an in-app help overlay for shortcuts and behaviors.

Acceptance:

- Help can be opened from connection and terminal states.
- Help does not forward keyboard input to remote session while open.

## 5. Target Requirements For Redevelopment

The following are target features gathered from `TODO.md` and existing help text. Treat them as future scope unless implemented during redevelopment.

### TR-001 Remote file tools

Provide an SCP/SFTP tools panel with:

- Remote directory listing.
- Download selected remote file or folder.
- Upload selected local file or folder.
- Auto-upload local file on save.
- Create remote file.
- Create remote folder.
- Rename or move remote path.
- Delete remote path.
- Chmod permissions editor.

Recommended implementation:

- Prefer SFTP if a reliable Rust crate is selected.
- If only SCP is available, use SSH shell commands for list, mkdir, touch, mv, rm, and chmod.
- Clearly label destructive operations and require confirmation for delete.

### TR-002 Local file manager

Provide local file operations:

- Browse local files.
- Copy, move, rename, delete.
- Create file and folder.
- Upload selected local path to remote.

### TR-003 Command palette

Provide keyboard-driven access to:

- Open tab.
- Close tab.
- Search tabs.
- Toggle focus mode.
- Toggle sync input.
- Run snippet.
- Open help.
- Open file tools.
- Change theme.

### TR-004 Theme system

Replace hardcoded theme literals with named tokens and selectable themes.

Acceptance:

- Theme file or built-in theme list exists.
- Terminal ANSI palette remains predictable.
- UI color roles are named and centralized.

## 6. Nonfunctional Requirements

### NFR-001 Responsiveness

The GUI shall remain responsive during high-throughput SSH output and uploads.

Acceptance:

- SSH I/O runs off the UI thread.
- Large writes are chunked.
- Output is batched.
- Per-frame processing limits prevent UI starvation.

### NFR-002 Reliability

The application shall clean up resources on disconnect or close.

Acceptance:

- SSH worker threads receive a stop signal.
- Shell and session are closed.
- Raw mode is disabled in CLI.
- Temp files and staging folders are removed.

### NFR-003 Portability

The application shall support Windows first and remain viable on Linux and macOS.

Acceptance:

- Windows console VT mode is enabled for CLI.
- Font loading uses cross-platform candidates.
- Config path uses `USERPROFILE` or `HOME`.
- File dialogs and open-folder behavior have OS-specific paths.

### NFR-004 Security

The application shall minimize credential exposure.

Acceptance:

- Warn about CLI password arguments.
- Avoid password logging.
- Protect config file permissions on Unix.
- Remove temp upload files.
- Confirm destructive remote file operations.

### NFR-005 Testability

Terminal protocol logic shall be unit-testable without a remote SSH server.

Acceptance:

- Host parsing tests.
- Terminal query parser tests.
- Bracketed paste tests.
- Key mapping tests.
- Config load/save tests using temp files.
- Transfer path construction tests.

## 7. Definition Of Product Parity

A redevelopment reaches current product parity when:

- CLI examples from `README.md` work.
- GUI opens with no arguments.
- A GUI SSH tab can connect, render output, accept input, resize, search, paste, and close.
- A CLI SSH session can connect, accept input, handle mouse mode, paste, upload clipboard image, and exit cleanly.
- Profiles and snippets persist.
- Clipboard image upload works in GUI and CLI.
- Drag-and-drop upload works in GUI.
- Unit tests for terminal protocol pass.
- `cargo fmt`, `cargo check`, and `cargo test` pass.
