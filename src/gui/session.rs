//! GUI session state and the SSH worker thread. The UI thread and the worker
//! communicate only via channels; the worker never touches GUI state.

use crate::ssh_client::{self, ConnParams, READ_TIMEOUT, SSH_WRITE_CHUNK};
use crate::terminal::output::{cpr_response, OutputParser, RemoteOutputEvent};
use crate::terminal::selection::Selection;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

// Backpressure limits (SOFTWARE_DESIGN.md §12).
const WRITE_CHUNKS_PER_TICK: usize = 4;
const INPUT_DRAIN_PER_TICK: usize = 64;
const FLUSH_THRESHOLD: usize = 64 * 1024;
const FLUSH_INTERVAL: Duration = Duration::from_millis(16);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);
const MAX_MSGS_PER_FRAME: usize = 128;
const MAX_BYTES_PER_FRAME: usize = 256 * 1024;
pub const SCROLLBACK_LINES: usize = 1000;

#[derive(Debug)]
pub enum GuiToSsh {
    Data(Vec<u8>),
    Resize {
        cols: u16,
        rows: u16,
    },
    /// Toggle periodic SSH_MSG_IGNORE keep-alive probes.
    KeepAlive(bool),
}

#[derive(Debug)]
pub enum SshMsg {
    Connected {
        cols: u16,
        rows: u16,
    },
    Data(Vec<u8>),
    Heartbeat {
        pending_chunks: usize,
        buffered_output: usize,
        read_timeouts: u64,
    },
    Disconnected,
    Error(String),
}

/// Editable connection form state (also used for reconnect pre-fill).
#[derive(Debug, Clone, Default)]
pub struct PendingConn {
    /// `[user@]host[:port]` as typed in the form.
    pub host: String,
    pub key_path: String,
    pub password: String,
    pub save_as: String,
    pub error: Option<String>,
    pub connecting: bool,
}

pub struct HeartbeatInfo {
    pub at: Instant,
    pub pending_chunks: usize,
    pub buffered_output: usize,
    pub read_timeouts: u64,
}

pub struct Session {
    pub parser: vt100::Parser,
    pub tx: Sender<GuiToSsh>,
    rx: Receiver<SshMsg>,
    stop: Arc<AtomicBool>,
    pub cols: u16,
    pub rows: u16,
    pub identity: String,
    pub params: ConnParams,
    pub remote_dir: String,
    pub out_parser: OutputParser,
    pub scrollback: usize,
    pub sent: u64,
    pub received: u64,
    pub last_data_at: Instant,
    pub heartbeat: Option<HeartbeatInfo>,
    pub connected: bool,
    pub disconnected: bool,
    pub error: Option<String>,
    pub selection: Selection,
    pub status: Option<(String, bool, Instant)>,
    /// Bumped whenever new terminal data arrives (invalidates search cache).
    pub generation: u64,
    /// Pointer hidden over the terminal while typing; cleared on pointer move.
    pub hide_pointer: bool,
    /// Wall-clock time each live-screen row last changed (FR-015 hover).
    row_times: Vec<Option<chrono::DateTime<chrono::Local>>>,
    row_hashes: Vec<u64>,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Session {
    pub fn connect(
        params: ConnParams,
        remote_dir: String,
        cols: u16,
        rows: u16,
        keep_alive: bool,
        ctx: egui::Context,
    ) -> Self {
        let (tx_in, rx_in) = std::sync::mpsc::channel::<GuiToSsh>();
        let (tx_out, rx_out) = std::sync::mpsc::channel::<SshMsg>();
        let stop = Arc::new(AtomicBool::new(false));
        let identity = params.identity();

        {
            let params = params.clone();
            let stop = stop.clone();
            std::thread::spawn(move || {
                worker(params, cols, rows, keep_alive, stop, tx_out, rx_in, ctx)
            });
        }

        Session {
            parser: vt100::Parser::new(rows, cols, SCROLLBACK_LINES),
            tx: tx_in,
            rx: rx_out,
            stop,
            cols,
            rows,
            identity,
            params,
            remote_dir,
            out_parser: OutputParser::new(),
            scrollback: 0,
            sent: 0,
            received: 0,
            last_data_at: Instant::now(),
            heartbeat: None,
            connected: false,
            disconnected: false,
            error: None,
            selection: Selection::default(),
            status: None,
            generation: 0,
            hide_pointer: false,
            row_times: vec![None; rows as usize],
            row_hashes: vec![0; rows as usize],
        }
    }

    pub fn send_input(&mut self, bytes: Vec<u8>) {
        self.sent += bytes.len() as u64;
        self.tx.send(GuiToSsh::Data(bytes)).ok();
    }

    /// Toggle keep-alive probes on the live worker.
    pub fn set_keep_alive(&mut self, on: bool) {
        self.tx.send(GuiToSsh::KeepAlive(on)).ok();
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        if cols == self.cols && rows == self.rows {
            return;
        }
        self.cols = cols;
        self.rows = rows;
        self.parser.set_size(rows, cols);
        // Reflow invalidates per-row stamps.
        self.row_times = vec![None; rows as usize];
        self.row_hashes = vec![0; rows as usize];
        self.tx.send(GuiToSsh::Resize { cols, rows }).ok();
    }

    /// When the given live-screen row last changed.
    pub fn row_time(&self, row: usize) -> Option<chrono::DateTime<chrono::Local>> {
        self.row_times.get(row).copied().flatten()
    }

    /// Re-hash live-screen rows and stamp the ones that changed.
    fn stamp_rows(&mut self) {
        use std::hash::{Hash, Hasher};
        let now = chrono::Local::now();
        let screen = self.parser.screen();
        for row in 0..self.rows.min(self.row_hashes.len() as u16) {
            let text = screen.contents_between(row, 0, row + 1, 0);
            let mut h = std::collections::hash_map::DefaultHasher::new();
            text.hash(&mut h);
            let hash = h.finish();
            let slot = row as usize;
            if self.row_hashes[slot] != hash {
                self.row_hashes[slot] = hash;
                self.row_times[slot] = Some(now);
            }
        }
    }

    pub fn set_status(&mut self, msg: impl Into<String>, is_ok: bool) {
        self.status = Some((msg.into(), is_ok, Instant::now()));
    }

    /// Drain worker messages with per-frame limits. Returns true if the frame
    /// limit was hit (caller should request another repaint).
    pub fn poll(&mut self) -> bool {
        let mut msgs = 0;
        let mut bytes = 0;
        let mut hit_limit = false;
        let mut got_data = false;
        loop {
            if msgs >= MAX_MSGS_PER_FRAME || bytes >= MAX_BYTES_PER_FRAME {
                hit_limit = true;
                break;
            }
            match self.rx.try_recv() {
                Ok(msg) => {
                    msgs += 1;
                    match msg {
                        SshMsg::Connected { cols, rows } => {
                            self.connected = true;
                            self.cols = cols;
                            self.rows = rows;
                        }
                        SshMsg::Data(data) => {
                            bytes += data.len();
                            self.received += data.len() as u64;
                            self.last_data_at = Instant::now();
                            self.selection.clear();
                            self.generation += 1;
                            got_data = true;
                            for ev in self.out_parser.process(&data) {
                                match ev {
                                    RemoteOutputEvent::Data(d) => self.parser.process(&d),
                                    RemoteOutputEvent::Response(r) => {
                                        self.tx.send(GuiToSsh::Data(r)).ok();
                                    }
                                    RemoteOutputEvent::Cpr => {
                                        let (r, c) = self.parser.screen().cursor_position();
                                        self.tx
                                            .send(GuiToSsh::Data(cpr_response(r + 1, c + 1)))
                                            .ok();
                                    }
                                }
                            }
                        }
                        SshMsg::Heartbeat {
                            pending_chunks,
                            buffered_output,
                            read_timeouts,
                        } => {
                            self.heartbeat = Some(HeartbeatInfo {
                                at: Instant::now(),
                                pending_chunks,
                                buffered_output,
                                read_timeouts,
                            });
                        }
                        SshMsg::Disconnected => {
                            self.connected = false;
                            self.disconnected = true;
                        }
                        SshMsg::Error(e) => {
                            self.connected = false;
                            self.disconnected = true;
                            self.error = Some(e);
                        }
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if !self.disconnected {
                        self.connected = false;
                        self.disconnected = true;
                    }
                    break;
                }
            }
        }
        if got_data {
            self.stamp_rows();
        }
        hit_limit
    }
}

/// SSH worker thread body: owns the SSH session and shell channel.
#[allow(clippy::too_many_arguments)]
fn worker(
    params: ConnParams,
    cols: u16,
    rows: u16,
    keep_alive: bool,
    stop: Arc<AtomicBool>,
    tx: Sender<SshMsg>,
    rx: Receiver<GuiToSsh>,
    ctx: egui::Context,
) {
    let send = |msg: SshMsg| {
        let ok = tx.send(msg).is_ok();
        ctx.request_repaint();
        ok
    };

    let mut session = match ssh_client::connect_session(&params) {
        Ok(s) => s,
        Err(e) => {
            send(SshMsg::Error(e));
            return;
        }
    };
    let mut shell = match ssh_client::open_shell(&mut session, cols, rows) {
        Ok(s) => s,
        Err(e) => {
            send(SshMsg::Error(e));
            session.close();
            return;
        }
    };
    session.set_timeout(Some(READ_TIMEOUT));
    send(SshMsg::Connected { cols, rows });

    let mut pending_writes: VecDeque<Vec<u8>> = VecDeque::new();
    let mut pending_resize: Option<(u16, u16)> = None;
    let mut out_buf: Vec<u8> = Vec::new();
    let mut last_flush = Instant::now();
    let mut last_heartbeat = Instant::now();
    let mut read_timeouts: u64 = 0;
    let mut keep_alive = keep_alive;
    let mut last_keepalive = Instant::now();

    let disconnect_reason: Option<String> = loop {
        if stop.load(Ordering::Relaxed) {
            break None;
        }

        // Drain a bounded number of input messages; coalesce resizes.
        for _ in 0..INPUT_DRAIN_PER_TICK {
            match rx.try_recv() {
                Ok(GuiToSsh::Data(data)) => {
                    for chunk in data.chunks(SSH_WRITE_CHUNK) {
                        pending_writes.push_back(chunk.to_vec());
                    }
                }
                Ok(GuiToSsh::Resize { cols, rows }) => pending_resize = Some((cols, rows)),
                Ok(GuiToSsh::KeepAlive(on)) => {
                    keep_alive = on;
                    last_keepalive = Instant::now();
                }
                Err(_) => break,
            }
        }

        // Apply the latest resize before reading to reduce drag lag.
        if let Some((c, r)) = pending_resize.take() {
            if let Err(e) = shell.window_change(ssh::TerminalSize::from(c as u32, r as u32)) {
                break Some(ssh_client::friendly_error(&e));
            }
        }

        // Bounded writes per tick.
        let mut write_err: Option<String> = None;
        for _ in 0..WRITE_CHUNKS_PER_TICK {
            let Some(chunk) = pending_writes.pop_front() else {
                break;
            };
            if let Err(e) = shell.write(&chunk) {
                write_err = Some(ssh_client::friendly_error(&e));
                break;
            }
        }
        if let Some(e) = write_err {
            break Some(e);
        }

        // Read remote output with the short timeout.
        match shell.read() {
            Ok(data) if !data.is_empty() => {
                out_buf.extend_from_slice(&data);
            }
            Ok(_) => {}
            Err(ssh::SshError::TimeoutError) => read_timeouts += 1,
            Err(e) => break Some(ssh_client::friendly_error(&e)),
        }

        // Flush buffered output on threshold or interval.
        if !out_buf.is_empty()
            && (out_buf.len() >= FLUSH_THRESHOLD || last_flush.elapsed() >= FLUSH_INTERVAL)
        {
            if !send(SshMsg::Data(std::mem::take(&mut out_buf))) {
                break None;
            }
            last_flush = Instant::now();
        }

        // Keep-alive probe: SSH_MSG_IGNORE, discarded by the server but
        // keeping NAT/firewall state fresh on idle sessions.
        if keep_alive && last_keepalive.elapsed() >= KEEPALIVE_INTERVAL {
            if let Err(e) = shell.keepalive() {
                break Some(ssh_client::friendly_error(&e));
            }
            last_keepalive = Instant::now();
        }

        if last_heartbeat.elapsed() >= HEARTBEAT_INTERVAL {
            send(SshMsg::Heartbeat {
                pending_chunks: pending_writes.len(),
                buffered_output: out_buf.len(),
                read_timeouts,
            });
            last_heartbeat = Instant::now();
        }

        if shell.closed() {
            break None;
        }
    };

    // Flush remaining output, then report shutdown.
    if !out_buf.is_empty() {
        send(SshMsg::Data(out_buf));
    }
    match disconnect_reason {
        Some(e) if !stop.load(Ordering::Relaxed) => {
            // Reads racing a deliberate stop are not errors.
            if e.contains("Timeout") {
                send(SshMsg::Disconnected);
            } else {
                send(SshMsg::Error(e));
            }
        }
        _ => {
            send(SshMsg::Disconnected);
        }
    }
    shell.close().ok();
    session.close();
}
