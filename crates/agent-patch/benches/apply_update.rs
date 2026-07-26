//! Criterion benches for locate→emit apply (observational; not a CI gate).

use agent_patch::engine::apply_update;
use agent_patch::error::SourceSpan;
use agent_patch::protocol::ast::{Hunk, HunkLine, UpdateFile};
use criterion::{criterion_group, criterion_main, Criterion};

fn sample_update(n_lines: usize) -> (String, UpdateFile) {
    let mut base = String::new();
    for i in 0..n_lines {
        base.push_str(&format!("line{i}\n"));
    }
    let mid = n_lines / 2;
    let update = UpdateFile {
        path: "bench.txt".into(),
        source_span: SourceSpan { line: 1, column: 1 },
        hunks: vec![Hunk {
            lines: vec![
                HunkLine::Context(format!("line{}", mid - 1)),
                HunkLine::Delete(format!("line{mid}")),
                HunkLine::Add(format!("line{mid}-edited")),
                HunkLine::Context(format!("line{}", mid + 1)),
            ],
            source_span: SourceSpan { line: 2, column: 1 },
            anchor: None,
            end_of_file: false,
        }],
    };
    (base, update)
}

fn bench_apply(c: &mut Criterion) {
    let (base, update) = sample_update(2_000);
    c.bench_function("apply_update_2k_lines", |b| {
        b.iter(|| apply_update(&base, &update, "\n", true).unwrap())
    });
}

criterion_group!(benches, bench_apply);
criterion_main!(benches);
