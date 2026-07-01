# Redevelopment Roadmap - ssh4

Last updated: 2026-06-10

This roadmap gives an implementation order for rebuilding `ssh4` without losing current behavior.

## 1. Strategy

Redevelop in layers:

1. Preserve terminal protocol correctness.
2. Preserve SSH connection and transfer behavior.
3. Rebuild CLI parity.
4. Rebuild GUI session parity.
5. Add target features only after parity is stable.

Do not begin with remote file explorer or visual polish. The hardest and most fragile behavior is PTY fidelity: terminal query handling, key mapping, mouse forwarding, bracketed paste, and output buffering.

## 2. Milestone 0 - Baseline Inventory

Outcome:

- Current behavior is captured before rewriting.

Tasks:

- Run `cargo fmt`, `cargo check`, and `cargo test`.
- Record any failing tests or compiler warnings.
- Save screenshots or short notes for current GUI flows.
- Identify local changes in `vendor/ssh-rs-0.3.3`.
- Decide whether to keep vendored `ssh-rs` or replace it later.
- Mark source of truth for current vs target features.

Acceptance:

- A baseline test result is documented.
- Known dirty worktree files are understood.
- Current feature parity matrix is filled.

## 3. Milestone 1 - Project Restructure

Outcome:

- The codebase has module boundaries suitable for redevelopment.

Tasks:

- Create modules for config, auth, SSH client, terminal protocol, transfer, CLI, and GUI.
- Move data structures without behavior changes.
- Preserve public function signatures temporarily where useful.
- Keep current tests passing after each move.

Acceptance:

- `cargo check` passes.
- Existing unit tests pass.
- CLI and GUI still launch.

## 4. Milestone 2 - Config And Profiles

Outcome:

- Config behavior is isolated and tested.

Tasks:

- Implement `Config`, `Profile`, and `Snippet` in `config.rs`.
- Preserve `~/.ssh4.toml` schema.
- Add config path abstraction for tests.
- Add tests for load, save, defaults, snippets, and profiles.
- Decide password persistence policy.

Acceptance:

- Existing config files continue to load.
- Saving does not discard unknown future-compatible fields unless intentionally migrated.
- Unix config permissions are owner-only.

## 5. Milestone 3 - Auth And SSH Client

Outcome:

- SSH connection logic is isolated from UI.

Tasks:

- Implement host parsing.
- Implement default key discovery.
- Implement credential resolution.
- Implement `ConnParams`.
- Implement `connect_session`.
- Map common auth errors to friendly messages.
- Keep CLI prompting out of GUI flows.

Acceptance:

- Host parser tests pass.
- Key discovery order is tested.
- GUI auth failure returns an error string, not a blocking prompt.
- CLI can still connect to a real host.

## 6. Milestone 4 - Terminal Protocol Core

Outcome:

- Terminal protocol logic is testable without SSH.

Tasks:

- Extract key mapping.
- Extract paste payload builder.
- Extract SGR mouse encoder.
- Extract remote output parser.
- Extract mouse and bracketed paste mode scanners.
- Add regression tests for split escape sequences.

Acceptance:

- Current `process_remote_output` tests pass in the new module.
- Additional key mapping tests pass.
- Unknown escape sequences pass through.
- CPR requests are answered in stream order.

## 7. Milestone 5 - CLI Parity

Outcome:

- The rebuilt CLI mode reaches current behavior.

Tasks:

- Rebuild `clap` argument parsing.
- Rebuild CLI dispatch from `main.rs`.
- Rebuild raw terminal loop.
- Rebuild input thread.
- Rebuild session logger.
- Rebuild reconnect prompt.
- Rebuild one-shot upload integration.

Acceptance:

- `ssh4 user@host` works.
- `ssh4 -P profile` works.
- `--once` uploads clipboard image and exits.
- `--log` appends raw output.
- `Ctrl+C`, `Ctrl+D`, `Ctrl+V`, and `Ctrl+P` behave correctly.
- Raw mode is restored on exit.

## 8. Milestone 6 - GUI Shell

Outcome:

- The GUI app launches and manages tabs without active SSH behavior.

Tasks:

- Create eframe app shell.
- Implement theme module.
- Implement sidebar layout.
- Implement connection form.
- Implement tab bar.
- Implement modal routing.
- Implement display controls.
- Implement profiles and snippets UI against config module.

Acceptance:

- GUI opens with no arguments.
- New tab, close tab, focus mode, tab color, and tab search work.
- Profiles and snippets load and save.
- Help overlay opens and closes.

## 9. Milestone 7 - GUI SSH Sessions

Outcome:

- GUI tabs can connect to SSH sessions and exchange bytes.

Tasks:

- Implement `Session` and `PendingConn`.
- Implement SSH worker thread.
- Implement GUI-to-SSH and SSH-to-GUI channels.
- Implement output batching and heartbeat.
- Implement disconnect-to-form transition.
- Implement tab close cleanup.

Acceptance:

- GUI can connect to a remote shell.
- Remote output appears.
- User input reaches remote shell.
- Closing a tab stops the worker.
- Disconnect preserves reconnect values.

## 10. Milestone 8 - GUI Terminal Renderer

Outcome:

- GUI terminal rendering reaches current parity.

Tasks:

- Integrate `vt100::Parser`.
- Paint terminal cells.
- Paint ANSI colors.
- Paint cursor.
- Paint selection.
- Paint search highlights.
- Paint bracket matching.
- Paint links and paths.
- Paint scrollbar and support scrollback.
- Add line timestamp hover.
- Add Kitty image support if retained.

Acceptance:

- Basic shell output is readable.
- Full-screen TUIs render acceptably.
- Search can navigate scrollback.
- Selection copies expected text.
- Links and paths are highlighted and actionable.

## 11. Milestone 9 - Clipboard And Transfer

Outcome:

- GUI and CLI transfer workflows reach current parity.

Tasks:

- Implement clipboard text paste.
- Implement GUI paste dialog.
- Implement clipboard image upload.
- Implement right-click Paste Picture Path and Copy Picture Path.
- Implement drag-and-drop upload confirmation.
- Implement dropped-file staging and cleanup.
- Implement upload status messages.

Acceptance:

- Single-line paste sends immediately.
- Multiline paste opens dialog.
- Clipboard image upload works.
- Drag-and-drop upload works.
- Upload failures are shown without freezing UI.
- Temp files are cleaned after failures.

## 12. Milestone 10 - Interaction Polish

Outcome:

- Current GUI workflows feel complete.

Tasks:

- Implement sync input.
- Implement focus mode resizing behavior.
- Implement auto-hide mouse cursor while typing.
- Implement debug logging toggle and open log folder.
- Implement status bar counters.
- Implement visual failure, warning, and success highlights.
- Implement online search from selected text.

Acceptance:

- Modal input never leaks to remote shell.
- Sync input broadcasts only intended terminal input.
- Debug logs help diagnose connection stalls.
- Visual alerts request user attention.

## 13. Milestone 11 - Target File Tools

Outcome:

- Future SCP/SFTP tooling begins after parity is stable.

Tasks:

- Select transport: SFTP crate or SSH command plus SCP.
- Define remote file operation API.
- Implement remote listing.
- Implement upload and download.
- Implement create file/folder.
- Implement rename/move.
- Implement delete with confirmation.
- Implement chmod editor.
- Add path quoting or structured command execution.

Acceptance:

- Remote browser can navigate folders.
- Download selected path works.
- Upload selected path works.
- Delete requires confirmation.
- Remote command errors are visible.

## 14. Milestone 12 - Packaging And Release

Outcome:

- The rebuilt app can be installed and maintained.

Tasks:

- Update `install.ps1`.
- Verify release build on Windows.
- Verify Linux/macOS build if supported.
- Update README and HELP.
- Update all docs in `docs/`.
- Add release checklist.

Acceptance:

- `cargo build --release` creates a working binary.
- Installer script output matches current CLI flags.
- Docs match shipped behavior.

## 15. Feature Parity Matrix

| Feature | Current source evidence | Rebuild milestone | Status |
| --- | --- | --- | --- |
| GUI default launch | `main.rs` dispatch | 6 | Implemented |
| CLI host/profile launch | `main.rs` CLI | 5 | Implemented |
| Profiles | `Config`, GUI sidebar | 2, 6 | Implemented |
| Snippets | `Config`, GUI sidebar | 2, 6 | Implemented |
| SSH shell | `connect_session`, workers | 3, 5, 7 | Implemented |
| Terminal output filtering | `process_remote_output_events` | 4 | Implemented (`terminal/output.rs`, tested) |
| Key mapping | `key_to_bytes`, `egui_ev_to_vt100` | 4, 5, 9 | Implemented (`terminal/input.rs`, tested) |
| Mouse forwarding | `scan_remote_flags`, SGR helpers | 4, 5, 9 | Implemented (`terminal/mouse.rs`, tested) |
| Bracketed paste | mode scanner, paste payload | 4, 5, 9 | Implemented (tested) |
| GUI tabs | `Tab`, tab bar | 6 | Implemented |
| GUI renderer | `render_screen` | 8 | Implemented (bracket matching, line timestamp hover; Kitty graphics dropped — see PRD FR-015) |
| GUI search | `SearchState` | 8 | Implemented |
| Clipboard image upload | `run_one_shot` and GUI helper | 9 | Implemented |
| Drag-and-drop upload | GUI upload helpers | 9 | Implemented |
| Debug logging | GUI debug helpers | 10 | Implemented (toggle + panic logger) |
| Interaction polish | sync input, focus mode, cursor auto-hide, online search | 10 | Implemented |
| Rich SCP tools | docs/TODO target, not current source | 11 | Implemented (`remote_fs.rs`, `gui/files.rs`: list/upload/download/create/rename/chmod/delete-with-confirm, edit-with-auto-upload) |
| Local file manager | TODO target | 11 | Implemented (local pane of the File Tools window) |
| Command palette | TODO target | 11 | Implemented (Ctrl+Shift+K) |
| Theme switching | TODO/design audit target | 11 | Implemented (Lavender / Amber Phosphor / Green Phosphor, persisted) |

## 16. Risk Register

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Terminal protocol regressions | Remote shells and TUIs behave incorrectly | Build parser tests first; run split-sequence fixtures |
| GUI freezes under large output | Poor usability | Preserve worker thread, batching, and frame limits |
| Credential leakage | Security issue | Centralize secret handling and logging policy |
| Temp file leaks | Disk clutter and data exposure | Use cleanup guards for every transfer |
| Remote file tools use unsafe shell strings | Data loss or command injection | Prefer SFTP or robust path quoting and confirmations |
| Replacing `ssh-rs` changes behavior | Auth or PTY regressions | Audit vendored changes and test before replacement |
| Docs drift from implementation | Bad redevelopment decisions | Update docs with code changes |

## 17. Backlog From Existing Notes

From `TODO.md`:

- Integrated SFTP/SCP client.
- Integrated local file manager.
- Unicode 13 support.
- Session dialog and session tree.
- Auto completion.
- Free type mode.
- Focus mode.
- Sync input.
- Command palette.
- Command sender.
- Explorer pane.
- Shell pane.
- Quick bar.
- Paste dialog.
- Local and remote modes with vim keybindings.
- Timestamps, folding, outlining, split views.
- Powerline prompt support.
- VS Code-like color schemes.
- Search and preview.
- Bracket and custom delimiter matching.
- Theme changing.
- Tab color and tab search.
- Window transparency.
- Select-to-copy and paste gestures.
- Online search engines.
- Hide mouse cursor while typing.

From `design-system-audit.md`:

- Replace repeated raw color literals with tokens.
- Add surface scale.
- Add overlay tokens.
- Consolidate semantic colors.
- Add radius scale.
- Add UI type scale.
- Normalize color naming.

## 18. Definition Of Done

For parity release:

- `cargo fmt` passes.
- `cargo check` passes.
- `cargo test` passes.
- GUI launches with no arguments.
- CLI launches with host and profile arguments.
- Manual SSH smoke test passes in GUI and CLI.
- Clipboard text paste works in GUI and CLI.
- Clipboard image upload works in GUI and CLI.
- GUI drag-and-drop upload works.
- Profiles and snippets persist.
- Terminal protocol regression tests cover query handling.
- README, HELP, and docs are updated.

For target feature release:

- Parity release is complete.
- Remote file tools have tests or repeatable manual test scripts.
- Destructive remote operations require confirmation.
- Documentation identifies which target features are now shipped.
