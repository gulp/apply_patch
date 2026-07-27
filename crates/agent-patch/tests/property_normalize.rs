//! Property tests for pure normalize helpers.

use agent_patch::match_opts::{normalize_line, FuzzyMode};
use proptest::prelude::*;

proptest! {
    #[test]
    fn rstrip_is_idempotent(s in ".*") {
        let once = normalize_line(&s, FuzzyMode::Rstrip);
        let twice = normalize_line(&once, FuzzyMode::Rstrip);
        prop_assert_eq!(once, twice);
    }

    #[test]
    fn strip_is_idempotent(s in ".*") {
        let once = normalize_line(&s, FuzzyMode::Strip);
        let twice = normalize_line(&once, FuzzyMode::Strip);
        prop_assert_eq!(once, twice);
    }

    #[test]
    fn off_preserves_bytes(s in ".*") {
        prop_assert_eq!(normalize_line(&s, FuzzyMode::Off), s);
    }

    #[test]
    fn strip_subset_of_rstrip_length(s in ".*") {
        let r = normalize_line(&s, FuzzyMode::Rstrip);
        let t = normalize_line(&s, FuzzyMode::Strip);
        prop_assert!(t.len() <= r.len());
    }
}
