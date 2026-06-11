//! Local input to VT byte-sequence translation for both UI modes.

/// xterm modifier parameter: 1 + Shift(1) + Alt(2) + Ctrl(4).
fn xterm_modifier(shift: bool, alt: bool, ctrl: bool) -> u8 {
    1 + (shift as u8) + ((alt as u8) << 1) + ((ctrl as u8) << 2)
}

/// Arrow/navigation sequence with optional xterm modifiers.
/// `final_byte` is e.g. b'A' for Up. Unmodified: `ESC[A`; modified: `ESC[1;5A`.
pub fn modified_seq(final_byte: u8, shift: bool, alt: bool, ctrl: bool) -> Vec<u8> {
    if !shift && !alt && !ctrl {
        vec![0x1b, b'[', final_byte]
    } else {
        let m = xterm_modifier(shift, alt, ctrl);
        format!("\x1b[1;{m}{}", final_byte as char).into_bytes()
    }
}

/// Tilde-style key (Insert/Delete/PgUp/PgDn/F5..F12) with optional modifiers.
/// Unmodified: `ESC[{n}~`; modified: `ESC[{n};{m}~`.
pub fn tilde_seq(n: u8, shift: bool, alt: bool, ctrl: bool) -> Vec<u8> {
    if !shift && !alt && !ctrl {
        format!("\x1b[{n}~").into_bytes()
    } else {
        let m = xterm_modifier(shift, alt, ctrl);
        format!("\x1b[{n};{m}~").into_bytes()
    }
}

/// Function key sequences F1..F12.
pub fn fkey_seq(n: u8) -> Option<Vec<u8>> {
    match n {
        1 => Some(b"\x1bOP".to_vec()),
        2 => Some(b"\x1bOQ".to_vec()),
        3 => Some(b"\x1bOR".to_vec()),
        4 => Some(b"\x1bOS".to_vec()),
        5 => Some(b"\x1b[15~".to_vec()),
        6 => Some(b"\x1b[17~".to_vec()),
        7 => Some(b"\x1b[18~".to_vec()),
        8 => Some(b"\x1b[19~".to_vec()),
        9 => Some(b"\x1b[20~".to_vec()),
        10 => Some(b"\x1b[21~".to_vec()),
        11 => Some(b"\x1b[23~".to_vec()),
        12 => Some(b"\x1b[24~".to_vec()),
        _ => None,
    }
}

/// Ctrl+letter (and a few symbols) to control byte.
pub fn ctrl_byte(c: char) -> Option<u8> {
    let c = c.to_ascii_lowercase();
    match c {
        'a'..='z' => Some(c as u8 - b'a' + 1),
        '@' | ' ' => Some(0x00),
        '[' => Some(0x1b),
        '\\' => Some(0x1c),
        ']' => Some(0x1d),
        '^' => Some(0x1e),
        '_' | '/' => Some(0x1f),
        _ => None,
    }
}

/// Convert a crossterm key event into VT bytes for the remote PTY.
/// Returns None when the key should not be forwarded.
pub fn key_to_bytes(ev: &crossterm::event::KeyEvent) -> Option<Vec<u8>> {
    use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};

    if ev.kind == KeyEventKind::Release {
        return None;
    }
    let shift = ev.modifiers.contains(KeyModifiers::SHIFT);
    let alt = ev.modifiers.contains(KeyModifiers::ALT);
    let ctrl = ev.modifiers.contains(KeyModifiers::CONTROL);

    let mut out: Vec<u8> = Vec::new();
    let body: Vec<u8> = match ev.code {
        KeyCode::Char(c) => {
            if ctrl {
                match ctrl_byte(c) {
                    Some(b) => vec![b],
                    None => return None,
                }
            } else {
                let mut buf = [0u8; 4];
                c.encode_utf8(&mut buf).as_bytes().to_vec()
            }
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => modified_seq(b'A', shift, false, ctrl),
        KeyCode::Down => modified_seq(b'B', shift, false, ctrl),
        KeyCode::Right => modified_seq(b'C', shift, false, ctrl),
        KeyCode::Left => modified_seq(b'D', shift, false, ctrl),
        KeyCode::Home => modified_seq(b'H', shift, false, ctrl),
        KeyCode::End => modified_seq(b'F', shift, false, ctrl),
        KeyCode::PageUp => tilde_seq(5, shift, false, ctrl),
        KeyCode::PageDown => tilde_seq(6, shift, false, ctrl),
        KeyCode::Insert => tilde_seq(2, shift, false, ctrl),
        KeyCode::Delete => tilde_seq(3, shift, false, ctrl),
        KeyCode::F(n) => fkey_seq(n)?,
        _ => return None,
    };

    // Alt prefixes the key sequence with ESC (except where already encoded
    // into the xterm modifier; for simplicity Alt always prefixes here,
    // matching common terminal behavior for Alt+letter).
    if alt {
        out.push(0x1b);
    }
    out.extend_from_slice(&body);
    Some(out)
}

/// Convert an egui key event into VT bytes. Printable text arrives separately
/// through `egui::Event::Text`, so this handles non-text keys and Ctrl combos.
pub fn egui_key_to_bytes(key: egui::Key, modifiers: egui::Modifiers) -> Option<Vec<u8>> {
    use egui::Key;
    let shift = modifiers.shift;
    let alt = modifiers.alt;
    let ctrl = modifiers.ctrl || modifiers.command;

    let body: Vec<u8> = match key {
        Key::Enter => vec![b'\r'],
        Key::Backspace => vec![0x7f],
        Key::Tab => {
            if shift {
                b"\x1b[Z".to_vec()
            } else {
                vec![b'\t']
            }
        }
        Key::Escape => vec![0x1b],
        Key::ArrowUp => modified_seq(b'A', shift, false, ctrl),
        Key::ArrowDown => modified_seq(b'B', shift, false, ctrl),
        Key::ArrowRight => modified_seq(b'C', shift, false, ctrl),
        Key::ArrowLeft => modified_seq(b'D', shift, false, ctrl),
        Key::Home => modified_seq(b'H', shift, false, ctrl),
        Key::End => modified_seq(b'F', shift, false, ctrl),
        Key::PageUp => tilde_seq(5, shift, false, ctrl),
        Key::PageDown => tilde_seq(6, shift, false, ctrl),
        Key::Insert => tilde_seq(2, shift, false, ctrl),
        Key::Delete => tilde_seq(3, shift, false, ctrl),
        Key::F1 => fkey_seq(1)?,
        Key::F2 => fkey_seq(2)?,
        Key::F3 => fkey_seq(3)?,
        Key::F4 => fkey_seq(4)?,
        Key::F5 => fkey_seq(5)?,
        Key::F6 => fkey_seq(6)?,
        Key::F7 => fkey_seq(7)?,
        Key::F8 => fkey_seq(8)?,
        Key::F9 => fkey_seq(9)?,
        Key::F10 => fkey_seq(10)?,
        Key::F11 => fkey_seq(11)?,
        Key::F12 => fkey_seq(12)?,
        _ if ctrl => {
            // Ctrl+letter combos: egui reports the Key, not a text event.
            let name = key.name();
            let mut chars = name.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            vec![ctrl_byte(c)?]
        }
        _ => return None,
    };

    let mut out = Vec::new();
    if alt {
        out.push(0x1b);
    }
    out.extend_from_slice(&body);
    Some(out)
}

/// Build a paste payload. Newlines are normalized to `\r`; when the remote
/// has enabled bracketed paste the payload is wrapped in `ESC[200~ .. ESC[201~`.
pub fn paste_payload(text: &str, bracketed: bool) -> Vec<u8> {
    let normalized = text.replace("\r\n", "\r").replace('\n', "\r");
    if bracketed {
        let mut out = b"\x1b[200~".to_vec();
        out.extend_from_slice(normalized.as_bytes());
        out.extend_from_slice(b"\x1b[201~");
        out
    } else {
        normalized.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn printable_char() {
        assert_eq!(
            key_to_bytes(&key(KeyCode::Char('a'), KeyModifiers::NONE)).unwrap(),
            b"a"
        );
        assert_eq!(
            key_to_bytes(&key(KeyCode::Char('é'), KeyModifiers::NONE)).unwrap(),
            "é".as_bytes()
        );
    }

    #[test]
    fn ctrl_letters() {
        assert_eq!(
            key_to_bytes(&key(KeyCode::Char('c'), KeyModifiers::CONTROL)).unwrap(),
            vec![0x03]
        );
        assert_eq!(
            key_to_bytes(&key(KeyCode::Char('d'), KeyModifiers::CONTROL)).unwrap(),
            vec![0x04]
        );
        assert_eq!(
            key_to_bytes(&key(KeyCode::Char('z'), KeyModifiers::CONTROL)).unwrap(),
            vec![0x1a]
        );
    }

    #[test]
    fn alt_prefix() {
        assert_eq!(
            key_to_bytes(&key(KeyCode::Char('b'), KeyModifiers::ALT)).unwrap(),
            vec![0x1b, b'b']
        );
    }

    #[test]
    fn shift_tab() {
        assert_eq!(
            key_to_bytes(&key(KeyCode::BackTab, KeyModifiers::SHIFT)).unwrap(),
            b"\x1b[Z"
        );
    }

    #[test]
    fn plain_arrows() {
        assert_eq!(
            key_to_bytes(&key(KeyCode::Up, KeyModifiers::NONE)).unwrap(),
            b"\x1b[A"
        );
        assert_eq!(
            key_to_bytes(&key(KeyCode::Left, KeyModifiers::NONE)).unwrap(),
            b"\x1b[D"
        );
    }

    #[test]
    fn modified_arrows() {
        assert_eq!(
            key_to_bytes(&key(KeyCode::Up, KeyModifiers::CONTROL)).unwrap(),
            b"\x1b[1;5A"
        );
        assert_eq!(
            key_to_bytes(&key(KeyCode::Right, KeyModifiers::SHIFT)).unwrap(),
            b"\x1b[1;2C"
        );
        assert_eq!(
            key_to_bytes(&key(
                KeyCode::Down,
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            ))
            .unwrap(),
            b"\x1b[1;6B"
        );
    }

    #[test]
    fn function_keys() {
        assert_eq!(
            key_to_bytes(&key(KeyCode::F(1), KeyModifiers::NONE)).unwrap(),
            b"\x1bOP"
        );
        assert_eq!(
            key_to_bytes(&key(KeyCode::F(5), KeyModifiers::NONE)).unwrap(),
            b"\x1b[15~"
        );
        assert_eq!(
            key_to_bytes(&key(KeyCode::F(12), KeyModifiers::NONE)).unwrap(),
            b"\x1b[24~"
        );
    }

    #[test]
    fn navigation_keys() {
        assert_eq!(
            key_to_bytes(&key(KeyCode::Home, KeyModifiers::NONE)).unwrap(),
            b"\x1b[H"
        );
        assert_eq!(
            key_to_bytes(&key(KeyCode::End, KeyModifiers::NONE)).unwrap(),
            b"\x1b[F"
        );
        assert_eq!(
            key_to_bytes(&key(KeyCode::PageUp, KeyModifiers::NONE)).unwrap(),
            b"\x1b[5~"
        );
        assert_eq!(
            key_to_bytes(&key(KeyCode::Delete, KeyModifiers::NONE)).unwrap(),
            b"\x1b[3~"
        );
        assert_eq!(
            key_to_bytes(&key(KeyCode::Insert, KeyModifiers::NONE)).unwrap(),
            b"\x1b[2~"
        );
    }

    #[test]
    fn paste_plain_normalizes_newlines() {
        assert_eq!(paste_payload("a\r\nb\nc", false), b"a\rb\rc");
    }

    #[test]
    fn paste_bracketed_wraps() {
        assert_eq!(
            paste_payload("hi\nthere", true),
            b"\x1b[200~hi\rthere\x1b[201~"
        );
    }
}
