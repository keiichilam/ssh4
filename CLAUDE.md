# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this repository is

This is a greenfield redevelopment of `ssh4`, a Rust SSH client with both GUI (eframe/egui) and CLI (crossterm) modes. The full implementation (parity milestones M2–M10 plus the TR-001..TR-004 target features) is in place under `src/`; four planning documents specify the target:

- `PRODUCT_REQUIREMENTS.md` — functional requirements (FR-001..FR-023), target features (TR-001..TR-004), nonfunctional requirements, and the definition of product parity.
- `SOFTWARE_DESIGN.md` — architecture: runtime modes, data model, channel messages, terminal protocol, threading, security, and performance limits.
- `TECHNICAL_IMPLEMENTATION.md` — module-by-module ownership, terminal protocol byte sequences, testing strategy, security/performance checklists.
- `REDEVELOPMENT_ROADMAP.md` — milestone order (M0–M12), feature parity matrix, risk register.

When implementing, treat these documents as the source of truth and follow the roadmap's milestone order. Update the docs in the same change as the code when behavior diverges.

## Commands

Standard Cargo workflow (once code exists):

```powershell
cargo fmt
cargo check
cargo test
cargo test <test_name>        # run a single test
cargo build --release
cargo run                     # GUI mode (default with no args)
cargo run -- user@host        # CLI mode
cargo run -- -P <profile>     # CLI mode from saved profile
```

Definition of done for any milestone: `cargo fmt`, `cargo check`, and `cargo test` all pass.

## Architecture

Two UI modes sharing a common core:

- **GUI mode** (default, no args): eframe/egui app with tabbed SSH sessions, `vt100::Parser` for terminal state, custom cell painting.
- **CLI mode** (host or `-P profile` given): crossterm raw-mode terminal loop.

Planned module layout (`TECHNICAL_IMPLEMENTATION.md` §3):

```
src/
  main.rs, cli.rs, config.rs, auth.rs, ssh_client.rs, transfer.rs,
  remote_fs.rs   (TR-001 remote file ops: quoting, ls parsing, exec/SCP)
  terminal/   input.rs, output.rs, mouse.rs, selection.rs, brackets.rs
  gui/        app.rs, session.rs, render.rs, sidebar.rs, tabs.rs,
              dialogs.rs, search.rs, theme.rs, files.rs
```

Key design invariants (don't violate these):

- **Threading**: the egui UI thread never blocks on network I/O. Each GUI tab owns one SSH worker thread; UI ↔ worker communicate only via channels (`GuiToSsh::Data/Resize` in, `SshMsg::Connected/Data/Heartbeat/Disconnected/Error` out). Stop flag is an `Arc<AtomicBool>` set on `Session` drop.
- **Terminal protocol is the hard part**: remote output is filtered before rendering — the app answers Device Attributes, DSR, CPR (in stream order), and OSC 10/11/12 queries, strips CPR responses, and scans for DEC mouse-mode and bracketed-paste enable/disable. Exact byte sequences are in `TECHNICAL_IMPLEMENTATION.md` §5. Split escape sequences across SSH reads must be buffered; unknown sequences pass through.
- **All terminal protocol logic must be unit-testable without an SSH server** — keep it out of UI and network code. Regression fixtures should split key escape sequences at every byte boundary.
- **Backpressure**: output batching, chunked writes (4096 B), coalesced resizes, and per-frame message/byte limits prevent GUI freezes under heavy output (constants in `SOFTWARE_DESIGN.md` §12).
- **Security**: never shell out to local `ssh`; never log passwords or key contents; don't persist passwords by default; zero password buffers; clean up temp upload files on success *and* failure (use guard patterns); confirm destructive remote operations; config at `~/.ssh4.toml` gets `0600` on Unix.
- **SSH library**: vendored `ssh-rs` 0.5.0 (pure Rust, path dependency at `vendor/ssh-rs-0.5.0`). Local modifications: `ChannelShell::window_change` (PTY resize), `ChannelShell::keepalive` (SSH_MSG_IGNORE keep-alive probe), resumable packet reads (`model/packet.rs` + `Client::recv_buf` + `Sequence::peek_server` — partial reads stay buffered across timeouts so the short interactive read timeout cannot desynchronize the transport; mid-packet EOF is a fatal `UnexpectedEof` instead of a spin; corrupt packet lengths over 1 MiB fail fast; `Data::uninit_new` removed), rekey hardening (`client_kex.rs`: key exchange runs under a ≥30 s timeout floor, timeout becomes a fatal `KexError`, unexpected transport messages mid-kex are skipped instead of `unreachable!()`), `TCP_NODELAY` on connect (`session/mod.rs`), and three `#[allow(dead_code)]` attributes silencing build warnings (`Encryption::iv_size` in `algorithm/encryption/mod.rs`, `IV_SIZE` in `algorithm/encryption/chacha20_poly1305_openssh.rs`, `ChannelRead::Code` field in `channel/local/channel.rs`). All are commented `ssh4 local modification` in source. Audit vendored modifications before replacing it.

Config persists profiles, snippets, ui_zoom, auto_fit, theme, and keep_alive to `~/.ssh4.toml` — keep the existing schema compatible.

## Implementation order

Follow `REDEVELOPMENT_ROADMAP.md`: terminal protocol core and tests first (M4), then CLI parity (M5), then GUI shell/sessions/renderer (M6–M8), then transfers (M9). All milestones through M11 (file tools, command palette, themes) are implemented; Kitty inline graphics was explicitly dropped (PRD FR-015). Remaining manual step: live-SSH smoke tests.

## Platform notes

Windows-first (PowerShell, `install.ps1`), but keep Linux/macOS viable: enable Windows console VT mode in CLI, resolve config path via `USERPROFILE` or `HOME`, use cross-platform font candidates.

<!-- rtk-instructions v2 -->
# RTK (Rust Token Killer) - Token-Optimized Commands

## Golden Rule

**Always prefix commands with `rtk`**. If RTK has a dedicated filter, it uses it. If not, it passes through unchanged. This means RTK is always safe to use.

**Important**: Even in command chains with `&&`, use `rtk`:
```bash
# ❌ Wrong
git add . && git commit -m "msg" && git push

# ✅ Correct
rtk git add . && rtk git commit -m "msg" && rtk git push
```

## RTK Commands by Workflow

### Build & Compile (80-90% savings)
```bash
rtk cargo build         # Cargo build output
rtk cargo check         # Cargo check output
rtk cargo clippy        # Clippy warnings grouped by file (80%)
rtk tsc                 # TypeScript errors grouped by file/code (83%)
rtk lint                # ESLint/Biome violations grouped (84%)
rtk prettier --check    # Files needing format only (70%)
rtk next build          # Next.js build with route metrics (87%)
```

### Test (60-99% savings)
```bash
rtk cargo test          # Cargo test failures only (90%)
rtk go test             # Go test failures only (90%)
rtk jest                # Jest failures only (99.5%)
rtk vitest              # Vitest failures only (99.5%)
rtk playwright test     # Playwright failures only (94%)
rtk pytest              # Python test failures only (90%)
rtk rake test           # Ruby test failures only (90%)
rtk rspec               # RSpec test failures only (60%)
rtk test <cmd>          # Generic test wrapper - failures only
```

### Git (59-80% savings)
```bash
rtk git status          # Compact status
rtk git log             # Compact log (works with all git flags)
rtk git diff            # Compact diff (80%)
rtk git show            # Compact show (80%)
rtk git add             # Ultra-compact confirmations (59%)
rtk git commit          # Ultra-compact confirmations (59%)
rtk git push            # Ultra-compact confirmations
rtk git pull            # Ultra-compact confirmations
rtk git branch          # Compact branch list
rtk git fetch           # Compact fetch
rtk git stash           # Compact stash
rtk git worktree        # Compact worktree
```

Note: Git passthrough works for ALL subcommands, even those not explicitly listed.

### GitHub (26-87% savings)
```bash
rtk gh pr view <num>    # Compact PR view (87%)
rtk gh pr checks        # Compact PR checks (79%)
rtk gh run list         # Compact workflow runs (82%)
rtk gh issue list       # Compact issue list (80%)
rtk gh api              # Compact API responses (26%)
```

### JavaScript/TypeScript Tooling (70-90% savings)
```bash
rtk pnpm list           # Compact dependency tree (70%)
rtk pnpm outdated       # Compact outdated packages (80%)
rtk pnpm install        # Compact install output (90%)
rtk npm run <script>    # Compact npm script output
rtk npx <cmd>           # Compact npx command output
rtk prisma              # Prisma without ASCII art (88%)
```

### Files & Search (60-75% savings)
```bash
rtk ls <path>           # Tree format, compact (65%)
rtk read <file>         # Code reading with filtering (60%)
rtk grep <pattern>      # Search grouped by file (75%). Format flags (-c, -l, -L, -o, -Z) run raw.
rtk find <pattern>      # Find grouped by directory (70%)
```

### Analysis & Debug (70-90% savings)
```bash
rtk err <cmd>           # Filter errors only from any command
rtk log <file>          # Deduplicated logs with counts
rtk json <file>         # JSON structure without values
rtk deps                # Dependency overview
rtk env                 # Environment variables compact
rtk summary <cmd>       # Smart summary of command output
rtk diff                # Ultra-compact diffs
```

### Infrastructure (85% savings)
```bash
rtk docker ps           # Compact container list
rtk docker images       # Compact image list
rtk docker logs <c>     # Deduplicated logs
rtk kubectl get         # Compact resource list
rtk kubectl logs        # Deduplicated pod logs
```

### Network (65-70% savings)
```bash
rtk curl <url>          # Compact HTTP responses (70%)
rtk wget <url>          # Compact download output (65%)
```

### Meta Commands
```bash
rtk gain                # View token savings statistics
rtk gain --history      # View command history with savings
rtk discover            # Analyze Claude Code sessions for missed RTK usage
rtk proxy <cmd>         # Run command without filtering (for debugging)
rtk init                # Add RTK instructions to CLAUDE.md
rtk init --global       # Add RTK to ~/.claude/CLAUDE.md
```

## Token Savings Overview

| Category | Commands | Typical Savings |
|----------|----------|-----------------|
| Tests | vitest, playwright, cargo test | 90-99% |
| Build | next, tsc, lint, prettier | 70-87% |
| Git | status, log, diff, add, commit | 59-80% |
| GitHub | gh pr, gh run, gh issue | 26-87% |
| Package Managers | pnpm, npm, npx | 70-90% |
| Files | ls, read, grep, find | 60-75% |
| Infrastructure | docker, kubectl | 85% |
| Network | curl, wget | 65-70% |

Overall average: **60-90% token reduction** on common development operations.
<!-- /rtk-instructions -->