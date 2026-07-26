#![no_main]
use agent_patch::engine::apply_update;
use agent_patch::protocol::ast::FileOperation;
use agent_patch::protocol::parse_patch;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 8 || data.len() > 32 * 1024 {
        return;
    }
    let split = (data[0] as usize % (data.len().saturating_sub(1)).max(1)) + 1;
    let (base_bytes, patch_bytes) = data.split_at(split.min(data.len()));
    let Ok(base) = std::str::from_utf8(base_bytes) else {
        return;
    };
    let Ok(patch) = std::str::from_utf8(patch_bytes) else {
        return;
    };
    let Ok(doc) = parse_patch(patch) else {
        return;
    };
    for op in &doc.operations {
        if let FileOperation::Update(u) = op {
            let _ = apply_update(base, u, "\n", base.ends_with('\n'));
        }
    }
});
