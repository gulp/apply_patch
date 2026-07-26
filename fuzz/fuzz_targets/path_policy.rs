#![no_main]
use agent_patch::path_policy::parse_repo_path;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 4 * 1024 {
        return;
    }
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = parse_repo_path(s);
    }
});
