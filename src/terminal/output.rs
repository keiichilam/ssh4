//! Remote output filtering. Remote bytes are parsed into ordered events:
//! clean data for the renderer, responses to send back to the remote shell,
//! and ordered CPR markers. Split escape sequences across reads are buffered;
//! unknown sequences pass through untouched.

/// Reply to `ESC[c` / `ESC[0c` (Primary Device Attributes): VT220-class.
pub const DA1_REPLY: &[u8] = b"\x1b[?62;1;6c";
/// Reply to `ESC[>c` / `ESC[>0c` (Secondary Device Attributes).
pub const DA2_REPLY: &[u8] = b"\x1b[>1;0;0c";
/// Reply to `ESC[5n` (Device Status Report): terminal OK.
pub const DSR_OK_REPLY: &[u8] = b"\x1b[0n";

/// Bound on buffered incomplete escape sequences. Anything longer is flushed
/// through as plain data so a malformed stream cannot grow the buffer forever.
const MAX_PENDING: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteOutputEvent {
    /// Clean bytes for the renderer / local terminal.
    Data(Vec<u8>),
    /// Bytes that must be written back to the remote shell.
    Response(Vec<u8>),
    /// Ordered CPR (`ESC[6n`) marker: the caller must feed all preceding
    /// `Data` to its parser first, then answer with the current cursor
    /// position via [`cpr_response`].
    Cpr,
}

/// Build a CPR answer for a 1-based cursor position.
pub fn cpr_response(row: u16, col: u16) -> Vec<u8> {
    format!("\x1b[{row};{col}R").into_bytes()
}

/// Format an OSC color reply, echoing the query's terminator style.
fn osc_color_reply(code: &str, rgb: (u8, u8, u8), bel_terminated: bool) -> Vec<u8> {
    let (r, g, b) = rgb;
    let mut out =
        format!("\x1b]{code};rgb:{r:02x}{r:02x}/{g:02x}{g:02x}/{b:02x}{b:02x}").into_bytes();
    if bel_terminated {
        out.push(0x07);
    } else {
        out.extend_from_slice(b"\x1b\\");
    }
    out
}

#[derive(Debug)]
pub struct OutputParser {
    /// Buffered tail of an incomplete escape sequence from the previous read.
    pending: Vec<u8>,
    /// Remote app has enabled DEC mouse reporting (1000/1002/1003/1006).
    pub mouse_mode: bool,
    /// Remote app has enabled bracketed paste (2004).
    pub bracketed_paste: bool,
    /// Colors reported for OSC 10 (fg), 11 (bg), 12 (cursor) queries.
    pub fg_color: (u8, u8, u8),
    pub bg_color: (u8, u8, u8),
    pub cursor_color: (u8, u8, u8),
}

impl Default for OutputParser {
    fn default() -> Self {
        Self {
            pending: Vec::new(),
            mouse_mode: false,
            bracketed_paste: false,
            fg_color: (0xd4, 0xd4, 0xd4),
            bg_color: (0x1e, 0x1e, 0x1e),
            cursor_color: (0xff, 0xcc, 0x00),
        }
    }
}

/// Accumulates events, merging consecutive data bytes into one `Data` chunk.
struct EventSink {
    events: Vec<RemoteOutputEvent>,
    data: Vec<u8>,
}

impl EventSink {
    fn new() -> Self {
        Self {
            events: Vec::new(),
            data: Vec::new(),
        }
    }
    fn data(&mut self, bytes: &[u8]) {
        self.data.extend_from_slice(bytes);
    }
    fn flush_data(&mut self) {
        if !self.data.is_empty() {
            self.events
                .push(RemoteOutputEvent::Data(std::mem::take(&mut self.data)));
        }
    }
    fn response(&mut self, bytes: Vec<u8>) {
        self.flush_data();
        self.events.push(RemoteOutputEvent::Response(bytes));
    }
    fn cpr(&mut self) {
        self.flush_data();
        self.events.push(RemoteOutputEvent::Cpr);
    }
    fn finish(mut self) -> Vec<RemoteOutputEvent> {
        self.flush_data();
        self.events
    }
}

/// Result of trying to consume one escape sequence at `buf[i..]` (buf[i] == ESC).
enum SeqResult {
    /// Sequence handled or passed through; advance by `len`.
    Consumed(usize),
    /// Not enough bytes to decide; keep `buf[i..]` pending.
    Incomplete,
}

impl OutputParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a chunk of remote output, returning ordered events.
    pub fn process(&mut self, input: &[u8]) -> Vec<RemoteOutputEvent> {
        let buf: Vec<u8> = if self.pending.is_empty() {
            input.to_vec()
        } else {
            let mut b = std::mem::take(&mut self.pending);
            b.extend_from_slice(input);
            b
        };

        let mut sink = EventSink::new();
        let mut i = 0;
        while i < buf.len() {
            if buf[i] != 0x1b {
                // Fast path: copy a run of plain bytes up to the next ESC.
                let next_esc = buf[i..]
                    .iter()
                    .position(|&b| b == 0x1b)
                    .map(|p| i + p)
                    .unwrap_or(buf.len());
                sink.data(&buf[i..next_esc]);
                i = next_esc;
                continue;
            }
            match self.consume_escape(&buf[i..], &mut sink) {
                SeqResult::Consumed(len) => i += len,
                SeqResult::Incomplete => {
                    if buf.len() - i > MAX_PENDING {
                        // Oversized incomplete sequence: give up and flush.
                        sink.data(&buf[i..]);
                        i = buf.len();
                    } else {
                        self.pending = buf[i..].to_vec();
                        i = buf.len();
                    }
                }
            }
        }
        sink.finish()
    }

    /// `seq[0] == ESC`. Try to consume one full escape sequence.
    fn consume_escape(&mut self, seq: &[u8], sink: &mut EventSink) -> SeqResult {
        if seq.len() < 2 {
            return SeqResult::Incomplete;
        }
        match seq[1] {
            b'[' => self.consume_csi(seq, sink),
            b']' => self.consume_osc(seq, sink),
            // Other ESC-prefixed sequences (charset selection, DECSC, ...)
            // pass through as data; the renderer's vt100 parser handles them.
            _ => {
                sink.data(&seq[..2]);
                SeqResult::Consumed(2)
            }
        }
    }

    /// CSI: `ESC [ <params 0x20..=0x3f> <final 0x40..=0x7e>`.
    fn consume_csi(&mut self, seq: &[u8], sink: &mut EventSink) -> SeqResult {
        let mut j = 2;
        while j < seq.len() && (0x20..=0x3f).contains(&seq[j]) {
            j += 1;
        }
        if j >= seq.len() {
            return SeqResult::Incomplete;
        }
        let final_byte = seq[j];
        if !(0x40..=0x7e).contains(&final_byte) {
            // Malformed CSI: pass the ESC through and resync on the next byte.
            sink.data(&seq[..1]);
            return SeqResult::Consumed(1);
        }
        let params = &seq[2..j];
        let len = j + 1;
        let handled = self.handle_csi(params, final_byte, sink);
        if !handled {
            sink.data(&seq[..len]);
        }
        SeqResult::Consumed(len)
    }

    /// Returns true when the sequence was intercepted (not passed through).
    fn handle_csi(&mut self, params: &[u8], final_byte: u8, sink: &mut EventSink) -> bool {
        match final_byte {
            b'c' => match params {
                b"" | b"0" => {
                    sink.response(DA1_REPLY.to_vec());
                    true
                }
                b">" | b">0" => {
                    sink.response(DA2_REPLY.to_vec());
                    true
                }
                _ => false,
            },
            b'n' => match params {
                b"5" => {
                    sink.response(DSR_OK_REPLY.to_vec());
                    true
                }
                b"6" => {
                    sink.cpr();
                    true
                }
                _ => false,
            },
            // CPR responses (`ESC[<digits>;<digits>R`) echoed back by the
            // remote are stripped so they never reach the renderer.
            b'R' => {
                let s = params;
                if let Some(semi) = s.iter().position(|&b| b == b';') {
                    let (a, b) = (&s[..semi], &s[semi + 1..]);
                    if !a.is_empty()
                        && !b.is_empty()
                        && a.iter().all(u8::is_ascii_digit)
                        && b.iter().all(u8::is_ascii_digit)
                    {
                        return true;
                    }
                }
                false
            }
            // DEC private mode set/reset: scan for mouse and bracketed paste
            // modes, then pass the sequence through to the renderer.
            b'h' | b'l' => {
                if params.first() == Some(&b'?') {
                    let enable = final_byte == b'h';
                    for part in params[1..].split(|&b| b == b';') {
                        match part {
                            b"1000" | b"1002" | b"1003" | b"1006" => self.mouse_mode = enable,
                            b"2004" => self.bracketed_paste = enable,
                            _ => {}
                        }
                    }
                }
                false
            }
            _ => false,
        }
    }

    /// OSC: `ESC ] ... (BEL | ESC \)`.
    fn consume_osc(&mut self, seq: &[u8], sink: &mut EventSink) -> SeqResult {
        let mut j = 2;
        let (content_end, term_len, bel) = loop {
            if j >= seq.len() {
                return SeqResult::Incomplete;
            }
            match seq[j] {
                0x07 => break (j, 1, true),
                0x1b => {
                    if j + 1 >= seq.len() {
                        return SeqResult::Incomplete;
                    }
                    if seq[j + 1] == b'\\' {
                        break (j, 2, false);
                    }
                    // ESC inside OSC that isn't ST: treat as malformed,
                    // pass leading ESC through and resync.
                    sink.data(&seq[..1]);
                    return SeqResult::Consumed(1);
                }
                _ => j += 1,
            }
        };
        let len = content_end + term_len;
        let content = &seq[2..content_end];
        let handled = self.handle_osc(content, bel, sink);
        if !handled {
            sink.data(&seq[..len]);
        }
        SeqResult::Consumed(len)
    }

    /// Answer OSC 10/11/12 color queries (`ESC]10;?BEL`). Returns true when
    /// the query was intercepted.
    fn handle_osc(&mut self, content: &[u8], bel: bool, sink: &mut EventSink) -> bool {
        let Ok(text) = std::str::from_utf8(content) else {
            return false;
        };
        let Some((code, arg)) = text.split_once(';') else {
            return false;
        };
        if arg != "?" {
            return false;
        }
        let rgb = match code {
            "10" => self.fg_color,
            "11" => self.bg_color,
            "12" => self.cursor_color,
            _ => return false,
        };
        sink.response(osc_color_reply(code, rgb, bel));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use RemoteOutputEvent::*;

    fn one_shot(input: &[u8]) -> Vec<RemoteOutputEvent> {
        OutputParser::new().process(input)
    }

    /// Feed `input` split at every possible byte boundary and assert the
    /// combined events match the unsplit result.
    fn assert_split_invariant(input: &[u8]) {
        let expected = one_shot(input);
        for cut in 1..input.len() {
            let mut p = OutputParser::new();
            let mut got = p.process(&input[..cut]);
            got.extend(p.process(&input[cut..]));
            // Merge adjacent Data events for comparison.
            let merged = merge_data(got);
            assert_eq!(
                merged,
                merge_data(expected.clone()),
                "split at byte {cut} diverged"
            );
        }
    }

    fn merge_data(events: Vec<RemoteOutputEvent>) -> Vec<RemoteOutputEvent> {
        let mut out: Vec<RemoteOutputEvent> = Vec::new();
        for ev in events {
            match (out.last_mut(), ev) {
                (Some(Data(prev)), Data(next)) => prev.extend_from_slice(&next),
                (_, ev) => out.push(ev),
            }
        }
        out
    }

    #[test]
    fn plain_text_passes_through() {
        assert_eq!(one_shot(b"hello"), vec![Data(b"hello".to_vec())]);
    }

    #[test]
    fn da1_query_answered_and_stripped() {
        assert_eq!(
            one_shot(b"ab\x1b[ccd"),
            vec![
                Data(b"ab".to_vec()),
                Response(DA1_REPLY.to_vec()),
                Data(b"cd".to_vec())
            ]
        );
        assert_eq!(one_shot(b"\x1b[0c"), vec![Response(DA1_REPLY.to_vec())]);
    }

    #[test]
    fn da2_query_answered() {
        assert_eq!(one_shot(b"\x1b[>c"), vec![Response(DA2_REPLY.to_vec())]);
        assert_eq!(one_shot(b"\x1b[>0c"), vec![Response(DA2_REPLY.to_vec())]);
    }

    #[test]
    fn dsr_answered() {
        assert_eq!(one_shot(b"\x1b[5n"), vec![Response(DSR_OK_REPLY.to_vec())]);
    }

    #[test]
    fn cpr_request_is_ordered_event() {
        assert_eq!(
            one_shot(b"before\x1b[6nafter"),
            vec![Data(b"before".to_vec()), Cpr, Data(b"after".to_vec())]
        );
    }

    #[test]
    fn cpr_response_is_stripped() {
        assert_eq!(one_shot(b"ab\x1b[12;45Rcd"), vec![Data(b"abcd".to_vec())]);
    }

    #[test]
    fn non_cpr_r_passes_through() {
        // `ESC[2R` lacks the digits;digits shape — pass through.
        assert_eq!(one_shot(b"\x1b[2R"), vec![Data(b"\x1b[2R".to_vec())]);
    }

    #[test]
    fn unknown_csi_passes_through() {
        assert_eq!(
            one_shot(b"\x1b[31mred\x1b[0m"),
            vec![Data(b"\x1b[31mred\x1b[0m".to_vec())]
        );
        assert_eq!(one_shot(b"\x1b[2J"), vec![Data(b"\x1b[2J".to_vec())]);
    }

    #[test]
    fn mouse_mode_scanning() {
        let mut p = OutputParser::new();
        let ev = p.process(b"\x1b[?1000h");
        assert!(p.mouse_mode);
        // Mode sequences pass through to the renderer.
        assert_eq!(ev, vec![Data(b"\x1b[?1000h".to_vec())]);
        p.process(b"\x1b[?1000l");
        assert!(!p.mouse_mode);
        for seq in [&b"\x1b[?1002h"[..], b"\x1b[?1003h", b"\x1b[?1006h"] {
            let mut p = OutputParser::new();
            p.process(seq);
            assert!(p.mouse_mode, "expected {seq:?} to enable mouse mode");
        }
    }

    #[test]
    fn bracketed_paste_scanning() {
        let mut p = OutputParser::new();
        p.process(b"\x1b[?2004h");
        assert!(p.bracketed_paste);
        p.process(b"\x1b[?2004l");
        assert!(!p.bracketed_paste);
    }

    #[test]
    fn combined_private_modes() {
        let mut p = OutputParser::new();
        p.process(b"\x1b[?1006;2004h");
        assert!(p.mouse_mode);
        assert!(p.bracketed_paste);
    }

    #[test]
    fn osc_color_query_bel() {
        let ev = one_shot(b"\x1b]11;?\x07");
        assert_eq!(ev.len(), 1);
        let Response(r) = &ev[0] else {
            panic!("expected response")
        };
        assert!(r.starts_with(b"\x1b]11;rgb:"));
        assert_eq!(*r.last().unwrap(), 0x07);
    }

    #[test]
    fn osc_color_query_st() {
        let ev = one_shot(b"\x1b]10;?\x1b\\");
        let Response(r) = &ev[0] else {
            panic!("expected response")
        };
        assert!(r.starts_with(b"\x1b]10;rgb:"));
        assert!(r.ends_with(b"\x1b\\"));
    }

    #[test]
    fn osc_cursor_color_query() {
        let ev = one_shot(b"\x1b]12;?\x07");
        assert!(matches!(&ev[0], Response(r) if r.starts_with(b"\x1b]12;rgb:")));
    }

    #[test]
    fn non_query_osc_passes_through() {
        // Window title set must reach the renderer.
        let seq = b"\x1b]0;my title\x07";
        assert_eq!(one_shot(seq), vec![Data(seq.to_vec())]);
    }

    #[test]
    fn split_sequences_at_every_boundary() {
        assert_split_invariant(b"ab\x1b[ccd");
        assert_split_invariant(b"x\x1b[6ny");
        assert_split_invariant(b"\x1b[>0c");
        assert_split_invariant(b"\x1b[5n");
        assert_split_invariant(b"\x1b]11;?\x07");
        assert_split_invariant(b"\x1b]10;?\x1b\\");
        assert_split_invariant(b"a\x1b[12;45Rb");
        assert_split_invariant(b"\x1b[?2004hxyz\x1b[?2004l");
        assert_split_invariant(b"\x1b[?1000h\x1b[31mhi\x1b[0m\x1b[?1000l");
    }

    #[test]
    fn coalesced_queries() {
        assert_eq!(
            one_shot(b"\x1b[c\x1b[6n\x1b[5n"),
            vec![
                Response(DA1_REPLY.to_vec()),
                Cpr,
                Response(DSR_OK_REPLY.to_vec())
            ]
        );
    }

    #[test]
    fn incomplete_sequence_buffers_across_reads() {
        let mut p = OutputParser::new();
        assert_eq!(p.process(b"abc\x1b"), vec![Data(b"abc".to_vec())]);
        assert_eq!(p.process(b"[6"), vec![]);
        assert_eq!(p.process(b"n"), vec![Cpr]);
    }

    #[test]
    fn oversized_incomplete_sequence_flushes_as_data() {
        let mut p = OutputParser::new();
        // An OSC that never terminates, longer than MAX_PENDING.
        let mut junk = b"\x1b]junk;".to_vec();
        junk.extend(std::iter::repeat(b'x').take(2000));
        let ev = p.process(&junk);
        assert_eq!(merged_len(&ev), junk.len());
        assert!(p.pending.is_empty());
    }

    fn merged_len(events: &[RemoteOutputEvent]) -> usize {
        events
            .iter()
            .map(|e| match e {
                Data(d) => d.len(),
                _ => 0,
            })
            .sum()
    }

    #[test]
    fn cpr_response_helper_format() {
        assert_eq!(cpr_response(3, 17), b"\x1b[3;17R");
    }

    #[test]
    fn esc_non_csi_passes_through() {
        // DECSC save-cursor: ESC 7
        assert_eq!(one_shot(b"\x1b7x"), vec![Data(b"\x1b7x".to_vec())]);
    }

    #[test]
    fn utf8_data_unmolested() {
        let text = "héllo wörld — ✓".as_bytes();
        assert_eq!(one_shot(text), vec![Data(text.to_vec())]);
    }
}
