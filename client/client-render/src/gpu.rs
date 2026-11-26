//! The `wgpu` side: surface, device, one pipeline for triangles. Vertices
//! arrive in screen pixels; the shader turns them into clip space. A vertex
//! with `0 <= uv.x < 2` takes its alpha from the glyph atlas (an `R8`
//! texture filled by `client-ui`); `uv.x >= 2` samples the sprite sheet (an
//! RGBA texture of pixel art, nearest filter) at `uv - (2, 0)`, tinted by the
//! vertex color. World, sprites, panels and text go in one draw call.
//!
//! Phase 10 (light): a vertex with `lit = 1` (the world) is multiplied by
//! the light at its pixel: the ambient color of the time of day plus up to
//! `MAX_LIGHTS` point lights (tiles, glowing sprites, the player's own),
//! each fading over an ellipse that follows the isometric ground. The
//! interface and glowing sprites (`lit = 0`) are drawn as they are.
//!
//! (fog): a vertex with `lit = 2` (the ground) is also darkened
//! where the player cannot see, read per pixel from a small `R8` mask of
//! one texel per tile with a linear filter, so the edge of sight is a soft
//! gradient instead of tile steps.

use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct Vertex {
    pub(crate) pos: [f32; 2],
    pub(crate) uv: [f32; 2],
    pub(crate) color: [f32; 4],
    /// 1: lit by the world's light; 0: drawn as it is.
    pub(crate) lit: f32,
}

/// Point lights the shader takes at once (nearest to the screen center).
pub(crate) const MAX_LIGHTS: usize = 32;

/// The uniform block: screen size, light count, ambient, lights.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct Uniforms {
    pub(crate) size: [f32; 2],
    pub(crate) count: f32,
    pub(crate) _pad: f32,
    pub(crate) ambient: [f32; 4],
    /// Screen x, y, radius in pixels (x), unused.
    pub(crate) lights: [[f32; 4]; MAX_LIGHTS],
    /// Color and strength.
    pub(crate) colors: [[f32; 4]; MAX_LIGHTS],
    /// Screen to world: camera origin x, y and half a tile w, h (zoomed).
    pub(crate) fog_map: [f32; 4],
    /// Mask: first tile x, y, side in tiles, 1 when there is a mask.
    pub(crate) fog_box: [f32; 4],
    /// What unseen ground is multiplied by.
    pub(crate) fog_shade: [f32; 4],
}

/// No texture: the vertex color as is.
pub(crate) const FLAT: [f32; 2] = [-1.0, -1.0];

const SHADER: &str = r#"
struct Screen {
    size: vec2<f32>,
    count: f32,
    pad: f32,
    ambient: vec4<f32>,
    lights: array<vec4<f32>, 32>,
    colors: array<vec4<f32>, 32>,
    fog_map: vec4<f32>,
    fog_box: vec4<f32>,
    fog_shade: vec4<f32>,
};
@group(0) @binding(0) var<uniform> screen: Screen;

@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;
@group(0) @binding(3) var sheet: texture_2d<f32>;
@group(0) @binding(4) var sheet_sampler: sampler;
@group(0) @binding(5) var fog: texture_2d<f32>;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) lit: f32,
};

@vertex
fn vs_main(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>, @location(3) lit: f32) -> Out {
    var o: Out;
    let ndc = vec2<f32>(pos.x / screen.size.x * 2.0 - 1.0, 1.0 - pos.y / screen.size.y * 2.0);
    o.pos = vec4<f32>(ndc, 0.0, 1.0);
    o.uv = uv;
    o.color = color;
    o.lit = lit;
    return o;
}

// Ambient plus every point light at this pixel, at most full white.
fn light_at(p: vec2<f32>) -> vec3<f32> {
    var l = screen.ambient.rgb;
    let n = i32(screen.count);
    for (var i = 0; i < n; i = i + 1) {
        let d = p - screen.lights[i].xy;
        // The ground is isometric: a circle on it is twice as wide as tall.
        let r = length(vec2<f32>(d.x, d.y * 2.0)) / max(screen.lights[i].z, 1.0);
        let f = clamp(1.0 - r, 0.0, 1.0);
        l = l + screen.colors[i].rgb * (f * f * screen.colors[i].a);
    }
    return min(l, vec3<f32>(1.0, 1.0, 1.0));
}

// How much of this pixel's ground is in sight, 0..1, smoothed between
// tile centers (the mask has one texel per tile).
fn sight_at(p: vec2<f32>) -> f32 {
    if (screen.fog_box.w < 0.5) {
        return 1.0;
    }
    let a = (p.x + screen.fog_map.x) / screen.fog_map.z;
    let b = (p.y + screen.fog_map.y) / screen.fog_map.w;
    let tile = vec2<f32>((a + b) * 0.5, (b - a) * 0.5);
    let uv = (tile - screen.fog_box.xy + vec2<f32>(0.5, 0.5)) / screen.fog_box.z;
    let s = textureSampleLevel(fog, atlas_sampler, uv, 0.0).r;
    return smoothstep(0.0, 1.0, s);
}

@fragment
fn fs_main(i: Out) -> @location(0) vec4<f32> {
    var c: vec4<f32>;
    if (i.uv.x < 0.0) {
        c = i.color;
    } else if (i.uv.x >= 2.0) {
        c = textureSampleLevel(sheet, sheet_sampler, i.uv - vec2<f32>(2.0, 0.0), 0.0) * i.color;
    } else {
        let coverage = textureSampleLevel(atlas, atlas_sampler, i.uv, 0.0).r;
        c = vec4<f32>(i.color.rgb, i.color.a * coverage);
    }
    if (i.lit > 1.5) {
        c = vec4<f32>(c.rgb * mix(screen.fog_shade.rgb, vec3<f32>(1.0, 1.0, 1.0), sight_at(i.pos.xy)), c.a);
    }
    if (i.lit > 0.5) {
        c = vec4<f32>(c.rgb * light_at(i.pos.xy), c.a);
    }
    return c;
}
"#;

const CLEAR: wgpu::Color = wgpu::Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};

#[derive(Debug)]
pub enum RenderError {
    Surface(wgpu::CreateSurfaceError),
    Adapter(wgpu::RequestAdapterError),
    Device(wgpu::RequestDeviceError),
    /// The adapter cannot present to this surface.
    Unsupported,
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::Surface(e) => write!(f, "cannot create the surface: {e}"),
            RenderError::Adapter(e) => write!(f, "no graphics adapter: {e}"),
            RenderError::Device(e) => write!(f, "cannot open the graphics device: {e}"),
            RenderError::Unsupported => write!(f, "the graphics adapter cannot draw on this window"),
        }
    }
}

impl std::error::Error for RenderError {}

pub(crate) struct Gpu {
    /// Set by the device-lost callback (driver reset, GPU removed).
    lost: std::sync::Arc<std::sync::atomic::AtomicBool>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    screen: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    atlas: wgpu::Texture,
    sheet: wgpu::Texture,
    /// One texel per tile, 255 in sight.
    fog: wgpu::Texture,
    nearest: wgpu::Sampler,
    bind_group: wgpu::BindGroup,
    vertices: wgpu::Buffer,
    /// The surface can be copied out (`COPY_SRC`), so screenshots work.
    capturable: bool,
    /// A screenshot was asked for: the next presented frame is copied.
    want_shot: bool,
    shot: Option<Screenshot>,
}

/// One pixel of a surface `format` as opaque RGBA bytes; `None` for a
/// format screenshots do not handle.
fn pixel(format: wgpu::TextureFormat, px: &[u8]) -> Option<[u8; 4]> {
    use wgpu::TextureFormat as F;
    let unit = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    match format {
        F::Bgra8Unorm | F::Bgra8UnormSrgb => Some([px[2], px[1], px[0], 255]),
        F::Rgba8Unorm | F::Rgba8UnormSrgb => Some([px[0], px[1], px[2], 255]),
        F::Rgb10a2Unorm => {
            let v = u32::from_le_bytes([px[0], px[1], px[2], px[3]]);
            let c = |s: u32| ((((v >> s) & 0x3ff) * 255 + 511) / 1023) as u8;
            Some([c(0), c(10), c(20), 255])
        }
        F::Rgba16Float => {
            let h = |i: usize| half_to_f32(u16::from_le_bytes([px[i], px[i + 1]]));
            Some([unit(h(0)), unit(h(2)), unit(h(4)), 255])
        }
        _ => None,
    }
}

/// An IEEE half float to `f32` (screenshots of 16-bit surfaces only).
fn half_to_f32(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = i32::from((bits >> 10) & 0x1f);
    let frac = f32::from(bits & 0x3ff);
    match exp {
        0 => sign * frac * 2f32.powi(-24),
        31 => sign * f32::INFINITY,
        _ => sign * (1.0 + frac / 1024.0) * 2f32.powi(exp - 15),
    }
}

/// A frame copy waiting to be read: the buffer and its layout.
struct Pending {
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    /// Bytes per row in the buffer (padded to the copy alignment).
    row: u32,
    /// Bytes per pixel of `format`.
    texel: u32,
    format: wgpu::TextureFormat,
}

/// One presented frame, RGBA, row by row from the top.
pub struct Screenshot {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Gpu {
    pub(crate) async fn new(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        size: (u32, u32),
    ) -> Result<Gpu, RenderError> {
        let desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        // A browser may list WebGPU and still have no adapter for it: then
        // WebGL2, decided before the canvas is bound to either.
        #[cfg(target_arch = "wasm32")]
        let instance = wgpu::util::new_instance_with_webgpu_detection(desc).await;
        #[cfg(not(target_arch = "wasm32"))]
        let instance = wgpu::Instance::new(desc);
        let surface = instance.create_surface(target).map_err(RenderError::Surface)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(RenderError::Adapter)?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("client"),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await
            .map_err(RenderError::Device)?;
        // A wgpu validation error must not abort the client: log it and go on.
        device.on_uncaptured_error(std::sync::Arc::new(|e| tracing::error!("wgpu: {e:?}")));
        let lost = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = lost.clone();
        device.set_device_lost_callback(move |reason, message| {
            // Dropping the device on purpose reports `Destroyed`: not a loss.
            if reason != wgpu::DeviceLostReason::Destroyed {
                tracing::error!("graphics device lost: {message}");
                flag.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        });
        let mut config = surface
            .get_default_config(&adapter, size.0.max(1), size.1.max(1))
            .ok_or(RenderError::Unsupported)?;
        config.present_mode = wgpu::PresentMode::AutoVsync;
        // Colors are given in sRGB (as picked by eye); draw them unchanged.
        let caps = surface.get_capabilities(&adapter);
        if let Some(f) = caps.formats.iter().find(|f| !f.is_srgb()) {
            config.format = *f;
        }
        // Screenshots copy the presented frame: only where the surface allows it.
        let capturable = caps.usages.contains(wgpu::TextureUsages::COPY_SRC);
        if capturable {
            config.usage |= wgpu::TextureUsages::COPY_SRC;
        }
        surface.configure(&device, &config);

        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("flat"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let screen = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("screen"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("screen"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX.union(wgpu::ShaderStages::FRAGMENT),
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atlas"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        // Pixel art stays sharp at any zoom.
        let nearest = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sheet"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let atlas = atlas_texture(&device, 1);
        let sheet = sheet_texture(&device, 1, 1);
        let fog = atlas_texture(&device, 1);
        let bind_group = bind_group(
            &device,
            &layout,
            &screen,
            [&atlas, &sheet, &fog],
            [&sampler, &nearest],
        );
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("flat"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let attributes =
            wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("flat"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let vertices = vertex_buffer(&device, 64 * 1024);
        Ok(Gpu {
            lost,
            surface,
            device,
            queue,
            config,
            pipeline,
            screen,
            layout,
            sampler,
            atlas,
            sheet,
            fog,
            nearest,
            bind_group,
            vertices,
            capturable,
            want_shot: false,
            shot: None,
        })
    }

    /// Asks for a copy of the next presented frame. False when this surface
    /// cannot be copied (the request is dropped).
    pub(crate) fn request_screenshot(&mut self) -> bool {
        self.want_shot = self.capturable;
        self.capturable
    }

    /// The frame copied since the last call, if any.
    pub(crate) fn take_screenshot(&mut self) -> Option<Screenshot> {
        self.shot.take()
    }

    /// Copies `texture` (the frame just drawn) into a buffer: its own size,
    /// which may differ from the surface configuration for a frame.
    fn copy_frame(&mut self, encoder: &mut wgpu::CommandEncoder, texture: &wgpu::Texture) -> Pending {
        let (w, h) = (texture.width(), texture.height());
        let texel = texture.format().block_copy_size(None).unwrap_or(4);
        let row =
            (w * texel).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("screenshot"),
            size: u64::from(row) * u64::from(h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        Pending {
            buffer,
            width: w,
            height: h,
            row,
            texel,
            format: texture.format(),
        }
    }

    /// Reads the copied frame back as RGBA (dropping the row padding).
    fn read_frame(&mut self, pending: &Pending) {
        let (buffer, w, h, row) = (&pending.buffer, pending.width, pending.height, pending.row);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer.map_async(wgpu::MapMode::Read, .., move |r| {
            let _ = tx.send(r);
        });
        if let Err(e) = self.device.poll(wgpu::PollType::wait_indefinitely()) {
            tracing::warn!("screenshot: {e}");
            return;
        }
        if !matches!(rx.recv(), Ok(Ok(()))) {
            tracing::warn!("screenshot: the frame could not be read");
            return;
        }
        let (texel, format) = (pending.texel as usize, pending.format);
        if pixel(format, &[0; 8][..texel]).is_none() {
            tracing::warn!(?format, "screenshot: this surface format is not supported");
            buffer.unmap();
            return;
        }
        let data = match buffer.get_mapped_range(..) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("screenshot: {e}");
                return;
            }
        };
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h as usize {
            let line = &data[y * row as usize..y * row as usize + w as usize * texel];
            for px in line.chunks_exact(texel) {
                rgba.extend_from_slice(&pixel(format, px).unwrap_or([0, 0, 0, 255]));
            }
        }
        drop(data);
        buffer.unmap();
        self.shot = Some(Screenshot {
            width: w,
            height: h,
            rgba,
        });
    }

    /// Replaces the glyph atlas (`size` × `size` bytes of coverage).
    pub(crate) fn upload_atlas(&mut self, size: u32, pixels: &[u8]) {
        if size == 0 || pixels.len() != (size as usize) * (size as usize) {
            return;
        }
        if size > self.device.limits().max_texture_dimension_2d {
            tracing::error!("glyph atlas {size}px is over the texture limit");
            return;
        }
        if self.atlas.width() != size {
            self.atlas = atlas_texture(&self.device, size);
            self.rebind();
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.atlas,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size),
                rows_per_image: Some(size),
            },
            wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
        );
    }

    fn rebind(&mut self) {
        self.bind_group = bind_group(
            &self.device,
            &self.layout,
            &self.screen,
            [&self.atlas, &self.sheet, &self.fog],
            [&self.sampler, &self.nearest],
        );
    }

    /// Replaces the sight mask (`side` × `side` bytes, one per tile).
    pub(crate) fn upload_fog(&mut self, side: u32, pixels: &[u8]) {
        if side == 0 || pixels.len() != (side as usize) * (side as usize) {
            return;
        }
        if side > self.device.limits().max_texture_dimension_2d {
            return;
        }
        if self.fog.width() != side {
            self.fog = atlas_texture(&self.device, side);
            self.rebind();
        }
        // Rows of an R8 texture copy need no padding through `write_texture`.
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.fog,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(side),
                rows_per_image: Some(side),
            },
            wgpu::Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Replaces the sprite sheet (`width` × `height` RGBA pixels).
    pub(crate) fn upload_sheet(&mut self, width: u32, height: u32, pixels: &[u8]) {
        if width == 0 || height == 0 || pixels.len() != (width as usize) * (height as usize) * 4 {
            return;
        }
        let limit = self.device.limits().max_texture_dimension_2d;
        if width > limit || height > limit {
            tracing::error!("sprite sheet {width}x{height} is over the texture limit {limit}");
            return;
        }
        if self.sheet.width() != width || self.sheet.height() != height {
            self.sheet = sheet_texture(&self.device, width, height);
            self.rebind();
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.sheet,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    /// The device died; the owner must build a new `Gpu` and upload again.
    pub(crate) fn is_lost(&self) -> bool {
        self.lost.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub(crate) fn resize(&mut self, size: (u32, u32)) {
        if size.0 == 0 || size.1 == 0 {
            return;
        }
        self.config.width = size.0;
        self.config.height = size.1;
        self.surface.configure(&self.device, &self.config);
    }

    pub(crate) fn present(&mut self, verts: &[Vertex], light: &Uniforms) {
        // The acquired texture must be presented (dropped) before `configure`,
        // so a suboptimal surface is reconfigured after this frame.
        let mut reconfigure = false;
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => {
                reconfigure = true;
                f
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                tracing::warn!("surface texture refused (validation); frame skipped");
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
        };
        let bytes: &[u8] = bytemuck::cast_slice(verts);
        if bytes.len() as u64 > self.vertices.size() {
            self.vertices = vertex_buffer(&self.device, (bytes.len() as u64).next_power_of_two());
        }
        self.queue.write_buffer(&self.vertices, 0, bytes);
        let mut u = *light;
        u.size = [self.config.width as f32, self.config.height as f32];
        self.queue.write_buffer(&self.screen, 0, bytemuck::bytes_of(&u));

        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("world"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(CLEAR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if !verts.is_empty() {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, self.vertices.slice(..bytes.len() as u64));
                pass.draw(0..verts.len() as u32, 0..1);
            }
        }
        let copy = if std::mem::take(&mut self.want_shot) {
            Some(self.copy_frame(&mut encoder, &frame.texture))
        } else {
            None
        };
        self.queue.submit([encoder.finish()]);
        if let Some(pending) = copy {
            self.read_frame(&pending);
        }
        self.queue.present(frame);
        if reconfigure {
            self.surface.configure(&self.device, &self.config);
        }
    }
}

fn atlas_texture(device: &wgpu::Device, size: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("atlas"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn sheet_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("sheet"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

/// Textures: glyph atlas, sprite sheet; samplers: linear, nearest.
fn bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    screen: &wgpu::Buffer,
    [atlas, sheet, fog]: [&wgpu::Texture; 3],
    [sampler, nearest]: [&wgpu::Sampler; 2],
) -> wgpu::BindGroup {
    let view = atlas.create_view(&wgpu::TextureViewDescriptor::default());
    let sheet_view = sheet.create_view(&wgpu::TextureViewDescriptor::default());
    let fog_view = fog.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("screen"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: screen.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&sheet_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(nearest),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(&fog_view),
            },
        ],
    })
}

fn vertex_buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("vertices"),
        size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

#[cfg(test)]
mod tests {
    /// The shader compiles only when the window opens: check it here.
    #[test]
    fn the_shader_is_valid_wgsl() {
        use wgpu::naga;
        let module = naga::front::wgsl::parse_str(super::SHADER).expect("WGSL parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("WGSL validates");
    }
}
