//! macOS 3D renderer entry points (issue 0101).
//!
//! The platform presenter (`frontend/plugin/macos/Classes`) is the only
//! caller. It is compiled into the same framework as this library, so these
//! exports are not part of the Dart-facing ABI that `ABI_VERSION` versions.
//!
//! # Ownership
//!
//! Renderer handles and target ids are opaque, monotonic and never reused; a
//! forged or destroyed one is detected (`UNKNOWN_HANDLE`), never dereferenced.
//! `domowik_r3d_attach_target` consumes one +1 retain of an `id<MTLTexture>`
//! on every path; an attached target's retain is released on detach or
//! destroy. The Metal device pointer is borrowed for the renderer's lifetime.
//! Buffers follow the bridge's token rules (`domowik_buffer_free`).
//!
//! # Threading
//!
//! GPU work on one renderer serializes on its renderer mutex.
//! `domowik_r3d_render` holds it until the GPU finishes the frame, so it, and
//! the scene-replacing calls, belong on the presenter's worker queues, never
//! the main or raster thread. `domowik_r3d_stats`, `domowik_r3d_last_error`
//! and `domowik_r3d_request_diagnostic` never take that mutex: they read a
//! snapshot published after each frame or scene change, so the main thread
//! never waits on the GPU. `domowik_r3d_load_scene` derives the scene WITHOUT
//! holding the renderer, so frames continue while a scene is built, and takes
//! it only to upload. It holds the plan's lease throughout, so
//! `domowik_plan_close` waits for it.

use super::*;
use domowik_renderer::{Instance, Renderer};
use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;

struct Target {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
}

/// GPU state, behind the renderer mutex.
struct Gpu {
    renderer: Renderer,
    targets: BTreeMap<u64, Target>,
    next_target: u64,
    /// Highest scene generation applied; older loads are rejected as stale.
    generation: u64,
    diagnostic: (u64, u64, u64),
}

/// What the main thread may read without waiting for the GPU.
#[derive(Default)]
struct Published {
    stats: serde_json::Value,
    /// The most recent refusal of any call, kept for `domowik_r3d_last_error`.
    last_error: String,
    /// Current frame health: the last frame's failure, cleared by a good one.
    health: String,
}

struct Entry {
    gpu: Mutex<Gpu>,
    published: Mutex<Published>,
    diagnostic_requested: AtomicBool,
}

impl Entry {
    fn gpu(&self) -> MutexGuard<'_, Gpu> {
        self.gpu.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn published(&self) -> MutexGuard<'_, Published> {
        self.published.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn refuse(&self, message: impl Into<String>) -> i32 {
        self.published().last_error = message.into();
        status::REJECTED
    }

    /// Republishes the counters. Called with the renderer mutex held, after
    /// any change they describe.
    fn publish(&self, gpu: &Gpu) {
        let s = gpu.renderer.stats();
        self.published().stats = serde_json::json!({
            "frames": s.frames,
            "meshUploads": s.mesh_uploads,
            "instanceUploads": s.instance_uploads,
            "sceneUpdates": s.scene_updates,
            "triangles": s.triangles,
            "drawCalls": s.draw_calls,
            "renderSubmitUs": s.submit_us,
            "driver": s.adapter,
            "gpuError": s.error,
            "diagnosticPixels": gpu.diagnostic.0,
            "topRedPixels": gpu.diagnostic.1,
            "bottomBluePixels": gpu.diagnostic.2,
            "targets": gpu.targets.len(),
            "generation": gpu.generation,
        });
    }
}

fn renderers() -> MutexGuard<'static, BTreeMap<u64, Arc<Entry>>> {
    static RENDERERS: Mutex<BTreeMap<u64, Arc<Entry>>> = Mutex::new(BTreeMap::new());
    RENDERERS.lock().unwrap_or_else(|e| e.into_inner())
}

static NEXT_RENDERER: AtomicU64 = AtomicU64::new(1);
static CREATE_ERROR: Mutex<String> = Mutex::new(String::new());

fn renderer(handle: u64) -> Option<Arc<Entry>> {
    renderers().get(&handle).cloned()
}

/// Releases a transferred Objective-C retain that will not be adopted.
///
/// # Safety
/// `object` is null or carries one +1 retain the caller gave up.
unsafe fn release_transferred(object: *mut std::ffi::c_void) {
    // SAFETY: per the function contract.
    drop(unsafe { objc2::rc::Retained::<objc2::runtime::AnyObject>::from_raw(object.cast()) });
}

/// Creates a renderer on the system GPU. On failure no handle is written and
/// `domowik_r3d_last_error(0, ..)` describes why.
///
/// # Safety
/// `out_handle` is writable.
#[no_mangle]
pub unsafe extern "C" fn domowik_r3d_create(out_handle: *mut u64) -> i32 {
    guard(|| {
        if out_handle.is_null() {
            return status::INVALID_ARGUMENT;
        }
        let renderer = match Renderer::new() {
            Ok(r) => r,
            Err(e) => {
                *CREATE_ERROR.lock().unwrap_or_else(|p| p.into_inner()) = e;
                return status::REJECTED;
            }
        };
        let handle = NEXT_RENDERER.fetch_add(1, Ordering::Relaxed);
        let entry = Arc::new(Entry {
            gpu: Mutex::new(Gpu {
                renderer,
                targets: BTreeMap::new(),
                next_target: 1,
                generation: 0,
                diagnostic: (0, 0, 0),
            }),
            published: Mutex::new(Published::default()),
            diagnostic_requested: AtomicBool::new(false),
        });
        entry.publish(&entry.gpu());
        renderers().insert(handle, entry);
        // SAFETY: checked non-null above.
        unsafe { *out_handle = handle };
        status::OK
    })
}

/// Destroys a renderer and every attached target. A call already inside the
/// renderer finishes first; later calls see `UNKNOWN_HANDLE`.
#[no_mangle]
pub extern "C" fn domowik_r3d_destroy(handle: u64) -> i32 {
    guard(|| match renderers().remove(&handle) {
        Some(_) => status::OK,
        None => status::UNKNOWN_HANDLE,
    })
}

/// The renderer's `id<MTLDevice>`, borrowed; null for an unknown handle.
#[no_mangle]
pub extern "C" fn domowik_r3d_metal_device(handle: u64) -> *mut std::ffi::c_void {
    catch_unwind(AssertUnwindSafe(|| {
        renderer(handle)
            .and_then(|r| r.gpu().renderer.metal_device())
            .unwrap_or(std::ptr::null_mut())
    }))
    .unwrap_or(std::ptr::null_mut())
}

/// Attaches a presenter-owned BGRA8 colour target.
///
/// # Safety
/// `texture` is null or a +1-retained, live 2D `MTLPixelFormatBGRA8Unorm`
/// `id<MTLTexture>` of `width` × `height` with render-target usage, created on
/// [`domowik_r3d_metal_device`]'s device. The retain is consumed on every
/// path, including every refusal. `out_target` is writable.
#[no_mangle]
pub unsafe extern "C" fn domowik_r3d_attach_target(
    handle: u64,
    texture: *mut std::ffi::c_void,
    width: u32,
    height: u32,
    out_target: *mut u64,
) -> i32 {
    guard(|| {
        if texture.is_null() {
            return status::INVALID_ARGUMENT;
        }
        let Some(r) = renderer(handle).filter(|_| !out_target.is_null()) else {
            // SAFETY: the caller transferred one retain we will not adopt.
            unsafe { release_transferred(texture) };
            return if out_target.is_null() {
                status::INVALID_ARGUMENT
            } else {
                status::UNKNOWN_HANDLE
            };
        };
        let mut gpu = r.gpu();
        // SAFETY: the caller's contract is exactly import_metal_target's,
        // which consumes the retain on every path.
        let texture = match unsafe { gpu.renderer.import_metal_target(texture, width, height) } {
            Ok(t) => t,
            Err(e) => return r.refuse(e),
        };
        let id = gpu.next_target;
        gpu.next_target += 1;
        let view = texture.create_view(&Default::default());
        gpu.targets.insert(
            id,
            Target {
                texture,
                view,
                width,
                height,
            },
        );
        r.publish(&gpu);
        // SAFETY: checked non-null above.
        unsafe { *out_target = id };
        status::OK
    })
}

/// Detaches a target and releases its texture retain.
#[no_mangle]
pub extern "C" fn domowik_r3d_detach_target(handle: u64, target: u64) -> i32 {
    guard(|| {
        let Some(r) = renderer(handle) else {
            return status::UNKNOWN_HANDLE;
        };
        let mut gpu = r.gpu();
        let removed = gpu.targets.remove(&target).is_some();
        r.publish(&gpu);
        if removed {
            status::OK
        } else {
            status::UNKNOWN_HANDLE
        }
    })
}

/// Renders one frame into `target` and returns after the GPU completed it, so
/// the presenter may publish the target to Flutter immediately. A recorded
/// `wgpu` validation or device error fails the frame rather than publishing
/// one that may be wrong.
///
/// `camera` is 16 column-major floats; `gl_depth` nonzero means GL depth
/// `[-1, 1]` (today's Dart camera), converted to WebGPU `[0, 1]` here.
///
/// # Safety
/// `camera` addresses 16 readable floats for the call.
#[no_mangle]
pub unsafe extern "C" fn domowik_r3d_render(
    handle: u64,
    target: u64,
    camera: *const f32,
    gl_depth: i32,
) -> i32 {
    guard(|| {
        if camera.is_null() {
            return status::INVALID_ARGUMENT;
        }
        // SAFETY: the caller guarantees 16 readable floats.
        let mut matrix = [0f32; 16];
        matrix.copy_from_slice(unsafe { std::slice::from_raw_parts(camera, 16) });
        if gl_depth != 0 {
            matrix = domowik_renderer::gl_to_webgpu_depth(matrix);
        }
        let Some(r) = renderer(handle) else {
            return status::UNKNOWN_HANDLE;
        };
        let mut guard = r.gpu();
        let gpu = &mut *guard;
        let Some(t) = gpu.targets.get(&target) else {
            return status::UNKNOWN_HANDLE;
        };
        let result = gpu
            .renderer
            .render(&t.view, t.width, t.height, &matrix)
            .and_then(|index| gpu.renderer.wait(index));
        if result.is_ok() && r.diagnostic_requested.swap(false, Ordering::AcqRel) {
            // The only CPU readback: an explicit diagnostic request.
            match gpu.renderer.read_pixels(&t.texture) {
                Ok(pixels) => {
                    let (top, bottom) = domowik_renderer::orientation_counts(&pixels, t.width);
                    gpu.diagnostic = (
                        domowik_renderer::non_background_pixels(&pixels),
                        top,
                        bottom,
                    );
                }
                Err(e) => {
                    r.publish(gpu);
                    return r.refuse(e);
                }
            }
        }
        r.publish(gpu);
        match result {
            Ok(()) => {
                r.published().health.clear();
                status::OK
            }
            Err(e) => {
                r.published().health = e.clone();
                r.refuse(e)
            }
        }
    })
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LoadRequest {
    level_ids: Vec<Uuid>,
    #[serde(default)]
    ghosted_ids: Vec<Uuid>,
    #[serde(default)]
    pose_only: bool,
    #[serde(default)]
    scene_token: Option<u64>,
}

/// Derives `plan`'s scene for the requested levels and makes it the
/// renderer's scene, without the geometry crossing the caller.
///
/// `generation` must exceed every generation already applied; a slower,
/// older load that finishes later is refused as stale rather than replacing
/// newer geometry. Success writes JSON `{generation, originMm, center, radius,
/// diagnostics, modelDecodes}`; camera framing stays with the caller.
///
/// # Safety
/// `req` addresses `req_len` readable bytes for the call; `out` is writable.
#[no_mangle]
pub unsafe extern "C" fn domowik_r3d_load_scene(
    handle: u64,
    plan: u64,
    req: *const u8,
    req_len: u64,
    generation: u64,
    out: *mut DomowikBuf,
) -> i32 {
    guard(|| {
        if out.is_null() || req.is_null() {
            return status::INVALID_ARGUMENT;
        }
        let Some(r) = renderer(handle) else {
            return status::UNKNOWN_HANDLE;
        };
        let Some((session, _lease)) = resolve(plan) else {
            return status::UNKNOWN_HANDLE;
        };
        // SAFETY: the caller guarantees `req_len` readable bytes.
        let bytes = unsafe { std::slice::from_raw_parts(req, req_len as usize) };
        let request: LoadRequest = match serde_json::from_slice(bytes) {
            Ok(v) => v,
            Err(e) => return r.refuse(e.to_string()),
        };
        // Derivation and model decoding run without the renderer lock.
        let (scene, models) = match asset_library::derive_scene(
            &session,
            &request.level_ids,
            request.ghosted_ids,
            request.pose_only,
            request.scene_token,
        ) {
            Ok(v) => v,
            Err(e) => return r.refuse(e),
        };
        let instances: Vec<Instance> = scene
            .nodes
            .iter()
            .map(|n| Instance {
                mesh: n.mesh.clone(),
                matrix: n.matrix,
                normal_matrix: n.normal_matrix,
                color: n.color,
                transparent: n.transparent,
            })
            .collect();
        let mut gpu = r.gpu();
        if generation <= gpu.generation {
            return r.refuse(format!("stale scene generation {generation}"));
        }
        let meshes = scene
            .meshes
            .iter()
            .filter(|_| !scene.instance_only)
            .map(|(k, v)| (k.as_str(), v.as_slice()))
            .chain(
                scene
                    .required_models
                    .iter()
                    .filter_map(|d| models.get(d).map(|m| (d.as_str(), m.vertices.as_slice()))),
            );
        let textures = scene.required_models.iter().filter_map(|d| {
            let t = models.get(d)?.texture.as_ref()?;
            Some((
                d.as_str(),
                domowik_renderer::TextureData {
                    width: t.width,
                    height: t.height,
                    layers: t.layers,
                    rgba: &t.rgba,
                    coordinates: &t.coordinates,
                },
            ))
        });
        if let Err(e) = gpu
            .renderer
            .set_scene_textured(meshes, &instances, textures)
        {
            return r.refuse(e);
        }
        gpu.generation = generation;
        r.publish(&gpu);
        drop(gpu);
        let decodes = session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .scene_model_decodes;
        let body = serde_json::json!({
            "generation": generation,
            "originMm": scene.origin_mm,
            "center": scene.center,
            "radius": scene.radius,
            "diagnostics": scene.diagnostics,
            "modelDecodes": decodes,
            "sceneToken": scene.scene_token,
            "instanceOnly": scene.instance_only,
        });
        release(body.to_string().into_bytes(), out)
    })
}

/// Replaces the scene with the asymmetric orientation fixture and requests a
/// diagnostic readback on the next frame. The caller renders it with an
/// identity camera. Diagnostic only; takes the renderer mutex, so call it from
/// the render queue.
#[no_mangle]
pub extern "C" fn domowik_r3d_orientation_probe(handle: u64) -> i32 {
    guard(|| {
        let Some(r) = renderer(handle) else {
            return status::UNKNOWN_HANDLE;
        };
        let mut gpu = r.gpu();
        let (mesh, instances) = domowik_renderer::orientation_fixture();
        if let Err(e) = gpu
            .renderer
            .set_scene([("diagnostic:orientation", mesh.as_slice())], &instances)
        {
            return r.refuse(e);
        }
        r.publish(&gpu);
        r.diagnostic_requested.store(true, Ordering::Release);
        status::OK
    })
}

/// Requests one explicit CPU readback after the next frame. Diagnostic only;
/// normal frames never read back. Never waits for the GPU.
#[no_mangle]
pub extern "C" fn domowik_r3d_request_diagnostic(handle: u64) -> i32 {
    guard(|| {
        let Some(r) = renderer(handle) else {
            return status::UNKNOWN_HANDLE;
        };
        r.diagnostic_requested.store(true, Ordering::Release);
        status::OK
    })
}

/// Counters as JSON, in the Linux renderer's key names, as of the latest
/// frame or scene change. `error` is current frame health (empty once a frame
/// succeeds again); `lastRefusal` is the most recent refusal of any call.
/// Never waits for the GPU.
///
/// # Safety
/// `out` is writable.
#[no_mangle]
pub unsafe extern "C" fn domowik_r3d_stats(handle: u64, out: *mut DomowikBuf) -> i32 {
    guard(|| {
        if out.is_null() {
            return status::INVALID_ARGUMENT;
        }
        let Some(r) = renderer(handle) else {
            return status::UNKNOWN_HANDLE;
        };
        let body = {
            let published = r.published();
            let mut stats = published.stats.clone();
            stats["error"] = serde_json::json!(published.health);
            stats["lastRefusal"] = serde_json::json!(published.last_error);
            stats.to_string()
        };
        release(body.into_bytes(), out)
    })
}

/// The renderer's last refusal, or (handle 0) why the last create failed.
/// Never waits for the GPU.
///
/// # Safety
/// `out` is writable.
#[no_mangle]
pub unsafe extern "C" fn domowik_r3d_last_error(handle: u64, out: *mut DomowikBuf) -> i32 {
    guard(|| {
        if out.is_null() {
            return status::INVALID_ARGUMENT;
        }
        let message = if handle == 0 {
            CREATE_ERROR
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone()
        } else {
            match renderer(handle) {
                Some(r) => r.published().last_error.clone(),
                None => return status::UNKNOWN_HANDLE,
            }
        };
        release(message.into_bytes(), out)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create() -> Option<u64> {
        let mut h = 0;
        // SAFETY: valid out pointer.
        match unsafe { domowik_r3d_create(&mut h) } {
            status::OK => Some(h),
            _ if std::env::var("DOMOWIK_REQUIRE_GPU").as_deref() != Ok("1") => {
                eprintln!("SKIPPED: no GPU renderer");
                None
            }
            code => panic!("create failed: {code}"),
        }
    }

    fn empty() -> DomowikBuf {
        DomowikBuf {
            data: std::ptr::null_mut(),
            len: 0,
            token: 0,
        }
    }

    #[test]
    fn forged_and_destroyed_handles_are_refused() {
        assert_eq!(domowik_r3d_destroy(u64::MAX), status::UNKNOWN_HANDLE);
        assert!(domowik_r3d_metal_device(u64::MAX).is_null());
        let Some(h) = create() else { return };
        assert!(!domowik_r3d_metal_device(h).is_null());
        assert_eq!(domowik_r3d_detach_target(h, 99), status::UNKNOWN_HANDLE);
        let camera = [0f32; 16];
        // SAFETY: 16 readable floats.
        assert_eq!(
            unsafe { domowik_r3d_render(h, 99, camera.as_ptr(), 1) },
            status::UNKNOWN_HANDLE
        );
        assert_eq!(domowik_r3d_destroy(h), status::OK);
        assert_eq!(domowik_r3d_destroy(h), status::UNKNOWN_HANDLE);
    }

    /// A transferred texture retain is released on every refusal: unknown
    /// handle, missing out pointer, oversize and mismatched dimensions.
    #[test]
    fn refused_attachments_release_the_transferred_texture() {
        use objc2_metal::{MTLDevice, MTLPixelFormat, MTLTextureDescriptor, MTLTextureUsage};
        let Some(h) = create() else { return };
        let device: &objc2::runtime::ProtocolObject<dyn MTLDevice> =
            // SAFETY: borrowed for the renderer's lifetime, which outlives this test body.
            unsafe { &*domowik_r3d_metal_device(h).cast() };
        let texture = |width: usize| {
            // SAFETY: plain descriptor construction on a live device.
            let desc = unsafe {
                MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                    MTLPixelFormat::BGRA8Unorm,
                    width,
                    1,
                    false,
                )
            };
            desc.setUsage(MTLTextureUsage::RenderTarget);
            device.newTextureWithDescriptor(&desc).expect("texture")
        };
        fn retains(t: &objc2::runtime::ProtocolObject<dyn objc2_metal::MTLTexture>) -> usize {
            // SAFETY: `retainCount` takes no arguments and returns NSUInteger.
            unsafe { objc2::msg_send![t, retainCount] }
        }
        let mut out = 0u64;
        for (width, declared, target_handle, out_ptr) in [
            (8usize, 8u32, u64::MAX, &mut out as *mut u64),
            (8, 8, h, std::ptr::null_mut()),
            (5000, 5000, h, &mut out as *mut u64),
            (8, 9, h, &mut out as *mut u64),
        ] {
            let t = texture(width);
            let before = retains(&t);
            let raw = objc2::rc::Retained::into_raw(t.clone()).cast();
            // SAFETY: one transferred retain of a live texture.
            let code =
                unsafe { domowik_r3d_attach_target(target_handle, raw, declared, 1, out_ptr) };
            assert_ne!(code, status::OK, "width {width}");
            assert_eq!(retains(&t), before, "retain released for width {width}");
        }
        assert_eq!(domowik_r3d_destroy(h), status::OK);
    }

    /// A failed frame is current health until a good frame clears it; the
    /// refusal stays available as history.
    #[test]
    fn a_good_frame_clears_frame_health_but_keeps_the_last_refusal() {
        let Some(h) = create() else { return };
        let r = renderer(h).unwrap();
        let target = {
            let mut gpu = r.gpu();
            let texture = gpu.renderer.offscreen(8, 8);
            let view = texture.create_view(&Default::default());
            gpu.targets.insert(
                1,
                Target {
                    texture,
                    view,
                    width: 8,
                    height: 8,
                },
            );
            1
        };
        let stats = || {
            let mut out = empty();
            // SAFETY: valid out pointer.
            assert_eq!(unsafe { domowik_r3d_stats(h, &mut out) }, status::OK);
            // SAFETY: the buffer is live until freed below.
            let bytes = unsafe { std::slice::from_raw_parts(out.data, out.len as usize) }.to_vec();
            domowik_buffer_free(out.token);
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()
        };
        let mut camera = [0f32; 16];
        for i in [0, 5, 10, 15] {
            camera[i] = 1.;
        }
        let mut bad = camera;
        bad[0] = f32::NAN;
        // SAFETY: 16 readable floats.
        assert_eq!(
            unsafe { domowik_r3d_render(h, target, bad.as_ptr(), 0) },
            status::REJECTED
        );
        assert_eq!(stats()["error"], "non-finite camera matrix");
        // SAFETY: 16 readable floats.
        assert_eq!(
            unsafe { domowik_r3d_render(h, target, camera.as_ptr(), 0) },
            status::OK
        );
        let s = stats();
        assert_eq!(s["error"], "", "recovered");
        assert_eq!(s["lastRefusal"], "non-finite camera matrix");
        assert_eq!(domowik_r3d_destroy(h), status::OK);
    }

    /// Destroying a renderer while a scene load is deriving neither crashes
    /// nor leaks: the load finishes on its own reference and the handle is
    /// gone afterwards.
    #[test]
    fn destroy_during_a_scene_load_is_safe() {
        let Some(h) = create() else { return };
        let mut plan = 0;
        // SAFETY: valid out pointer.
        assert_eq!(
            unsafe { domowik_plan_create_default(&mut plan) },
            status::OK
        );
        let level = {
            let p = resolve(plan).unwrap().0;
            let p = p.lock().unwrap();
            p.session().document().levels[0].id
        };
        let req = serde_json::json!({ "levelIds": [level] }).to_string();
        let loader = std::thread::spawn(move || {
            // Keeps loading until the destroy lands, so the race is exercised
            // however fast one load is.
            let mut codes = Vec::new();
            for generation in 1..=1_000_000u64 {
                let mut out = empty();
                // SAFETY: valid buffers for the call.
                let code = unsafe {
                    domowik_r3d_load_scene(
                        h,
                        plan,
                        req.as_ptr(),
                        req.len() as u64,
                        generation,
                        &mut out,
                    )
                };
                if code == status::OK {
                    domowik_buffer_free(out.token);
                }
                codes.push(code);
                if code == status::UNKNOWN_HANDLE {
                    break;
                }
            }
            codes
        });
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert_eq!(domowik_r3d_destroy(h), status::OK);
        let codes = loader.join().unwrap();
        assert!(codes
            .iter()
            .all(|c| *c == status::OK || *c == status::UNKNOWN_HANDLE));
        assert_eq!(codes.last(), Some(&status::UNKNOWN_HANDLE));
        // No global buffer-count assertion: `domowik_outstanding_buffers` is
        // process-wide and other tests allocate in parallel, which made it
        // flaky. The loader frees every buffer it is given above.
        assert_eq!(domowik_plan_close(plan), status::OK);
    }

    #[test]
    fn statistics_do_not_wait_for_the_renderer() {
        let Some(h) = create() else { return };
        let r = renderer(h).unwrap();
        let held = r.gpu();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut out = empty();
            // SAFETY: valid out pointer.
            let code = unsafe { domowik_r3d_stats(h, &mut out) };
            if code == status::OK {
                domowik_buffer_free(out.token);
            }
            let diagnostic = domowik_r3d_request_diagnostic(h);
            tx.send((code, diagnostic)).unwrap();
        });
        let result = rx.recv_timeout(std::time::Duration::from_secs(5));
        drop(held);
        assert_eq!(
            result,
            Ok((status::OK, status::OK)),
            "stats blocked on the renderer"
        );
        assert_eq!(domowik_r3d_destroy(h), status::OK);
    }

    #[test]
    fn a_plan_scene_loads_without_crossing_the_caller_and_stale_loads_are_refused() {
        let Some(h) = create() else { return };
        let mut plan = 0;
        // SAFETY: valid out pointer.
        assert_eq!(
            unsafe { domowik_plan_create_default(&mut plan) },
            status::OK
        );
        let level = {
            let p = resolve(plan).unwrap().0;
            let p = p.lock().unwrap();
            p.session().document().levels[0].id
        };
        let edit = serde_json::json!({"kind":"addWall","levelId":level,
            "start":[0.0,0.0],"end":[4000.0,0.0],"thicknessMm":200.0,"heightMm":2500.0})
        .to_string();
        let mut buf = empty();
        // SAFETY: valid buffers for the call.
        let code = unsafe { domowik_plan_edit(plan, edit.as_ptr(), edit.len() as u64, &mut buf) };
        assert_eq!(code, status::OK, "addWall");
        domowik_buffer_free(buf.token);
        let req = serde_json::json!({ "levelIds": [level] }).to_string();
        let load = |generation| {
            let mut out = empty();
            // SAFETY: valid buffers for the call.
            let code = unsafe {
                domowik_r3d_load_scene(
                    h,
                    plan,
                    req.as_ptr(),
                    req.len() as u64,
                    generation,
                    &mut out,
                )
            };
            if code == status::OK {
                domowik_buffer_free(out.token);
            }
            code
        };
        assert_eq!(load(2), status::OK);
        assert_eq!(load(1), status::REJECTED, "older generation is stale");
        assert_eq!(load(2), status::REJECTED, "repeated generation is stale");
        assert_eq!(load(3), status::OK);
        let r = renderer(h).unwrap();
        let s = r.gpu().renderer.stats();
        assert_eq!(s.scene_updates, 2);
        assert_eq!(
            s.mesh_uploads, 1,
            "an unchanged wall mesh is not re-uploaded"
        );
        assert_eq!(r.published().stats["generation"], 3);
        assert_eq!(domowik_plan_close(plan), status::OK);
        assert_eq!(load(4), status::UNKNOWN_HANDLE, "closed plan");
        assert_eq!(domowik_r3d_destroy(h), status::OK);
    }
}
