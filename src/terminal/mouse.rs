//! SGR mouse protocol encoding (mode 1006). Coordinates are 1-based.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

impl MouseButton {
    fn code(self) -> u8 {
        match self {
            MouseButton::Left => 0,
            MouseButton::Middle => 1,
            MouseButton::Right => 2,
        }
    }
}

fn sgr(btn: u16, col: u16, row: u16, press: bool) -> Vec<u8> {
    let suffix = if press { 'M' } else { 'm' };
    format!("\x1b[<{btn};{col};{row}{suffix}").into_bytes()
}

/// Button press/release. `press = false` emits the release form (`m`).
pub fn sgr_button(button: MouseButton, col: u16, row: u16, press: bool) -> Vec<u8> {
    sgr(button.code() as u16, col.max(1), row.max(1), press)
}

/// Drag motion with a button held (motion flag 32).
pub fn sgr_drag(button: MouseButton, col: u16, row: u16) -> Vec<u8> {
    sgr(button.code() as u16 + 32, col.max(1), row.max(1), true)
}

/// Motion with no button held (only relevant in any-event mode 1003).
#[allow(dead_code)]
pub fn sgr_move(col: u16, row: u16) -> Vec<u8> {
    sgr(35, col.max(1), row.max(1), true)
}

/// Scroll wheel: up = 64, down = 65. Wheel events are always "presses".
pub fn sgr_scroll(up: bool, col: u16, row: u16) -> Vec<u8> {
    sgr(if up { 64 } else { 65 }, col.max(1), row.max(1), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn press_release() {
        assert_eq!(sgr_button(MouseButton::Left, 1, 1, true), b"\x1b[<0;1;1M");
        assert_eq!(sgr_button(MouseButton::Left, 1, 1, false), b"\x1b[<0;1;1m");
        assert_eq!(
            sgr_button(MouseButton::Right, 10, 5, true),
            b"\x1b[<2;10;5M"
        );
    }

    #[test]
    fn drag_adds_motion_flag() {
        assert_eq!(sgr_drag(MouseButton::Left, 3, 4), b"\x1b[<32;3;4M");
        assert_eq!(sgr_drag(MouseButton::Middle, 3, 4), b"\x1b[<33;3;4M");
    }

    #[test]
    fn scroll_codes() {
        assert_eq!(sgr_scroll(true, 2, 2), b"\x1b[<64;2;2M");
        assert_eq!(sgr_scroll(false, 2, 2), b"\x1b[<65;2;2M");
    }

    #[test]
    fn coordinates_clamped_to_one() {
        assert_eq!(sgr_button(MouseButton::Left, 0, 0, true), b"\x1b[<0;1;1M");
    }
}
