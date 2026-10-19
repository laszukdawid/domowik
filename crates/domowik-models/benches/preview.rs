use criterion::{black_box, criterion_group, criterion_main, Criterion};
#[path = "../tests/support/mod.rs"]
mod support;
fn benchmark(c: &mut Criterion) {
    let (j, bin) = support::fixture(image::ImageFormat::WebP, true);
    let textured = support::pack(&j, &bin);
    c.bench_function("model texture / Draco WebP preview", |b| {
        b.iter(|| domowik_models::preview(black_box(&textured)).unwrap())
    });
    c.bench_function("library thumbnail / Draco WebP / angled PNG", |b| {
        b.iter(|| domowik_models::thumbnail(black_box(&textured)).unwrap())
    });
    c.bench_function("model texture / Draco WebP GPU mesh", |b| {
        b.iter(|| domowik_models::mesh(black_box(&textured)).unwrap())
    });
    let mesh = b"v 0 1 0\nv 2 1 0\nv 2 1 1\nv 0 1 1\nf 1 2 3 4\n";
    c.bench_function("model preview / original quad / PNG", |b| {
        b.iter(|| domowik_models::preview(black_box(mesh)).unwrap())
    });
    c.bench_function("library thumbnail / original quad / angled PNG", |b| {
        b.iter(|| domowik_models::thumbnail(black_box(mesh)).unwrap())
    });
    c.bench_function("model dimensions / original quad / bounded decode", |b| {
        b.iter(|| domowik_models::dimensions(black_box(mesh)).unwrap())
    });
}
criterion_group!(benches, benchmark);
criterion_main!(benches);
