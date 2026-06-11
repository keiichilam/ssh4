//! Bracket matching over the visible screen grid (FR-015).
//!
//! Pure text logic: given the screen rows and a cursor cell, find the
//! matching bracket in linear reading order. The renderer paints both cells.

const PAIRS: [(char, char); 4] = [('(', ')'), ('[', ']'), ('{', '}'), ('<', '>')];

fn open_for(c: char) -> Option<char> {
    PAIRS.iter().find(|(_, close)| *close == c).map(|(o, _)| *o)
}

fn close_for(c: char) -> Option<char> {
    PAIRS.iter().find(|(open, _)| *open == c).map(|(_, c)| *c)
}

/// Find the cell matching the bracket at `(row, col)`, scanning the whole
/// grid in reading order. Returns `None` when the cell is not a bracket or
/// the match is off-screen/unbalanced.
pub fn match_bracket(rows: &[Vec<char>], row: usize, col: usize) -> Option<(usize, usize)> {
    let at = *rows.get(row)?.get(col)?;

    if let Some(close) = close_for(at) {
        // Scan forward for the matching close.
        let mut depth = 0usize;
        let mut iter = iter_forward(rows, row, col);
        iter.next(); // skip the bracket itself
        for (r, c, ch) in iter {
            if ch == at {
                depth += 1;
            } else if ch == close {
                if depth == 0 {
                    return Some((r, c));
                }
                depth -= 1;
            }
        }
        None
    } else if let Some(open) = open_for(at) {
        // Scan backward for the matching open.
        let mut depth = 0usize;
        let mut iter = iter_backward(rows, row, col);
        iter.next(); // skip the bracket itself
        for (r, c, ch) in iter {
            if ch == at {
                depth += 1;
            } else if ch == open {
                if depth == 0 {
                    return Some((r, c));
                }
                depth -= 1;
            }
        }
        None
    } else {
        None
    }
}

fn iter_forward(
    rows: &[Vec<char>],
    row: usize,
    col: usize,
) -> impl Iterator<Item = (usize, usize, char)> + '_ {
    rows.iter()
        .enumerate()
        .skip(row)
        .flat_map(move |(r, line)| {
            let start = if r == row { col } else { 0 };
            line.iter()
                .enumerate()
                .skip(start)
                .map(move |(c, ch)| (r, c, *ch))
        })
}

fn iter_backward(
    rows: &[Vec<char>],
    row: usize,
    col: usize,
) -> impl Iterator<Item = (usize, usize, char)> + '_ {
    rows.iter()
        .enumerate()
        .take(row + 1)
        .rev()
        .flat_map(move |(r, line)| {
            let end = if r == row { col + 1 } else { line.len() };
            line.iter()
                .enumerate()
                .take(end)
                .rev()
                .map(move |(c, ch)| (r, c, *ch))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(lines: &[&str]) -> Vec<Vec<char>> {
        lines.iter().map(|l| l.chars().collect()).collect()
    }

    #[test]
    fn matches_simple_pair_same_row() {
        let g = grid(&["fn main() {}"]);
        assert_eq!(match_bracket(&g, 0, 7), Some((0, 8)));
        assert_eq!(match_bracket(&g, 0, 8), Some((0, 7)));
    }

    #[test]
    fn matches_nested_pairs() {
        let g = grid(&["((a) (b))"]);
        assert_eq!(match_bracket(&g, 0, 0), Some((0, 8)));
        assert_eq!(match_bracket(&g, 0, 1), Some((0, 3)));
        assert_eq!(match_bracket(&g, 0, 5), Some((0, 7)));
    }

    #[test]
    fn matches_across_rows() {
        let g = grid(&["if x {", "  y();", "}"]);
        assert_eq!(match_bracket(&g, 0, 5), Some((2, 0)));
        assert_eq!(match_bracket(&g, 2, 0), Some((0, 5)));
    }

    #[test]
    fn non_bracket_and_unbalanced_return_none() {
        let g = grid(&["abc (def"]);
        assert_eq!(match_bracket(&g, 0, 0), None);
        assert_eq!(match_bracket(&g, 0, 4), None);
        assert_eq!(match_bracket(&g, 9, 0), None);
    }

    #[test]
    fn angle_brackets_match() {
        let g = grid(&["Vec<Option<u8>>"]);
        assert_eq!(match_bracket(&g, 0, 3), Some((0, 14)));
        assert_eq!(match_bracket(&g, 0, 10), Some((0, 13)));
    }
}
