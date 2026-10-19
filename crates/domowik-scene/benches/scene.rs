use criterion::{black_box, criterion_group, criterion_main, Criterion};
fn bench(c: &mut Criterion) {
    c.bench_function("3D asset pose / projection and matrices", |b| {
        b.iter(|| {
            let angles = black_box([100., 10., 30.]);
            (
                domowik_scene::asset_pose::matrices([0.; 3], [1.2, 0.05, 0.7], angles, 1.),
                domowik_scene::asset_pose::projected_dimensions(
                    [1200., 700., 50.],
                    [angles[0], angles[1]],
                ),
            )
        })
    });
    let mut doc = domowik_format::Document::new();
    doc.levels.push(domowik_format::Level::new("Ground"));
    c.bench_function("3D scene / empty baseline", |b| {
        b.iter(|| {
            domowik_scene::build(
                black_box(&doc),
                &[],
                &Default::default(),
                &Default::default(),
            )
            .unwrap()
        })
    });
    let level = doc.levels[0].id;
    let digest = format!("sha256:{}", "0".repeat(64));
    for i in 0..1000 {
        doc.asset_instances.push(domowik_format::AssetInstance {
            id: uuid::Uuid::now_v7(),
            level_id: level,
            digest: digest.clone(),
            transform: domowik_format::InstanceTransform {
                translation_mm: [(i % 50) as f64 * 1000., (i / 50) as f64 * 1000.],
                ..Default::default()
            },
            display_name: None,
            unknown: Default::default(),
        });
    }
    let models = std::collections::BTreeMap::from([(digest, 1.5)]);
    c.bench_function("3D scene / 1000 shared furniture instances", |b| {
        b.iter(|| {
            domowik_scene::build(black_box(&doc), &[level], &Default::default(), &models).unwrap()
        })
    });
}
criterion_group!(benches, bench);
criterion_main!(benches);
