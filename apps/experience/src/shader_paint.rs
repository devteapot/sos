//! Bounded execution bridge for revision WGSL paint assets.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use gpui::RenderImage;

use crate::assets;

const MAX_SHADER_EDGE: u32 = 1024;

type ShaderImageKey = (String, u32, u32);
type ShaderImageCache = Mutex<HashMap<ShaderImageKey, Arc<RenderImage>>>;

static IMAGES: OnceLock<ShaderImageCache> = OnceLock::new();
static GPU: OnceLock<Result<Mutex<ShaderGpu>, String>> = OnceLock::new();

pub(crate) fn render(asset: &str, width: u32, height: u32) -> Option<Arc<RenderImage>> {
    let width = width.clamp(1, MAX_SHADER_EDGE);
    let height = height.clamp(1, MAX_SHADER_EDGE);
    let key = (asset.to_owned(), width, height);
    let cache = IMAGES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(image) = cache.lock().ok()?.get(&key).cloned() {
        return Some(image);
    }
    let source = assets::bytes(asset)?;
    let gpu = GPU
        .get_or_init(ShaderGpu::new)
        .as_ref()
        .map_err(|error| {
            log::warn!("shader_gpu_unavailable error={error}");
        })
        .ok()?;
    let pixels = match gpu.lock().ok()?.execute(asset, &source, width, height) {
        Ok(pixels) => pixels,
        Err(error) => {
            log::warn!("shader_paint_failed asset={asset} error={error}");
            return None;
        }
    };
    let buffer = image::RgbaImage::from_raw(width, height, pixels)?;
    let image = Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]));
    cache.lock().ok()?.insert(key, image.clone());
    log::info!("shader_paint_ready asset={asset} size={width}x{height}");
    Some(image)
}

struct ShaderGpu {
    _instance: wgpu::Instance,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl ShaderGpu {
    fn new() -> Result<Mutex<Self>, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN | wgpu::Backends::GL,
            flags: wgpu::InstanceFlags::default(),
            backend_options: wgpu::BackendOptions::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            display: None,
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|error| format!("adapter: {error}"))?;
        let (device, queue) = pollster::block_on(
            adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("sos_shader_paint_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults()
                    .using_resolution(adapter.limits())
                    .using_alignment(adapter.limits()),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
            }),
        )
        .map_err(|error| format!("device: {error}"))?;
        Ok(Mutex::new(Self {
            _instance: instance,
            device,
            queue,
        }))
    }

    fn execute(
        &mut self,
        label: &str,
        source: &[u8],
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, String> {
        let source = std::str::from_utf8(source).map_err(|error| error.to_string())?;
        let error_scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("sos_shader_paint_layout"),
                bind_group_layouts: &[],
                immediate_size: 0,
            });
        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });
        if let Some(error) = pollster::block_on(error_scope.pop()) {
            return Err(format!("pipeline validation: {error}"));
        }

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sos_shader_paint_target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let unpadded_row = width * 4;
        let padded_row = unpadded_row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sos_shader_paint_readback"),
            size: u64::from(padded_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("sos_shader_paint_encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sos_shader_paint_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            pass.set_pipeline(&pipeline);
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let submission = self.queue.submit(Some(encoder.finish()));
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result.map_err(|error| error.to_string()));
            });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(std::time::Duration::from_secs(2)),
            })
            .map_err(|error| format!("poll: {error:?}"))?;
        receiver
            .recv_timeout(std::time::Duration::from_secs(2))
            .map_err(|error| format!("map timeout: {error}"))??;
        let mapped = readback.slice(..).get_mapped_range();
        let mut pixels = Vec::with_capacity((unpadded_row * height) as usize);
        for row in mapped
            .chunks_exact(padded_row as usize)
            .take(height as usize)
        {
            pixels.extend_from_slice(&row[..unpadded_row as usize]);
        }
        drop(mapped);
        readback.unmap();
        Ok(pixels)
    }
}

#[cfg(test)]
mod tests {
    use runtime_luau::RevisionAsset;

    #[test]
    fn executes_validated_fragment_shader_into_rgba_pixels() {
        let source = br#"
            @vertex fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
                let x = f32((i << 1u) & 2u);
                let y = f32(i & 2u);
                return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
            }
            @fragment fn fs_main() -> @location(0) vec4<f32> {
                return vec4<f32>(1.0, 0.25, 0.0, 1.0);
            }
        "#;
        crate::assets::install(&[RevisionAsset {
            id: "test".into(),
            path: "sos/revisions/test.wgsl".into(),
            kind: "shader".into(),
            bytes: source.to_vec(),
            sha256: "test".into(),
        }]);
        let image = super::render("sos/revisions/test.wgsl", 8, 8)
            .expect("headless shader paint must render");
        let pixels = image.as_bytes(0).expect("one frame");
        assert_eq!(pixels.len(), 8 * 8 * 4);
        // The render target is sRGB, so a linear 0.25 green channel encodes
        // near 137 while exact 0/1 channels remain exact.
        assert!(pixels.chunks_exact(4).all(|pixel| {
            pixel[0] >= 250 && (132..=142).contains(&pixel[1]) && pixel[2] <= 2 && pixel[3] == 255
        }));
    }
}
