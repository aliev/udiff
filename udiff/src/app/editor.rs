pub(super) fn previous_boundary(text: &str, cursor: usize) -> Option<usize> {
    text[..cursor]
        .char_indices()
        .next_back()
        .map(|(index, _)| index)
}

pub(super) fn next_boundary(text: &str, cursor: usize) -> Option<usize> {
    text[cursor..]
        .chars()
        .next()
        .map(|character| cursor + character.len_utf8())
}

/// Byte offsets of the logical line the cursor sits on, its break excluded.
/// Logical, not visual: `vertical_cursor` already moves by `\n` rather than by
/// wrapped rows, and the two would disagree otherwise.
pub(super) fn line_bounds(text: &str, cursor: usize) -> (usize, usize) {
    let start = text[..cursor].rfind('\n').map_or(0, |index| index + 1);
    let end = text[cursor..]
        .find('\n')
        .map_or(text.len(), |offset| cursor + offset);
    (start, end)
}

pub(super) fn vertical_cursor(text: &str, cursor: usize, down: bool) -> usize {
    let line_start = text[..cursor].rfind('\n').map_or(0, |index| index + 1);
    let column = text[line_start..cursor].chars().count();
    let (target_start, target_end) = if down {
        let Some(current_end_offset) = text[cursor..].find('\n') else {
            return cursor;
        };
        let target_start = cursor + current_end_offset + 1;
        let target_end = text[target_start..]
            .find('\n')
            .map_or(text.len(), |offset| target_start + offset);
        (target_start, target_end)
    } else {
        if line_start == 0 {
            return cursor;
        }
        let target_end = line_start - 1;
        let target_start = text[..target_end].rfind('\n').map_or(0, |index| index + 1);
        (target_start, target_end)
    };
    text[target_start..target_end]
        .char_indices()
        .nth(column)
        .map_or(target_end, |(offset, _)| target_start + offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_ends_at_its_break_not_at_the_end_of_the_text() {
        // "alpha\nβeta\ngamma": β is two bytes, so the middle line is 6..11.
        let text = "alpha\nβeta\ngamma";
        assert_eq!(line_bounds(text, 8), (6, 11));
        assert_eq!(line_bounds(text, 0), (0, 5));
        assert_eq!(line_bounds(text, 12), (12, 17));
    }

    #[test]
    fn an_empty_line_starts_and_ends_in_the_same_place() {
        assert_eq!(line_bounds("a\n\nb", 2), (2, 2));
    }

    #[test]
    fn cursor_operations_stay_on_utf8_boundaries() {
        assert_eq!(next_boundary("aλb", 1), Some(3));
        assert_eq!(previous_boundary("aλb", 3), Some(1));
        assert_eq!(vertical_cursor("aλ\nxyz", 3, true), 6);
    }
}
