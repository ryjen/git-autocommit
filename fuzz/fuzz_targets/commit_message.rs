#![no_main]

use git_autocommit::validation::{MAX_COMMIT_MESSAGE_BYTES, validate_conventional_message};
use libfuzzer_sys::fuzz_target;

const FUZZ_MESSAGE_BYTES: usize = MAX_COMMIT_MESSAGE_BYTES * 2;

fuzz_target!(|data: &[u8]| {
    if data.len() > FUZZ_MESSAGE_BYTES {
        return;
    }
    let Ok(message) = std::str::from_utf8(data) else {
        return;
    };

    let first = validate_conventional_message(message);
    let second = validate_conventional_message(message);
    assert_eq!(first, second);
});
