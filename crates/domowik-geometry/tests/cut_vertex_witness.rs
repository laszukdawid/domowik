use domowik_format::Point;
use domowik_geometry::boolean::union;

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Point> {
    vec![
        Point::new(x0, y0),
        Point::new(x1, y0),
        Point::new(x1, y1),
        Point::new(x0, y1),
    ]
}

#[test]
fn a_union_ring_reproduces_every_input_vertex_it_passes_through() {
    let a = rect(0.0, 0.0, 2864.5859375, 13658.0546875);
    let b = rect(2864.5859375, 741.6640625, 3364.5859375, 3591.8203125);
    let out = union(&[a.clone(), b.clone()]);
    assert_eq!(out.len(), 1, "the rectangles touch, so they merge");

    let inputs: Vec<Point> = a.iter().chain(b.iter()).copied().collect();
    for v in &out[0].outer {
        // Every ring vertex here IS an input corner; none is constructed.
        let exact = inputs.iter().any(|i| i.x == v.x && i.y == v.y);
        let near = inputs
            .iter()
            .find(|i| (i.x - v.x).abs() < 1e-6 && (i.y - v.y).abs() < 1e-6);
        assert!(
            exact,
            "ring vertex {v:?} is {} an input corner {:?}",
            if near.is_some() {
                "one ulp off"
            } else {
                "nowhere near"
            },
            near
        );
    }
}
