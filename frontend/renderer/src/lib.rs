//! Shared GPU renderer for the editor's 3D view (issue 0101).
//!
//! One `wgpu` renderer and one WGSL shader serve every platform presenter:
//! Metal through an IOSurface-backed texture on macOS, WebGPU through a page
//! canvas in the browser (issue 0088, `frontend/web-renderer`). The presenter
//! owns the colour target; this crate owns the device, pipelines, depth
//! target, meshes and instances.
//!
//! # Conventions
//!
//! - World space is `domowik-scene`'s: metres, +Y up, origin-rebased.
//! - Vertices are [`VERTEX_FLOATS`] `f32`s: position xyz, normal xyz, rgba.
//!   Optional parallel UV/layer/wrap attributes and sRGB pixels arrive as
//!   [`TextureData`]; untextured geometry retains its colour-only layout.
//! - The camera is a column-major clip-from-world matrix with WebGPU depth in
//!   `[0, 1]`. A GL-convention matrix (depth `[-1, 1]`) goes through
//!   [`gl_to_webgpu_depth`] first.
//! - Colour targets are [`COLOR_FORMAT`] natively, or the format a browser
//!   canvas prefers ([`Renderer::with_device`]); always non-sRGB, written
//!   with the same values the legacy GL renderer writes; row 0 is the top row.
//!
//! # Threading and errors
//!
//! A [`Renderer`] is `Send` but not shared: callers serialize access (the
//! macOS presenter holds it behind one mutex). No method blocks on the GPU
//! except [`Renderer::wait`] and the explicit diagnostic readback, and neither
//! is meant for a UI or raster thread. Invalid input is refused with an error
//! string before any GPU work; GPU validation errors surface through `wgpu`'s
//! device error callback, which this crate records in [`Stats::error`].

use std::collections::{hash_map::DefaultHasher, HashMap};
use std::hash::Hasher;
use std::sync::{Arc, Mutex};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

#[cfg(target_vendor = "apple")]
pub mod metal;

/// Floats per vertex: position (3), normal (3), colour (4).
pub const VERTEX_FLOATS: usize = 10;
/// Floats per instance: model matrix (16), normal matrix (9), tint (4).
pub const INSTANCE_FLOATS: usize = 29;
/// The colour target format every presenter must supply.
pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Same background as the legacy GL renderer (0.84, 0.90, 0.94).
const CLEAR: wgpu::Color = wgpu::Color {
    r: 0.84,
    g: 0.9,
    b: 0.94,
    a: 1.0,
};
/// Largest accepted target edge, matching the Dart camera clamp.
pub const MAX_EDGE: u32 = 4096;
/// Largest accepted instance count per scene, matching the Linux channel.
pub const MAX_INSTANCES: usize = 20_000;

/// One placed mesh instance, as `domowik-scene` derives it.
#[derive(Clone, Debug)]
pub struct Instance {
    pub mesh: String,
    pub matrix: [f32; 16],
    pub normal_matrix: [f32; 9],
    pub color: [f32; 4],
    pub transparent: bool,
}

/// Counters for probes and benchmarks. Upload counters only move when scene
/// data changes, so navigation can be checked to perform no uploads.
#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub frames: u64,
    pub mesh_uploads: u64,
    pub instance_uploads: u64,
    pub scene_updates: u64,
    pub triangles: u64,
    pub draw_calls: u64,
    /// CPU time to encode and submit the last frame; not GPU time. Zero in
    /// the browser, whose presenter times frames itself.
    pub submit_us: f64,
    pub adapter: String,
    pub error: String,
}

struct Mesh {
    buffer: wgpu::Buffer,
    coordinates: wgpu::Buffer,
    texture: wgpu::BindGroup,
    texture_bytes: usize,
    vertices: u32,
    hash: u64,
}

struct Group {
    mesh: String,
    transparent: bool,
    buffer: wgpu::Buffer,
    count: u32,
    data: Vec<f32>,
}

/// The GPU renderer. See the crate documentation for its contract.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    opaque: wgpu::RenderPipeline,
    blended: wgpu::RenderPipeline,
    camera: wgpu::Buffer,
    bind: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    meshes: HashMap<String, Mesh>,
    groups: Vec<Group>,
    depth: Option<(u32, u32, wgpu::TextureView)>,
    stats: Stats,
    errors: Arc<Mutex<DeviceErrors>>,
}

/// Borrowed model texture array. RGBA8 is sRGB, rows top-first; five floats per
/// vertex: UV, layer (-1 untextured), wrap S/T (0 repeat, 1 clamp, 2 mirror).
/// Borrowed only for scene upload, copied into renderer-owned GPU resources.
pub struct TextureData<'a> {
    pub width: u32,
    pub height: u32,
    pub layers: u32,
    pub rgba: &'a [u8],
    pub coordinates: &'a [f32],
}

fn validate_texture(t: &TextureData<'_>, vertices: usize) -> Result<(), String> {
    if t.width == 0
        || t.height == 0
        || t.width > 2048
        || t.height > 2048
        || t.layers == 0
        || t.layers > 16
    {
        return Err("Invalid texture array dimensions".into());
    }
    let size = u64::from(t.width) * u64::from(t.height) * u64::from(t.layers) * 4;
    if size > 64 * 1024 * 1024
        || size != t.rgba.len() as u64
        || t.coordinates.len() != vertices / VERTEX_FLOATS * 5
    {
        return Err("Invalid texture array payload".into());
    }
    for c in t.coordinates.as_chunks::<5>().0 {
        if c.iter().any(|v| !v.is_finite())
            || c[0].abs() > 1e6
            || c[1].abs() > 1e6
            || c[2] < -1.
            || c[2] >= t.layers as f32
            || c[2].fract() != 0.
            || c[3..].iter().any(|v| *v < 0. || *v > 2. || v.fract() != 0.)
        {
            return Err("Invalid texture coordinates/layer/wrap".into());
        }
    }
    Ok(())
}

/// Uncaptured device errors: `pending` until a frame reports it, `last` for
/// statistics.
#[derive(Default)]
struct DeviceErrors {
    pending: Option<String>,
    last: String,
}

/// Converts a GL clip matrix (depth `[-1, 1]`) to WebGPU's `[0, 1]`:
/// `z' = (z + w) / 2`, applied to the matrix's third row.
pub fn gl_to_webgpu_depth(mut m: [f32; 16]) -> [f32; 16] {
    for c in 0..4 {
        m[c * 4 + 2] = 0.5 * m[c * 4 + 2] + 0.5 * m[c * 4 + 3];
    }
    m
}

#[cfg(not(target_arch = "wasm32"))]
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    use std::task::{Context, Poll, Wake, Waker};
    struct Thread(std::thread::Thread);
    impl Wake for Thread {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Thread(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::park(),
        }
    }
}

fn bytes(values: &[f32]) -> &[u8] {
    // SAFETY: f32 has no padding or invalid bit patterns, u8 has alignment 1,
    // and the byte length is exactly the float slice's.
    unsafe { std::slice::from_raw_parts(values.as_ptr().cast(), std::mem::size_of_val(values)) }
}

fn digest(values: &[f32]) -> u64 {
    let mut h = DefaultHasher::new();
    h.write(bytes(values));
    h.finish()
}

impl Renderer {
    /// Creates a renderer on the platform's primary backend (Metal on Apple).
    /// Fails, rather than panicking, when no backend is compiled in or no
    /// adapter exists, so headless hosts can report "no GPU". Native only: a
    /// browser cannot block on the adapter, so its presenter creates the
    /// device asynchronously and calls [`Renderer::with_device`].
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new() -> Result<Self, String> {
        if wgpu::Instance::enabled_backend_features().is_empty() {
            return Err("no GPU backend is enabled for this platform".into());
        }
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|e| format!("no GPU adapter: {e}"))?;
        let info = adapter.get_info();
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("domowik-3d"),
            required_limits:
                wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .map_err(|e| format!("GPU device: {e}"))?;
        Ok(Self::with_device(
            device,
            queue,
            format!("{} ({:?})", info.name, info.backend),
            COLOR_FORMAT,
        ))
    }

    /// A renderer on a device the caller created, drawing into `color` targets.
    ///
    /// `color` must be a non-sRGB 8-bit RGBA or BGRA format; the browser
    /// presenter passes the canvas's preferred one. `adapter` names the
    /// adapter in [`Stats`].
    pub fn with_device(
        device: wgpu::Device,
        queue: wgpu::Queue,
        adapter: String,
        color: wgpu::TextureFormat,
    ) -> Self {
        let errors = Arc::new(Mutex::new(DeviceErrors::default()));
        let sink = errors.clone();
        device.on_uncaptured_error(Arc::new(move |e| {
            let mut errors = sink.lock().unwrap_or_else(|p| p.into_inner());
            errors.last = e.to_string();
            errors.pending = Some(errors.last.clone());
        }));
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scene.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera.as_entire_binding(),
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("base-colour texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene"),
            bind_group_layouts: &[Some(&layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let vertex_attributes = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x3, 2 => Float32x4
        ];
        let instance_attributes = wgpu::vertex_attr_array![
            3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4,
            7 => Float32x3, 8 => Float32x3, 9 => Float32x3, 10 => Float32x4
        ];
        let texture_attributes = wgpu::vertex_attr_array![11 => Float32x3, 12 => Float32x2];
        let buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: (VERTEX_FLOATS * 4) as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &vertex_attributes,
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: (INSTANCE_FLOATS * 4) as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &instance_attributes,
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: 20,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &texture_attributes,
            }),
        ];
        let pipeline = |transparent: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(if transparent { "blended" } else { "opaque" }),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &buffers,
                },
                // Two-sided lighting: nothing is culled and the fragment
                // stage flips back-face normals, as the GL renderer does.
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(!transparent),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: color,
                        blend: transparent.then_some(wgpu::BlendState {
                            color: wgpu::BlendComponent {
                                src_factor: wgpu::BlendFactor::SrcAlpha,
                                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                                operation: wgpu::BlendOperation::Add,
                            },
                            alpha: wgpu::BlendComponent {
                                src_factor: wgpu::BlendFactor::One,
                                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                                operation: wgpu::BlendOperation::Add,
                            },
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let opaque = pipeline(false);
        let blended = pipeline(true);
        Self {
            opaque,
            blended,
            camera,
            bind,
            texture_layout,
            meshes: HashMap::new(),
            groups: Vec::new(),
            depth: None,
            stats: Stats {
                adapter,
                ..Default::default()
            },
            errors,
            device,
            queue,
        }
    }

    /// The device; presenters import their colour targets through it.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// The queue; the browser presenter presents canvas frames through it.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Counters, with the latest recorded device error.
    pub fn stats(&self) -> Stats {
        let mut stats = self.stats.clone();
        stats.error = self
            .errors
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .last
            .clone();
        stats
    }

    /// Replaces the scene. Meshes whose bytes are unchanged are not
    /// re-uploaded; meshes no instance references are released. Every
    /// instance's mesh must be in `meshes` or already resident.
    ///
    /// Refuses, changing nothing, on a vertex buffer that is not whole
    /// triangles, a non-finite value, a missing mesh or too many instances.
    pub fn set_scene<'a>(
        &mut self,
        meshes: impl IntoIterator<Item = (&'a str, &'a [f32])>,
        instances: &[Instance],
    ) -> Result<(), String> {
        self.set_scene_textured(meshes, instances, [])
    }

    /// Like [`Self::set_scene`], including embedded texture arrays. A complete
    /// replacement of a mesh without a texture removes its old texture. Warm
    /// pose updates omit both, retaining resident resources. Validation is
    /// atomic; no image decoding or texture upload occurs on camera updates.
    pub fn set_scene_textured<'a>(
        &mut self,
        meshes: impl IntoIterator<Item = (&'a str, &'a [f32])>,
        instances: &[Instance],
        textures: impl IntoIterator<Item = (&'a str, TextureData<'a>)>,
    ) -> Result<(), String> {
        let textures: HashMap<_, _> = textures.into_iter().collect();
        if instances.len() > MAX_INSTANCES {
            return Err(format!(
                "{} instances exceed {MAX_INSTANCES}",
                instances.len()
            ));
        }
        let mut incoming: HashMap<&str, (&[f32], u64)> = HashMap::new();
        for (key, vertices) in meshes {
            if vertices.len() % (VERTEX_FLOATS * 3) != 0 {
                return Err(format!("mesh {key}: vertex buffer is not whole triangles"));
            }
            if !vertices.iter().all(|v| v.is_finite()) {
                return Err(format!("mesh {key}: non-finite vertex value"));
            }
            let mut h = DefaultHasher::new();
            h.write_u64(digest(vertices));
            if let Some(t) = textures.get(key) {
                validate_texture(t, vertices.len())?;
                h.write_u32(t.width);
                h.write_u32(t.height);
                h.write_u32(t.layers);
                h.write(t.rgba);
                h.write(bytes(t.coordinates));
            }
            incoming.insert(key, (vertices, h.finish()));
        }
        if textures.keys().any(|k| !incoming.contains_key(k)) {
            return Err("Texture references missing incoming mesh".into());
        }
        if textures.values().map(|t| t.rgba.len()).sum::<usize>() > 128 * 1024 * 1024 {
            return Err("Scene textures exceed 128 MiB".into());
        }
        for i in instances {
            if !incoming.contains_key(i.mesh.as_str()) && !self.meshes.contains_key(&i.mesh) {
                return Err(format!("instance references unknown mesh {}", i.mesh));
            }
            if !i
                .matrix
                .iter()
                .chain(&i.normal_matrix)
                .chain(&i.color)
                .all(|v| v.is_finite())
            {
                return Err(format!("instance of {}: non-finite transform", i.mesh));
            }
        }
        let used: std::collections::HashSet<_> =
            instances.iter().map(|i| i.mesh.as_str()).collect();
        let resident_bytes: usize = used
            .iter()
            .map(|key| {
                if incoming.contains_key(key) {
                    textures.get(key).map_or(0, |t| t.rgba.len())
                } else {
                    self.meshes.get(*key).map_or(0, |m| m.texture_bytes)
                }
            })
            .sum();
        if resident_bytes > 128 * 1024 * 1024 {
            return Err("Resident scene textures exceed 128 MiB".into());
        }
        for (key, (vertices, hash)) in incoming {
            if self.meshes.get(key).is_some_and(|m| m.hash == hash) {
                continue;
            }
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("mesh"),
                size: (vertices.len() * 4).max(4) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            if !vertices.is_empty() {
                self.queue.write_buffer(&buffer, 0, bytes(vertices));
            }
            let defaults: Vec<f32> = if textures.contains_key(key) {
                Vec::new()
            } else {
                (0..vertices.len() / VERTEX_FLOATS)
                    .flat_map(|_| [0., 0., -1., 0., 0.])
                    .collect()
            };
            let fallback = TextureData {
                width: 1,
                height: 1,
                layers: 1,
                rgba: &[255; 4],
                coordinates: &defaults,
            };
            let t = textures.get(key).unwrap_or(&fallback);
            let coordinates = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("texture coordinates"),
                size: (t.coordinates.len() * 4).max(4) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            if !t.coordinates.is_empty() {
                self.queue
                    .write_buffer(&coordinates, 0, bytes(t.coordinates));
            }
            let image = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("base-colour array"),
                size: wgpu::Extent3d {
                    width: t.width,
                    height: t.height,
                    depth_or_array_layers: t.layers,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            self.queue.write_texture(
                image.as_image_copy(),
                t.rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(t.width * 4),
                    rows_per_image: Some(t.height),
                },
                image.size(),
            );
            let view = image.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });
            let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
            let texture = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("base-colour"),
                layout: &self.texture_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            });
            self.meshes.insert(
                key.to_owned(),
                Mesh {
                    buffer,
                    coordinates,
                    texture,
                    texture_bytes: t.rgba.len(),
                    vertices: (vertices.len() / VERTEX_FLOATS) as u32,
                    hash,
                },
            );
            self.stats.mesh_uploads += 1;
        }
        // Transparent groups are not sorted by camera depth: two overlapping
        // differently tinted panes (glass doors, ghosted levels) can blend in
        // the wrong order. Rare in plans, and sorting would cost work on every
        // camera move. The Linux GL presenter draws in the same order.
        // Group by (mesh, transparency) in first-appearance order so the draw
        // order, and hence blending, is deterministic for a given scene.
        let mut order: Vec<(String, bool)> = Vec::new();
        let mut grouped: HashMap<(String, bool), Vec<f32>> = HashMap::new();
        for i in instances {
            let key = (i.mesh.clone(), i.transparent);
            let data = grouped.entry(key.clone()).or_insert_with(|| {
                order.push(key);
                Vec::new()
            });
            data.extend_from_slice(&i.matrix);
            data.extend_from_slice(&i.normal_matrix);
            data.extend_from_slice(&i.color);
        }
        let mut previous = std::mem::take(&mut self.groups).into_iter();
        for key in order {
            let data = grouped.remove(&key).expect("grouped above");
            let old = previous.next().filter(|g| {
                g.mesh == key.0 && g.transparent == key.1 && g.data.len() == data.len()
            });
            if let Some(mut group) = old {
                if group.data != data {
                    self.queue.write_buffer(&group.buffer, 0, bytes(&data));
                    self.stats.instance_uploads += 1;
                    group.data = data;
                }
                self.groups.push(group);
            } else {
                let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("instances"),
                    size: (data.len() * 4) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.queue.write_buffer(&buffer, 0, bytes(&data));
                self.stats.instance_uploads += 1;
                self.groups.push(Group {
                    count: (data.len() / INSTANCE_FLOATS) as u32,
                    mesh: key.0,
                    transparent: key.1,
                    buffer,
                    data,
                });
            }
        }
        let referenced: std::collections::HashSet<&str> =
            self.groups.iter().map(|g| g.mesh.as_str()).collect();
        self.meshes.retain(|k, _| referenced.contains(k.as_str()));
        self.stats.scene_updates += 1;
        Ok(())
    }

    /// Encodes and submits one frame into `target` (a view of `width` ×
    /// `height` in the renderer's colour format). Returns without waiting for the GPU; pass the
    /// result to [`Renderer::wait`] before handing `target` to a consumer.
    pub fn render(
        &mut self,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
        camera: &[f32; 16],
    ) -> Result<wgpu::SubmissionIndex, String> {
        if width == 0 || height == 0 || width > MAX_EDGE || height > MAX_EDGE {
            return Err(format!("target {width}x{height} is outside 1..={MAX_EDGE}"));
        }
        if !camera.iter().all(|v| v.is_finite()) {
            return Err("non-finite camera matrix".into());
        }
        #[cfg(not(target_arch = "wasm32"))]
        let start = Instant::now();
        if !matches!(&self.depth, Some((w, h, _)) if *w == width && *h == height) {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            self.depth = Some((width, height, texture.create_view(&Default::default())));
        }
        let depth = &self.depth.as_ref().expect("depth target allocated above").2;
        self.queue.write_buffer(&self.camera, 0, bytes(camera));
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        let (mut draws, mut triangles) = (0u64, 0u64);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(CLEAR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.bind, &[]);
            for transparent in [false, true] {
                pass.set_pipeline(if transparent {
                    &self.blended
                } else {
                    &self.opaque
                });
                for g in self.groups.iter().filter(|g| g.transparent == transparent) {
                    let Some(mesh) = self.meshes.get(&g.mesh) else {
                        continue;
                    };
                    if mesh.vertices == 0 || g.count == 0 {
                        continue;
                    }
                    pass.set_vertex_buffer(0, mesh.buffer.slice(..));
                    pass.set_vertex_buffer(1, g.buffer.slice(..));
                    pass.set_vertex_buffer(2, mesh.coordinates.slice(..));
                    pass.set_bind_group(1, &mesh.texture, &[]);
                    pass.draw(0..mesh.vertices, 0..g.count);
                    draws += 1;
                    triangles += u64::from(mesh.vertices / 3) * u64::from(g.count);
                }
            }
        }
        let index = self.queue.submit([encoder.finish()]);
        self.stats.frames += 1;
        self.stats.draw_calls = draws;
        self.stats.triangles = triangles;
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.stats.submit_us = start.elapsed().as_secs_f64() * 1e6;
        }
        Ok(index)
    }

    /// Blocks until `index` has executed. For render threads and tests only.
    ///
    /// Fails if the GPU did not finish within five seconds, or if `wgpu`
    /// reported a validation or device error since the last call: a frame
    /// that may be wrong is never reported as complete.
    pub fn wait(&self, index: wgpu::SubmissionIndex) -> Result<(), String> {
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(index),
                timeout: Some(std::time::Duration::from_secs(5)),
            })
            .map_err(|e| format!("GPU wait: {e}"))?;
        match self
            .errors
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .pending
            .take()
        {
            Some(e) => Err(format!("GPU error: {e}")),
            None => Ok(()),
        }
    }

    /// Diagnostic readback of a [`COLOR_FORMAT`] texture created with
    /// `COPY_SRC`, as tightly packed BGRA rows. Blocks; never on normal frames.
    pub fn read_pixels(&self, texture: &wgpu::Texture) -> Result<Vec<u8>, String> {
        let (width, height) = (texture.width(), texture.height());
        let row = (width * 4).div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(height),
                },
            },
            texture.size(),
        );
        let index = self.queue.submit([encoder.finish()]);
        buffer.map_async(wgpu::MapMode::Read, .., |_| {});
        self.wait(index)?;
        let mapped = buffer.get_mapped_range(..).map_err(|e| e.to_string())?;
        let mut out = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height as usize {
            let start = y * row as usize;
            out.extend_from_slice(&mapped[start..start + (width * 4) as usize]);
        }
        Ok(out)
    }

    /// An offscreen colour target for tests, benchmarks and headless probes.
    pub fn offscreen(&self, width: u32, height: u32) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    }
}

/// The asymmetric orientation fixture the Linux probe also draws: a red quad
/// at world +Y and a blue quad at world -Y under an identity camera.
pub fn orientation_fixture() -> (Vec<f32>, Vec<Instance>) {
    let mut vertices = Vec::new();
    for p in [
        [-0.3, -0.15, 0.],
        [0.3, -0.15, 0.],
        [0.3, 0.15, 0.],
        [-0.3, -0.15, 0.],
        [0.3, 0.15, 0.],
        [-0.3, 0.15, 0.],
    ] {
        vertices.extend_from_slice(&p);
        vertices.extend_from_slice(&[0., 0., 1., 1., 1., 1., 1.]);
    }
    let instances = [true, false]
        .map(|top| Instance {
            mesh: "diagnostic:orientation".into(),
            matrix: [
                1.,
                0.,
                0.,
                0.,
                0.,
                1.,
                0.,
                0.,
                0.,
                0.,
                1.,
                0.,
                0.,
                if top { 0.5 } else { -0.5 },
                0.5,
                1.,
            ],
            normal_matrix: [1., 0., 0., 0., 1., 0., 0., 0., 1.],
            color: if top {
                [1., 0., 0., 1.]
            } else {
                [0., 0., 1., 1.]
            },
            transparent: false,
        })
        .to_vec();
    (vertices, instances)
}

/// Counts red pixels in the top half and blue pixels in the bottom half of a
/// BGRA image, the same classification the Linux probe applies.
pub fn orientation_counts(bgra: &[u8], width: u32) -> (u64, u64) {
    let (mut top_red, mut bottom_blue) = (0, 0);
    let height = bgra.len() / 4 / width as usize;
    for (i, px) in bgra.as_chunks::<4>().0.iter().enumerate() {
        let (b, g, r) = (u32::from(px[0]), u32::from(px[1]), u32::from(px[2]));
        let row = i / width as usize;
        if row < height / 2 && r > 80 && r > 2 * b && r > 2 * g {
            top_red += 1;
        }
        if row >= height / 2 && b > 80 && b > 2 * r && b > 2 * g {
            bottom_blue += 1;
        }
    }
    (top_red, bottom_blue)
}

/// Pixels that differ from the clear colour by more than the Linux probe's
/// threshold: evidence that geometry was rasterised.
pub fn non_background_pixels(bgra: &[u8]) -> u64 {
    bgra.as_chunks::<4>()
        .0
        .iter()
        .filter(|px| {
            (i32::from(px[2]) - 214).abs()
                + (i32::from(px[1]) - 230).abs()
                + (i32::from(px[0]) - 240).abs()
                > 20
        })
        .count() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beige_in_shadow_is_lit_in_linear_space_for_plain_and_textured_surfaces() {
        let Some(mut r) = renderer() else { return };
        let (mut mesh, mut instances) = orientation_fixture();
        for vertex in mesh.as_chunks_mut::<10>().0 {
            vertex[3..6].copy_from_slice(&[0., 0., -1.]);
        }
        instances.truncate(1);
        instances[0].matrix[13] = 0.;
        instances[0].color = [232. / 255., 220. / 255., 196. / 255., 1.];
        let decode = |v: f64| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        let encode = |v: f64| {
            if v <= 0.0031308 {
                v * 12.92
            } else {
                1.055 * v.powf(1. / 2.4) - 0.055
            }
        };
        let coords = [0.5, 0.5, 0., 0., 0.].repeat(mesh.len() / 10);
        // Ordinary solid leaves, glass, and glass on a ghosted level all use
        // the same linear-light RGB, then the existing coverage blend pass.
        for alpha in [1., 0.25, 0.075] {
            instances[0].transparent = alpha < 1.;
            instances[0].color[3] = alpha;
            for textured in [false, true] {
                let textures = if textured {
                    vec![(
                        "diagnostic:orientation",
                        TextureData {
                            width: 1,
                            height: 1,
                            layers: 1,
                            rgba: &[128, 128, 128, 255],
                            coordinates: &coords,
                        },
                    )]
                } else {
                    vec![]
                };
                r.set_scene_textured(
                    [("diagnostic:orientation", mesh.as_slice())],
                    &instances,
                    textures,
                )
                .unwrap();
                let target = r.offscreen(32, 32);
                let frame = r
                    .render(&target.create_view(&Default::default()), 32, 32, &IDENTITY)
                    .unwrap();
                r.wait(frame).unwrap();
                let pixels = r.read_pixels(&target).unwrap();
                let pixel = &pixels[(16 * 32 + 16) * 4..][..4];
                for (actual, base) in [pixel[2], pixel[1], pixel[0]]
                    .into_iter()
                    .zip([232., 220., 196.])
                {
                    let lit = encode(
                        decode(base / 255.)
                            * if textured { decode(128. / 255.) } else { 1. }
                            * 0.38,
                    );
                    let channel = match base as u32 {
                        232 => CLEAR.r,
                        220 => CLEAR.g,
                        _ => CLEAR.b,
                    };
                    let expected =
                        (lit * f64::from(alpha) + channel * (1. - f64::from(alpha))) * 255.;
                    assert!(
                        (f64::from(actual) - expected).abs() <= 2.,
                        "textured={textured}, actual={pixel:?}, expected channel={expected}"
                    );
                }
                assert_eq!(pixel[3], 255);
            }
        }
    }

    #[test]
    fn texture_validation_is_bounded_without_a_gpu() {
        let coords = [0., 0., 0., 1., 1.].repeat(3);
        let mut t = TextureData {
            width: 1,
            height: 1,
            layers: 1,
            rgba: &[255; 4],
            coordinates: &coords,
        };
        assert!(validate_texture(&t, 30).is_ok());
        t.width = 4096;
        assert!(validate_texture(&t, 30).is_err());
        t.width = 1;
        t.layers = 17;
        assert!(validate_texture(&t, 30).is_err());
        t.layers = 1;
        t.coordinates = &[0.; 2];
        assert!(validate_texture(&t, 30).is_err());
    }

    #[test]
    fn embedded_texture_pixels_are_sampled_and_resources_reused() {
        let Some(mut r) = renderer() else { return };
        let (mut mesh, mut instances) = orientation_fixture();
        // Enlarge and centre one quad. Its left/right halves sample different
        // colours from ONE texture; uniform per-triangle tint cannot pass.
        for v in mesh.as_chunks_mut::<10>().0 {
            v[0] *= 3.;
            v[1] *= 5.;
        }
        instances.truncate(1);
        instances[0].matrix[13] = 0.;
        instances[0].color = [1.; 4];
        let coords: Vec<f32> = mesh
            .as_chunks::<10>()
            .0
            .iter()
            .flat_map(|v| [(v[0] / 1.8 + 0.5), 0.5, 0., 1., 1.])
            .collect();
        let pixels = [255, 0, 0, 255, 0, 0, 255, 255];
        let upload = |r: &mut Renderer, rgba: &[u8]| {
            r.set_scene_textured(
                [("diagnostic:orientation", mesh.as_slice())],
                &instances,
                [(
                    "diagnostic:orientation",
                    TextureData {
                        width: 2,
                        height: 1,
                        layers: 1,
                        rgba,
                        coordinates: &coords,
                    },
                )],
            )
        };
        upload(&mut r, &pixels).unwrap();
        upload(&mut r, &pixels).unwrap();
        assert_eq!(r.stats().mesh_uploads, 1);
        let target = r.offscreen(64, 64);
        let view = target.create_view(&Default::default());
        let frame = r.render(&view, 64, 64, &IDENTITY).unwrap();
        r.wait(frame).unwrap();
        let out = r.read_pixels(&target).unwrap();
        let left = &out[(32 * 64 + 10) * 4..][..4];
        let right = &out[(32 * 64 + 53) * 4..][..4];
        assert!(left[2] > 80 && left[0] < 10, "{left:?}");
        assert!(right[0] > 80 && right[2] < 10, "{right:?}");
        r.set_scene([], &instances).unwrap();
        assert_eq!(r.stats().mesh_uploads, 1);
        upload(&mut r, &[0, 255, 0, 255, 0, 255, 0, 255]).unwrap();
        assert_eq!(
            r.stats().mesh_uploads,
            2,
            "texture-only change invalidates upload"
        );
        assert_eq!(r.stats().error, "");
    }

    /// GPU tests need an adapter. A host without one skips, loudly, unless
    /// `DOMOWIK_REQUIRE_GPU=1` makes the absence a failure.
    fn renderer() -> Option<Renderer> {
        match Renderer::new() {
            Ok(r) => Some(r),
            Err(e) if std::env::var("DOMOWIK_REQUIRE_GPU").as_deref() != Ok("1") => {
                eprintln!("SKIPPED: {e}");
                None
            }
            Err(e) => panic!("{e}"),
        }
    }

    const IDENTITY: [f32; 16] = [
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ];

    #[test]
    fn gl_depth_maps_near_and_far_planes_to_zero_and_one() {
        // A GL projection maps z_view = -near to z_ndc = -1 and -far to +1.
        let (near, far) = (0.5f32, 10.0f32);
        let mut p = [0f32; 16];
        p[0] = 1.;
        p[5] = 1.;
        p[10] = (far + near) / (near - far);
        p[11] = -1.;
        p[14] = 2. * far * near / (near - far);
        let m = gl_to_webgpu_depth(p);
        for (z, expected) in [(-near, 0.), (-far, 1.)] {
            let clip_z = m[10] * z + m[14];
            let clip_w = m[11] * z + m[15];
            assert!((clip_z / clip_w - expected).abs() < 1e-5, "{z}");
        }
    }

    #[test]
    fn world_up_is_the_top_of_the_image() {
        let Some(mut r) = renderer() else { return };
        let (mesh, instances) = orientation_fixture();
        r.set_scene([("diagnostic:orientation", mesh.as_slice())], &instances)
            .unwrap();
        let target = r.offscreen(64, 64);
        let view = target.create_view(&Default::default());
        let index = r.render(&view, 64, 64, &IDENTITY).unwrap();
        r.wait(index).unwrap();
        let pixels = r.read_pixels(&target).unwrap();
        let (top_red, bottom_blue) = orientation_counts(&pixels, 64);
        assert!(
            top_red > 150 && bottom_blue > 150,
            "{top_red} {bottom_blue}"
        );
        assert_eq!(r.stats().error, "");
    }

    #[test]
    fn nearer_geometry_hides_farther_geometry_regardless_of_draw_order() {
        let Some(mut r) = renderer() else { return };
        let (mesh, mut instances) = orientation_fixture();
        // Both quads centred; red drawn first but farther (z = 0.8), blue
        // second and nearer (z = 0.2). Depth must make blue win.
        for (i, z) in instances.iter_mut().zip([0.8f32, 0.2]) {
            i.matrix[13] = 0.;
            i.matrix[14] = z;
        }
        r.set_scene([("diagnostic:orientation", mesh.as_slice())], &instances)
            .unwrap();
        let target = r.offscreen(32, 32);
        let view = target.create_view(&Default::default());
        let index = r.render(&view, 32, 32, &IDENTITY).unwrap();
        r.wait(index).unwrap();
        let pixels = r.read_pixels(&target).unwrap();
        let centre = &pixels[(16 * 32 + 16) * 4..][..4];
        assert!(centre[0] > 100 && centre[2] < 50, "{centre:?}");
    }

    #[test]
    fn unchanged_meshes_are_not_uploaded_again_and_unused_ones_are_released() {
        let Some(mut r) = renderer() else { return };
        let (mesh, instances) = orientation_fixture();
        r.set_scene([("diagnostic:orientation", mesh.as_slice())], &instances)
            .unwrap();
        r.set_scene([("diagnostic:orientation", mesh.as_slice())], &instances)
            .unwrap();
        assert_eq!(r.stats().mesh_uploads, 1);
        assert_eq!(
            r.stats().instance_uploads,
            1,
            "unchanged groups reuse their resident data"
        );
        // Resident meshes may be referenced without resending their bytes.
        r.set_scene([], &instances).unwrap();
        assert_eq!(r.stats().mesh_uploads, 1);
        let mut moved = instances.clone();
        moved[0].matrix[12] += 0.25;
        r.set_scene([], &moved).unwrap();
        assert_eq!(
            r.stats().mesh_uploads,
            1,
            "pose updates never resend geometry"
        );
        assert_eq!(
            r.stats().instance_uploads,
            2,
            "only the changed group is uploaded"
        );
        r.set_scene([], &[]).unwrap();
        assert!(
            r.set_scene([], &instances).is_err(),
            "released mesh is gone"
        );
    }

    #[test]
    fn invalid_scenes_are_refused_without_changing_the_current_one() {
        let Some(mut r) = renderer() else { return };
        let (mesh, instances) = orientation_fixture();
        r.set_scene([("diagnostic:orientation", mesh.as_slice())], &instances)
            .unwrap();
        let before = r.stats();
        assert!(r.set_scene([("bad", &mesh[..7])], &[]).is_err());
        let mut nan = mesh.clone();
        nan[0] = f32::NAN;
        assert!(r.set_scene([("bad", nan.as_slice())], &[]).is_err());
        let mut far = instances.clone();
        far[0].matrix[12] = f32::INFINITY;
        assert!(r.set_scene([], &far).is_err());
        let after = r.stats();
        assert_eq!(before.scene_updates, after.scene_updates);
        assert_eq!(before.mesh_uploads, after.mesh_uploads);
        let target = r.offscreen(8, 8);
        let view = target.create_view(&Default::default());
        assert!(r.render(&view, 0, 8, &IDENTITY).is_err());
        let mut camera = IDENTITY;
        camera[0] = f32::NAN;
        assert!(r.render(&view, 8, 8, &camera).is_err());
    }
}
