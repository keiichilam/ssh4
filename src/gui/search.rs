//! Terminal find bar: search over visible screen plus scrollback. Match IDs
//! are (distance-from-bottom row, col) so navigation survives scroll changes.

use crate::gui::session::Session;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    /// Rows above the live bottom screen (0 = visible screen at bottom).
    pub scrollback: usize,
    /// Row within that screen view.
    pub row: u16,
    pub col: u16,
    pub len: u16,
}

#[derive(Default)]
pub struct SearchState {
    pub open: bool,
    pub query: String,
    pub case_sensitive: bool,
    pub matches: Vec<Match>,
    pub active: usize,
    /// Session generation the cache was built against.
    cached_generation: Option<(u64, String, bool)>,
}

impl SearchState {
    pub fn invalidate(&mut self) {
        self.cached_generation = None;
    }

    /// Rebuild the match cache if the query or terminal contents changed.
    /// Walks scrollback by temporarily adjusting the parser offset.
    pub fn refresh(&mut self, session: &mut Session) {
        let key = (session.generation, self.query.clone(), self.case_sensitive);
        if self.cached_generation.as_ref() == Some(&key) {
            return;
        }
        self.cached_generation = Some(key);
        self.matches.clear();
        self.active = 0;
        if self.query.is_empty() {
            return;
        }

        let needle = if self.case_sensitive {
            self.query.clone()
        } else {
            self.query.to_lowercase()
        };
        let rows = session.rows;
        let saved = session.scrollback;

        // Visit the live screen and each scrollback step. Scanning row by
        // row at offsets that are multiples of the screen height covers all
        // lines exactly once.
        let mut offset = 0usize;
        loop {
            session.parser.set_scrollback(offset);
            let screen = session.parser.screen();
            // vt100 clamps the offset; detect the top by comparing.
            for row in 0..rows {
                let text = screen.contents_between(row, 0, row + 1, 0);
                let hay = if self.case_sensitive {
                    text.clone()
                } else {
                    text.to_lowercase()
                };
                let mut from = 0;
                while let Some(pos) = hay[from..].find(&needle) {
                    let col = hay[..from + pos].chars().count() as u16;
                    self.matches.push(Match {
                        scrollback: offset,
                        row,
                        col,
                        len: needle.chars().count() as u16,
                    });
                    from += pos + needle.len().max(1);
                }
            }
            let next = offset + rows as usize;
            session.parser.set_scrollback(next);
            // set_scrollback clamps at the top; stop when it no longer moves.
            if scrollback_pos(&session.parser) == offset
                || next > crate::gui::session::SCROLLBACK_LINES
            {
                break;
            }
            offset = scrollback_pos(&session.parser);
        }
        session.parser.set_scrollback(saved);

        // Sort top-to-bottom (largest scrollback first), then row/col.
        self.matches.sort_by(|a, b| {
            b.scrollback
                .cmp(&a.scrollback)
                .then(a.row.cmp(&b.row))
                .then(a.col.cmp(&b.col))
        });
        // Start at the last (most recent) match.
        if !self.matches.is_empty() {
            self.active = self.matches.len() - 1;
        }
    }

    pub fn next(&mut self) {
        if !self.matches.is_empty() {
            self.active = (self.active + 1) % self.matches.len();
        }
    }

    pub fn prev(&mut self) {
        if !self.matches.is_empty() {
            self.active = (self.active + self.matches.len() - 1) % self.matches.len();
        }
    }

    /// Scroll the session so the active match is visible.
    pub fn reveal_active(&self, session: &mut Session) {
        if let Some(m) = self.matches.get(self.active) {
            session.scrollback = m.scrollback;
            session.parser.set_scrollback(m.scrollback);
        }
    }
}

/// Current scrollback offset of a parser. vt100 0.15 exposes it on Screen.
fn scrollback_pos(parser: &vt100::Parser) -> usize {
    parser.screen().scrollback()
}
