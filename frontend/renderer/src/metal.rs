//! Metal interop for the macOS presenter (issue 0101): the one place raw
//! Objective-C objects cross into `wgpu`. Kept small and isolated because it
//! is version-pinned `wgpu-hal` API with unsafe ownership rules.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLDevice, MTLTexture, MTLTextureType};

use crate::{Renderer, COLOR_FORMAT, MAX_EDGE};

impl Renderer {
    /// The renderer's `id<MTLDevice>`, borrowed: valid while the renderer
    /// lives, and the caller must not release it. `None` when the device is
    /// not a Metal device. Presenters create IOSurface-backed textures on this
    /// device so the renderer can draw into them without a copy.
    pub fn metal_device(&self) -> Option<*mut std::ffi::c_void> {
        // SAFETY: only reads the HAL device's raw handle; nothing is retained.
        let hal = unsafe { self.device().as_hal::<wgpu::hal::api::Metal>() }?;
        let device: &ProtocolObject<dyn MTLDevice> = hal.raw_device();
        Some(device as *const _ as *mut std::ffi::c_void)
    }

    /// Wraps a presenter-owned `id<MTLTexture>` as a `wgpu` colour target.
    ///
    /// # Safety
    ///
    /// `raw` must be a live 2D `MTLPixelFormatBGRA8Unorm` texture of exactly
    /// `width` × `height`, created on [`Renderer::metal_device`]'s device with
    /// render-target usage. This call takes one +1 retain from the caller on
    /// EVERY path: the returned texture releases it when dropped, and a
    /// refusal releases it before returning. The presenter must not let
    /// another consumer sample the texture's storage between [`Renderer::render`]
    /// and the matching [`Renderer::wait`].
    pub unsafe fn import_metal_target(
        &self,
        raw: *mut std::ffi::c_void,
        width: u32,
        height: u32,
    ) -> Result<wgpu::Texture, String> {
        // Adopt first, so every refusal below drops the transferred retain.
        // SAFETY: the caller transfers one retain of a live MTLTexture.
        let texture: Retained<ProtocolObject<dyn MTLTexture>> =
            unsafe { Retained::from_raw(raw.cast()) }.ok_or("null Metal texture")?;
        if width == 0 || height == 0 || width > MAX_EDGE || height > MAX_EDGE {
            return Err(format!("target {width}x{height} is outside 1..={MAX_EDGE}"));
        }
        if texture.width() != width as usize || texture.height() != height as usize {
            return Err("Metal texture size does not match the declared size".into());
        }
        // SAFETY: format, type and size are the texture's own, checked above
        // or guaranteed by the caller; wgpu-hal owns the retain from here.
        let hal = unsafe {
            wgpu::hal::metal::Device::texture_from_raw(
                texture,
                COLOR_FORMAT,
                MTLTextureType::Type2D,
                1,
                1,
                wgpu::hal::CopyExtent {
                    width,
                    height,
                    depth: 1,
                },
                None,
            )
        };
        let desc = wgpu::TextureDescriptor {
            label: Some("presenter target"),
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
        };
        // SAFETY: `hal` was created on this device's Metal backend from a
        // texture matching `desc`; no other wgpu object aliases it.
        Ok(unsafe {
            self.device()
                .create_texture_from_hal::<wgpu::hal::api::Metal>(
                    hal,
                    &desc,
                    wgpu::TextureUses::UNINITIALIZED,
                )
        })
    }
}
