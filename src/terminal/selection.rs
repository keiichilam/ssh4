//! Terminal grid selection model, shared by CLI and GUI. Rows/cols are
//! 0-based grid coordinates; the model is independent of any renderer.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellPos {
    pub row: i32,
    pub col: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    pub anchor: Option<CellPos>,
    pub head: Option<CellPos>,
}

impl Selection {
    pub fn start(&mut self, pos: CellPos) {
        self.anchor = Some(pos);
        self.head = Some(pos);
    }

    pub fn drag(&mut self, pos: CellPos) {
        if self.anchor.is_some() {
            self.head = Some(pos);
        }
    }

    pub fn clear(&mut self) {
        self.anchor = None;
        self.head = None;
    }

    pub fn is_active(&self) -> bool {
        matches!((self.anchor, self.head), (Some(a), Some(h)) if a != h)
    }

    /// Normalized (start, end) in reading order, inclusive.
    pub fn normalized(&self) -> Option<(CellPos, CellPos)> {
        let (a, h) = (self.anchor?, self.head?);
        if a == h {
            return None;
        }
        if (a.row, a.col) <= (h.row, h.col) {
            Some((a, h))
        } else {
            Some((h, a))
        }
    }

    /// Whether a cell falls inside the selection (linear reading order).
    pub fn contains(&self, row: i32, col: i32) -> bool {
        match self.normalized() {
            Some((s, e)) => (s.row, s.col) <= (row, col) && (row, col) <= (e.row, e.col),
            None => false,
        }
    }

    /// Extract selected text given a row-text lookup. Trailing whitespace per
    /// row is trimmed; rows are joined with `\n`.
    pub fn extract_text(&self, row_text: impl Fn(i32) -> String) -> String {
        let Some((s, e)) = self.normalized() else {
            return String::new();
        };
        let mut lines = Vec::new();
        for row in s.row..=e.row {
            let text = row_text(row);
            let chars: Vec<char> = text.chars().collect();
            let from = if row == s.row {
                s.col.max(0) as usize
            } else {
                0
            };
            let to = if row == e.row {
                ((e.col + 1).max(0) as usize).min(chars.len())
            } else {
                chars.len()
            };
            let slice: String = if from < to {
                chars[from..to].iter().collect()
            } else {
                String::new()
            };
            lines.push(slice.trim_end().to_string());
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(row: i32, col: i32) -> CellPos {
        CellPos { row, col }
    }

    #[test]
    fn empty_selection_inactive() {
        let s = Selection::default();
        assert!(!s.is_active());
        assert_eq!(s.normalized(), None);
    }

    #[test]
    fn zero_width_selection_inactive() {
        let mut s = Selection::default();
        s.start(pos(1, 1));
        assert!(!s.is_active());
    }

    #[test]
    fn normalizes_backwards_drag() {
        let mut s = Selection::default();
        s.start(pos(5, 10));
        s.drag(pos(2, 3));
        let (a, b) = s.normalized().unwrap();
        assert_eq!(a, pos(2, 3));
        assert_eq!(b, pos(5, 10));
    }

    #[test]
    fn contains_linear_order() {
        let mut s = Selection::default();
        s.start(pos(1, 5));
        s.drag(pos(2, 2));
        assert!(s.contains(1, 5));
        assert!(s.contains(1, 70)); // rest of first row
        assert!(s.contains(2, 0));
        assert!(s.contains(2, 2));
        assert!(!s.contains(2, 3));
        assert!(!s.contains(0, 9));
    }

    #[test]
    fn extract_single_row() {
        let mut s = Selection::default();
        s.start(pos(0, 2));
        s.drag(pos(0, 4));
        let text = s.extract_text(|_| "hello world".to_string());
        assert_eq!(text, "llo");
    }

    #[test]
    fn extract_multi_row_trims_trailing() {
        let mut s = Selection::default();
        s.start(pos(0, 6));
        s.drag(pos(1, 2));
        let rows = ["first   ", "second"];
        let text = s.extract_text(|r| rows[r as usize].to_string());
        assert_eq!(text, "\nsec");
    }
}
