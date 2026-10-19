//! Original 0109 TV-placement fixtures, derived by elementary right-angle rotation.
use domowik_scene::asset_pose::{matrices, projected_dimensions, rotate};

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-5, "{a} != {b}");
}

#[test]
fn tv_can_stand_upright_and_tilt_ten_degrees() {
    let upright = projected_dimensions([1200., 700., 50.], [90., 0.]);
    close(upright[0], 1200.);
    close(upright[1], 50.);
    let tilted = projected_dimensions([1200., 700., 50.], [100., 0.]);
    close(
        tilted[1],
        700. * 10_f64.to_radians().sin() + 50. * 10_f64.to_radians().cos(),
    );
}

#[test]
fn rotation_axes_are_right_handed_plan_axes() {
    let rotated = rotate([0., 0., -1.], [90., 0., 0.]);
    close(rotated[1], 1.);
    let rotated = rotate([1., 0., 0.], [0., 90., 0.]);
    close(rotated[1], -1.);
    let rotated = rotate([1., 0., 0.], [0., 0., 90.]);
    close(rotated[2], -1.);
}

#[test]
fn center_pivot_and_normal_matrix_are_consistent_under_nonuniform_scale() {
    let (m, n) = matrices([2., 3., 4.], [1.2, 0.05, 0.7], [100., 10., 30.], 1.);
    for row in 0..3 {
        close(
            f64::from(m[12 + row]) + f64::from(m[4 + row]) * 0.5,
            [2., 3.025, 4.][row],
        );
    }
    // Inverse-transpose normals are dual to the transformed tangent basis.
    for a in 0..3 {
        for b in 0..3 {
            let dot: f64 = (0..3)
                .map(|r| f64::from(m[a * 4 + r]) * f64::from(n[b * 3 + r]))
                .sum();
            close(dot, if a == b { 1. } else { 0. });
        }
    }
}
