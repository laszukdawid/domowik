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
    // Picking: all 1000 nodes share one 5000-triangle model inside the unit
    // box. A low diagonal ray crosses the 50 × 20 grid, so many boxes and some
    // full meshes are tested; a ray into the sky is rejected by boxes alone.
    let scene = domowik_scene::build(&doc, &[level], &Default::default(), &models).unwrap();
    let model: Vec<f32> = (0..5000 * 3)
        .flat_map(|i| {
            let t = i as f32 * 0.618_034;
            let p = [
                t.fract() - 0.5,
                (t * 0.37).fract() * 1.5,
                (t * 0.71).fract() - 0.5,
            ];
            [p[0], p[1], p[2], 0., 1., 0., 1., 1., 1., 1.]
        })
        .collect();
    let view = domowik_scene::pick::MeshView {
        vertices: &model,
        bounds: domowik_scene::pick::mesh_bounds(&model).unwrap(),
    };
    for (name, origin, direction) in [
        ("ray across", [-30., 0.5, 20.], [1., -0.01, -0.7]),
        ("ray into the sky over", [0., 5., 0.], [0.2, 1., -0.3]),
    ] {
        c.bench_function(&format!("3D pick / {name} 1000 5k-triangle nodes"), |b| {
            b.iter(|| {
                domowik_scene::pick::pick(
                    black_box(&scene.nodes),
                    origin,
                    direction,
                    [0., f64::INFINITY],
                    |_| Some(view),
                    |_| false,
                )
            })
        });
    }
    c.bench_function("3D frame / bounds of 1000 5k-triangle nodes", |b| {
        b.iter(|| domowik_scene::pick::world_bounds(black_box(&scene.nodes), |_| Some(view.bounds)))
    });
    let selected = doc.asset_instances.iter().map(|a| a.id).collect();
    c.bench_function("3D highlight / tint 1000 selected nodes", |b| {
        b.iter_batched_ref(
            || scene.nodes.clone(),
            |nodes| domowik_scene::pick::highlight(nodes, black_box(&selected)),
            criterion::BatchSize::SmallInput,
        )
    });
}
criterion_group!(benches, bench);
criterion_main!(benches);
