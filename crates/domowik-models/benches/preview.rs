use criterion::{black_box, criterion_group, criterion_main, Criterion};
fn benchmark(c: &mut Criterion) {
    let mesh = b"v 0 1 0\nv 2 1 0\nv 2 1 1\nv 0 1 1\nf 1 2 3 4\n";
    c.bench_function("model preview / original quad / PNG", |b| {
        b.iter(|| domowik_models::preview(black_box(mesh)).unwrap())
    });
    c.bench_function("model dimensions / original quad / bounded decode", |b| {
        b.iter(|| domowik_models::dimensions(black_box(mesh)).unwrap())
    });
}
criterion_group!(benches, benchmark);
criterion_main!(benches);
