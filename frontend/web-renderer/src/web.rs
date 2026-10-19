use domowik_renderer::{gl_to_webgpu_depth, Instance, Renderer, MAX_EDGE};
use wasm_bindgen::prelude::*;

/// One canvas, one device, one shared renderer.
#[wasm_bindgen]
pub struct WebRenderer {
    renderer: Renderer,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    configured: bool,
}

#[wasm_bindgen]
impl WebRenderer {
    /// Creates a renderer drawing into `canvas`. Rejects when the browser has
    /// no WebGPU, no adapter, or refuses a device.
    pub async fn create(canvas: web_sys::HtmlCanvasElement) -> Result<WebRenderer, JsError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| JsError::new(&format!("WebGPU canvas surface: {e}")))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(|e| JsError::new(&format!("no WebGPU adapter: {e}")))?;
        let info = adapter.get_info();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("domowik-3d"),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await
            .map_err(|e| JsError::new(&format!("WebGPU device: {e}")))?;
        // The canvas's preferred format, made non-sRGB to match the shader's
        // output values on every presenter.
        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|f| {
                matches!(
                    f,
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
                )
            })
            .ok_or_else(|| JsError::new("the canvas offers no 8-bit RGBA/BGRA format"))?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: 1,
            height: 1,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
        };
        // Browsers may withhold adapter details for privacy.
        let name = match info.name.as_str() {
            "" => "WebGPU adapter".to_owned(),
            named => format!("{named} (WebGPU)"),
        };
        Ok(WebRenderer {
            renderer: Renderer::with_device(device, queue, name, format),
            surface,
            config,
            configured: false,
        })
    }

    /// Replaces the scene with a `domowik-scene-wire` frame and returns its
    /// summary JSON (`originMm`, `center`, `radius`, `diagnostics`, extras).
    #[wasm_bindgen(js_name = setScene)]
    pub fn set_scene(&mut self, frame: &[u8]) -> Result<String, JsError> {
        let frame = domowik_scene_wire::decode(frame).map_err(|e| JsError::new(&e))?;
        let instances: Vec<Instance> = crate::instances(&frame)
            .into_iter()
            .map(|p| Instance {
                mesh: p.mesh,
                matrix: p.matrix,
                normal_matrix: p.normal_matrix,
                color: p.color,
                transparent: p.transparent,
            })
            .collect();
        self.renderer
            .set_scene_textured(
                frame
                    .meshes
                    .iter()
                    .filter(|_| {
                        frame.header.extra.get("instanceOnly")
                            != Some(&serde_json::Value::Bool(true))
                    })
                    .map(|(k, v)| (k.as_str(), v.as_slice())),
                &instances,
                frame.textures.iter().map(|(key, t)| {
                    (
                        key.as_str(),
                        domowik_renderer::TextureData {
                            width: t.width,
                            height: t.height,
                            layers: t.layers,
                            rgba: &t.rgba,
                            coordinates: &t.coordinates,
                        },
                    )
                }),
            )
            .map_err(|e| JsError::new(&e))?;
        Ok(crate::scene_summary(&frame))
    }

    /// Draws one frame of `width` × `height` device pixels with `camera`, a
    /// column-major GL-depth clip-from-world matrix, and presents it.
    pub fn render(&mut self, camera: &[f32], width: u32, height: u32) -> Result<(), JsError> {
        let camera: [f32; 16] = camera
            .try_into()
            .map_err(|_| JsError::new("the camera must be 16 floats"))?;
        let (width, height) = (width.clamp(1, MAX_EDGE), height.clamp(1, MAX_EDGE));
        if !self.configured || self.config.width != width || self.config.height != height {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(self.renderer.device(), &self.config);
            self.configured = true;
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            other => {
                // Lost or outdated: reconfigure for the next frame.
                self.configured = false;
                return Err(JsError::new(&format!("no canvas frame: {other:?}")));
            }
        };
        let view = frame.texture.create_view(&Default::default());
        self.renderer
            .render(&view, width, height, &gl_to_webgpu_depth(camera))
            .map_err(|e| JsError::new(&e))?;
        self.renderer.queue().present(frame);
        Ok(())
    }

    /// The renderer's counters and last device error, as JSON.
    pub fn stats(&self) -> String {
        let s = self.renderer.stats();
        serde_json::json!({
            "frames": s.frames,
            "meshUploads": s.mesh_uploads,
            "instanceUploads": s.instance_uploads,
            "sceneUpdates": s.scene_updates,
            "triangles": s.triangles,
            "drawCalls": s.draw_calls,
            "adapter": s.adapter,
            "error": s.error,
            "format": format!("{:?}", self.config.format),
        })
        .to_string()
    }
}
