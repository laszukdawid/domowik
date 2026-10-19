//! Renderer upload and frame-submission costs (issue 0101). Timings are
//! host-dependent and never a pass/fail gate; verify.sh only compiles this.

use domowik_renderer::{orientation_fixture, Instance, Renderer};
use std::time::Instant;

fn main() {
    let mut r = match Renderer::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("SKIPPED: {e}");
            return;
        }
    };
    let (mesh, base) = orientation_fixture();
    // 1,000 shared instances: the representative-corpus scale of 0033.
    let instances: Vec<Instance> = (0..1000)
        .map(|k| {
            let mut i = base[k % 2].clone();
            i.matrix[12] = (k % 40) as f32 * 0.05 - 1.0;
            i.matrix[13] = (k / 40) as f32 * 0.08 - 1.0;
            i
        })
        .collect();
    let start = Instant::now();
    r.set_scene([("diagnostic:orientation", mesh.as_slice())], &instances)
        .unwrap();
    println!("set_scene (1 mesh, 1000 instances): {:?}", start.elapsed());
    let start = Instant::now();
    for _ in 0..100 {
        r.set_scene([("diagnostic:orientation", mesh.as_slice())], &instances)
            .unwrap();
    }
    println!(
        "set_scene unchanged mesh, mean: {:?}",
        start.elapsed() / 100
    );
    let target = r.offscreen(1920, 1080);
    let view = target.create_view(&Default::default());
    let camera = domowik_renderer::gl_to_webgpu_depth([
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., -1., -1., 0., 0., -0.2, 0.,
    ]);
    let mut samples = Vec::new();
    for _ in 0..200 {
        let start = Instant::now();
        let index = r.render(&view, 1920, 1080, &camera).unwrap();
        r.wait(index).unwrap();
        samples.push(start.elapsed());
    }
    samples.sort();
    let pct = |p: usize| samples[samples.len() * p / 100];
    println!(
        "1920x1080 frame, submit to GPU complete: p50 {:?} p95 {:?} p99 {:?}",
        pct(50),
        pct(95),
        pct(99)
    );
}
