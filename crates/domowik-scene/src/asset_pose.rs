//! Original issue-0109 Euler pose convention. Units are mm/degrees; rotations
//! are right-handed plan X, Y, Z, applied in that order about the cuboid center.
//! This is a view transform, not an exact geometry predicate.
use domowik_format::AssetInstance;

/// Renderer-neutral placement inputs shared by full scene builds and warm
/// pose-only updates. Position/scales are metres; angles are plan X/Y/Z degrees.
pub struct Placement {
    pub position_m: [f64; 3],
    pub scale_m: [f64; 3],
    pub angles_deg: [f64; 3],
}

/// Computes one instance's placement without deriving any wall/room geometry.
/// Uses the same optional dimensions and legacy height inference as scene builds.
/// `height_ratio` is the positive normalized height of the resident mesh.
pub fn placement(
    a: &AssetInstance,
    origin_mm: [f64; 2],
    level_mm: f64,
    height_ratio: f64,
) -> Placement {
    let dims = a
        .unknown
        .get("editorFootprintMm")
        .and_then(|v| serde_json::from_value::<[f64; 2]>(v.clone()).ok())
        .filter(|p| p.iter().all(|v| v.is_finite() && *v > 0.))
        .unwrap_or([1000.; 2]);
    let vertical = a
        .unknown
        .get("editorHeightMm")
        .and_then(serde_json::Value::as_f64)
        .filter(|h| h.is_finite() && *h > 0. && *h <= 1_000_000.)
        .map(|h| h / height_ratio)
        .unwrap_or(dims[0]);
    let [elevation, rx, ry] = extra_pose(a);
    let uniform = a.transform.scale_uniform;
    Placement {
        position_m: [
            (a.transform.translation_mm[0] - origin_mm[0]) / 1000.,
            (level_mm + elevation) / 1000.,
            -(a.transform.translation_mm[1] - origin_mm[1]) / 1000.,
        ],
        scale_m: [
            dims[0] * uniform / 1000.,
            vertical * uniform / 1000.,
            dims[1] * uniform / 1000.,
        ],
        angles_deg: [rx, ry, a.transform.rotation_deg],
    }
}

/// Optional editor pose: elevation offset, X rotation, Y rotation. Legacy and
/// malformed optional metadata defaults to zero; mutation APIs reject invalid
/// new input. Z rotation remains the authoritative transform's rotation_deg.
pub fn extra_pose(asset: &AssetInstance) -> [f64; 3] {
    [
        "editorElevationMm",
        "editorRotationXDeg",
        "editorRotationYDeg",
    ]
    .map(|key| {
        asset
            .unknown
            .get(key)
            .and_then(serde_json::Value::as_f64)
            .filter(|v| {
                v.is_finite()
                    && (key != "editorElevationMm" || v.abs() <= domowik_format::COORD_BOUND_MM)
            })
            .unwrap_or(0.)
    })
}

/// Rotate a world-space vector (+Y vertical, plan Y = world -Z), without
/// translation or scaling. Inputs must be finite; angles are reduced modulo
/// 360 before trigonometry. Output is deterministic binary64, not exact-real.
pub fn rotate(v: [f64; 3], angles: [f64; 3]) -> [f64; 3] {
    let [x, y, z] = [v[0], -v[2], v[1]];
    let [(sx, cx), (sy, cy), (sz, cz)] = angles.map(|a| a.rem_euclid(360.).to_radians().sin_cos());
    let (y, z) = (cx * y - sx * z, sx * y + cx * z);
    let (x, z) = (cy * x + sy * z, -sy * x + cy * z);
    [cz * x - sz * y, z, -(sz * x + cz * y)]
}

/// Column-major model/normal matrices for a centered asset cuboid. `height_ratio`
/// is the mesh's normalized local height; scales and position are world metres.
/// Rotation preserves the unrotated cuboid center, not its bottom face.
/// Requires positive finite scales, finite position/angles, and a nonnegative
/// finite mesh height ratio. Uses binary64 arithmetic, returning rounded f32;
/// callers must keep derived coordinates within their renderer's range.
pub fn matrices(
    position: [f64; 3],
    scale: [f64; 3],
    angles: [f64; 3],
    height_ratio: f64,
) -> ([f32; 16], [f32; 9]) {
    if angles[0] == 0. && angles[1] == 0. {
        return super::matrix(position, scale, angles[2]);
    }
    let basis = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]].map(|v| rotate(v, angles));
    let center_height = scale[1] * height_ratio / 2.;
    let mut m = [0.; 16];
    let mut n = [0.; 9];
    for column in 0..3 {
        for row in 0..3 {
            m[column * 4 + row] = (basis[column][row] * scale[column]) as f32;
            n[column * 3 + row] = (basis[column][row] / scale[column]) as f32;
        }
    }
    for row in 0..3 {
        m[12 + row] = (position[row] + if row == 1 { center_height } else { 0. }
            - basis[1][row] * center_height) as f32;
    }
    m[15] = 1.;
    (m, n)
}

/// Dimensions of the conservative yaw-aligned projection of an unrotated
/// width/depth/height cuboid, after X/Y tilt. Used by 2D paint and hit testing.
pub fn projected_dimensions(dimensions: [f64; 3], angles_xy: [f64; 2]) -> [f64; 2] {
    let [w, d, h] = dimensions;
    let basis = [[w, 0., 0.], [0., h, 0.], [0., 0., d]]
        .map(|v| rotate(v, [angles_xy[0], angles_xy[1], 0.]));
    [
        basis.iter().map(|v| v[0].abs()).sum(),
        basis.iter().map(|v| v[2].abs()).sum(),
    ]
}
