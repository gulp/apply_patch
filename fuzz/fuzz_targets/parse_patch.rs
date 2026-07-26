#![no_main]
use agent_patch::protocol::parse_patch;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 64 * 1024 {
        return;
    }
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = parse_patch(s);
    }
});
