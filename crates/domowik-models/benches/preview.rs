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
    let grid = usd_grid(100);
    c.bench_function("model mesh / USDC 100x100 quad grid / 20k triangles", |b| {
        b.iter(|| domowik_models::mesh(black_box(&grid)).unwrap())
    });
}

/// An original n x n quad grid as a binary USD crate layer.
fn usd_grid(n: usize) -> Vec<u8> {
    let mut points = Vec::new();
    for z in 0..=n {
        for x in 0..=n {
            points.push(format!("({x}, {}, {z})", (x + z) % 3));
        }
    }
    let mut indices = Vec::new();
    for z in 0..n {
        for x in 0..n {
            let i = z * (n + 1) + x;
            indices.extend([i, i + 1, i + n + 2, i + n + 1].map(|v| v.to_string()));
        }
    }
    let text = format!(
        "#usda 1.0\n(\nmetersPerUnit = 0.01\n)\ndef Mesh \"Grid\" {{\n\
         int[] faceVertexCounts = [{}]\nint[] faceVertexIndices = [{}]\npoint3f[] points = [{}]\n}}\n",
        vec!["4"; n * n].join(", "),
        indices.join(", "),
        points.join(", "),
    );
    let layer = openusd::sdf::Layer::from_bytes("grid.usda", text.into_bytes()).unwrap();
    let mut out = std::io::Cursor::new(Vec::new());
    openusd::usdc::CrateWriter::write(layer.data(), &mut out).unwrap();
    out.into_inner()
}
criterion_group!(benches, benchmark);
criterion_main!(benches);
