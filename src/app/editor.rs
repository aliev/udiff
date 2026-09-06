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
    fn cursor_operations_stay_on_utf8_boundaries() {
        assert_eq!(next_boundary("aλb", 1), Some(3));
        assert_eq!(previous_boundary("aλb", 3), Some(1));
        assert_eq!(vertical_cursor("aλ\nxyz", 3, true), 6);
    }
}
