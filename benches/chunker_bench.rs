//! Chunker benchmarks

use criterion::{criterion_group, criterion_main, Criterion};

fn chunker_benchmark(_c: &mut Criterion) {
    // Benchmarks will be added when chunker is implemented
}

criterion_group!(benches, chunker_benchmark);
criterion_main!(benches);
