use unicode_width::UnicodeWidthChar;

/// Truncate a string so its terminal display width is at most `max_cells`,
/// appending `…` if truncation occurred. Operates on grapheme widths via the
/// `unicode-width` crate so multi-byte characters and wide CJK glyphs are
/// counted correctly.
pub fn truncate_to_width(s: &str, max_cells: usize) -> String {
    let total: usize = s.chars().map(|c| c.width().unwrap_or(0)).sum();
    if total <= max_cells {
        return s.to_string();
    }
    if max_cells == 0 {
        return String::new();
    }
    if max_cells == 1 {
        return "…".into();
    }
    let budget = max_cells - 1; // reserve a cell for the ellipsis
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > budget {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_truncation_when_short() {
        assert_eq!(truncate_to_width("abc", 10), "abc");
    }

    #[test]
    fn truncates_with_ellipsis() {
        assert_eq!(truncate_to_width("abcdef", 4), "abc…");
    }

    #[test]
    fn handles_multibyte_chars() {
        // "café" has 4 display cells; truncating to 3 → "ca…" (2 + ellipsis)
        assert_eq!(truncate_to_width("café", 3), "ca…");
    }

    #[test]
    fn zero_width_safe() {
        assert_eq!(truncate_to_width("xxx", 0), "");
        assert_eq!(truncate_to_width("xxx", 1), "…");
    }
}
