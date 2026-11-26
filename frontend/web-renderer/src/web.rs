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
    /// Creates a renderer drawing into `canvas`, preferring WebGPU and falling
    /// back to WebGL2 when the browser has no WebGPU adapter. Rejects when
    /// neither backend can supply an adapter and device.
    pub async fn create(canvas: web_sys::HtmlCanvasElement) -> Result<WebRenderer, JsError> {
        // Probe WebGPU adapter availability BEFORE touching `canvas`: calling
        // `canvas.getContext('webgpu')` locks the canvas to WebGPU even when
        // `navigator.gpu.requestAdapter()` later returns null (common on Linux
        // Chromium without Vulkan), which would otherwise block WebGL2 on the
        // same canvas.
        match Self::create_webgpu(canvas.clone()).await {
            Ok(renderer) => Ok(renderer),
            Err(webgpu_error) => Self::create_webgl(canvas).await.map_err(|webgl_error| {
                JsError::new(&format!(
                    "no 3D adapter (WebGPU: {webgpu_error}; WebGL2: {webgl_error})"
                ))
            }),
        }
    }

    async fn create_webgpu(canvas: web_sys::HtmlCanvasElement) -> Result<WebRenderer, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                ..Default::default()
            })
            .await
            .map_err(|e| format!("no WebGPU adapter: {e}"))?;
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| format!("WebGPU canvas surface: {e}"))?;
        if !adapter.is_surface_supported(&surface) {
            return Err("WebGPU adapter does not support canvas surface".to_owned());
        }
        Self::finish_create(adapter, surface, "WebGPU").await
    }

    async fn create_webgl(canvas: web_sys::HtmlCanvasElement) -> Result<WebRenderer, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::GL,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| format!("WebGL2 canvas surface: {e}"))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(|e| format!("no WebGL2 adapter: {e}"))?;
        Self::finish_create(adapter, surface, "WebGL2").await
    }

    async fn finish_create(
        adapter: wgpu::Adapter,
        surface: wgpu::Surface<'static>,
        backend_label: &str,
    ) -> Result<WebRenderer, String> {
        let info = adapter.get_info();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("domowik-3d"),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await
            .map_err(|e| format!("{backend_label} device: {e}"))?;
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
            .ok_or_else(|| "the canvas offers no 8-bit RGBA/BGRA format".to_owned())?;
        let alpha_mode = if capabilities
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::Opaque)
        {
            wgpu::CompositeAlphaMode::Opaque
        } else {
            wgpu::CompositeAlphaMode::Auto
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: 1,
            height: 1,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![],
        };
        // Browsers may withhold adapter details for privacy.
        let name = match info.name.as_str() {
            "" => format!("{backend_label} adapter"),
            named => format!("{named} ({backend_label})"),
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
