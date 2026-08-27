pub(super) fn fuzzy(query: &str, candidate: &str) -> bool {
    let mut chars = query.to_lowercase().chars().collect::<Vec<_>>().into_iter();
    let mut want = chars.next();
    for character in candidate.to_lowercase().chars() {
        if want == Some(character) {
            want = chars.next();
            if want.is_none() {
                return true;
            }
        }
    }
    want.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_search_is_case_insensitive_and_ordered() {
        assert!(fuzzy("SMM", "src/MainModel.rs"));
        assert!(!fuzzy("MSZ", "src/MainModel.rs"));
    }
}
