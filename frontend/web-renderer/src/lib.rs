//! The browser 3D presenter (issue 0088, ADR 0012).
//!
//! [`WebRenderer`] owns one WebGPU device and one canvas surface and wraps the
//! shared [`domowik_renderer::Renderer`]: the same pipelines, shader and
//! scene contract as the macOS presenter. Scenes arrive as
//! `domowik-scene-wire` frames from the core module; camera matrices arrive
//! from the editor in GL depth convention, as on macOS.
//!
//! # Threading and errors
//!
//! Everything runs on the page's thread; JavaScript holds the object and calls
//! it in order. Creation is asynchronous (adapter and device requests are
//! promises) and rejects with a message when WebGPU is missing. Other calls
//! return `Err(message)` and leave the previous scene and frame in place. GPU
//! validation errors are recorded in the statistics' `error` field.

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::WebRenderer;

/// One placed mesh, keyed by mesh name as the renderer takes it.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub mesh: String,
    pub matrix: [f32; 16],
    pub normal_matrix: [f32; 9],
    pub color: [f32; 4],
    pub transparent: bool,
}

/// A decoded frame's instances with their mesh indices resolved to names.
/// Shared by the browser wrapper and its native-run tests.
pub fn instances(frame: &domowik_scene_wire::Frame) -> Vec<Placed> {
    frame
        .instances
        .iter()
        .map(|i| Placed {
            mesh: frame.meshes[i.mesh as usize].0.clone(),
            matrix: i.matrix,
            normal_matrix: i.normal_matrix,
            color: i.color,
            transparent: i.transparent,
        })
        .collect()
}

/// The JSON the presenter hands back after a scene loads: the frame header's
/// camera framing and diagnostics, plus its producer extras.
pub fn scene_summary(frame: &domowik_scene_wire::Frame) -> String {
    let h = &frame.header;
    let mut out = serde_json::json!({
        "originMm": h.origin_mm,
        "center": h.center,
        "radius": h.radius,
        "diagnostics": h.diagnostics,
        "instances": frame.instances.len(),
        "meshes": frame.meshes.len(),
    });
    for (key, value) in &h.extra {
        out[key] = value.clone();
    }
    out.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use domowik_scene_wire::{decode, encode, Header, Instance};

    #[test]
    fn a_frame_maps_to_renderer_instances_and_a_summary() {
        let wall: Vec<f32> = vec![0.5; 30];
        let bytes = encode(
            Header {
                origin_mm: [1000.0, 2000.0],
                radius: 4.0,
                extra: serde_json::Map::from_iter([("modelDecodes".into(), 2.into())]),
                ..Header::default()
            },
            [("wall", wall.as_slice())],
            &[Instance {
                mesh: 0,
                transparent: true,
                matrix: [1.0; 16],
                normal_matrix: [0.0; 9],
                color: [0.1, 0.2, 0.3, 0.4],
            }],
        )
        .unwrap();
        let frame = decode(&bytes).unwrap();
        let mapped = instances(&frame);
        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].mesh, "wall");
        assert!(mapped[0].transparent);
        let summary: serde_json::Value = serde_json::from_str(&scene_summary(&frame)).unwrap();
        assert_eq!(summary["originMm"], serde_json::json!([1000.0, 2000.0]));
        assert_eq!(summary["modelDecodes"], 2);
        assert_eq!(summary["instances"], 1);
    }
}
