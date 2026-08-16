#![no_main]

use git_autocommit::validation::{validate_conventional_message, validate_requested_plan};
use libfuzzer_sys::fuzz_target;

const MAX_MODEL_TEXT_BYTES: usize = 256 * 1024;
const MAX_COMMITS: usize = 8;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_MODEL_TEXT_BYTES {
        return;
    }
    let Ok(raw) = std::str::from_utf8(data) else {
        return;
    };
    let staged = vec!["a".to_owned(), "dir/b".to_owned()];

    let first = validate_requested_plan(raw, &staged, MAX_COMMITS, false);
    let second = validate_requested_plan(raw, &staged, MAX_COMMITS, false);
    assert_eq!(first.is_ok(), second.is_ok());

    if let Ok(plan) = first {
        assert!(!plan.is_empty());
        assert!(plan.len() <= MAX_COMMITS);

        let mut files: Vec<&str> = plan
            .iter()
            .flat_map(|entry| entry.files.iter().map(String::as_str))
            .collect();
        files.sort_unstable();
        assert_eq!(files, ["a", "dir/b"]);

        for entry in &plan {
            assert!(!entry.files.is_empty());
            assert!(validate_conventional_message(entry.message.trim()).is_ok());
        }

        let single = validate_requested_plan(raw, &staged, MAX_COMMITS, true);
        assert_eq!(single.is_ok(), plan.len() == 1);
    }
});
