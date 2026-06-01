//! Headless block gallery renderer.
//!
//! Renders every block-like `VoxelType` as a single textured cube, captured
//! from all six axis-aligned sides (+X, -X, +Y, -Y, +Z, -Z), and writes the
//! results as PNG files under `block_renders/<block_name>/<face>.png`.
//!
//! This is a standalone diagnostic / visual-demo tool. It does NOT open a
//! window: it renders to an offscreen texture using wgpu, copies the result
//! back to the CPU, and saves it with the `image` crate. It reuses the game's
//! real procedural texture atlas (`texture::generate_texture_data`) and the
//! real per-face texture mapping (`voxel::VoxelType::texture_index`) so the
//! gallery matches what the blocks look like in game.
//!
//! Run with:  cargo run --bin block_gallery   (add --release for speed)

// Pull in only the modules needed to describe blocks + their textures. The
// dependency closure of these three modules is self-contained (it does not
// reach into chunk/renderer/etc.).
#[path = "../voxel.rs"]
mod voxel;
#[path = "../block.rs"]
mod block;
#[path = "../texture.rs"]
mod texture;

use cgmath::{Matrix4, Point3, Vector3, Deg, EuclideanSpace, InnerSpace, perspective};
use voxel::{VoxelType, VOXEL_TYPE_COUNT, FaceDir};
use wgpu::util::DeviceExt;

/// Output image dimensions (square).
const IMG_SIZE: u32 = 512;

/// One vertex of the cube mesh sent to the GPU.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    tex_coords: [f32; 2],
    tex_index: u32,
    _pad: u32,
}

/// Camera/transform uniform.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    light_dir: [f32; 4],
}

/// OpenGL (cgmath) -> wgpu clip-space z remap ([-1,1] -> [0,1]).
#[rustfmt::skip]
const OPENGL_TO_WGPU: Matrix4<f32> = Matrix4::new(
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 0.5, 0.0,
    0.0, 0.0, 0.5, 1.0,
);

/// The six capture directions, named for the file they produce. Each entry is
/// (file_label, eye_position_direction). The cube is centered at the origin
/// with extent [-0.5, 0.5].
const VIEWS: [(&str, [f32; 3]); 6] = [
    ("pos_x", [1.0, 0.0, 0.0]),
    ("neg_x", [-1.0, 0.0, 0.0]),
    ("pos_y", [0.0, 1.0, 0.0]),
    ("neg_y", [0.0, -1.0, 0.0]),
    ("pos_z", [0.0, 0.0, 1.0]),
    ("neg_z", [0.0, 0.0, -1.0]),
];

/// Map a face's outward normal to the `FaceDir` used for texture selection.
/// +Y = Top, -Y = Bottom, everything else = Side.
fn face_dir_for_normal(normal: [f32; 3]) -> FaceDir {
    if normal[1] > 0.5 {
        FaceDir::Top
    } else if normal[1] < -0.5 {
        FaceDir::Bottom
    } else {
        FaceDir::Side
    }
}

/// Human-readable, filesystem-safe name for a voxel type.
fn block_name(vt: VoxelType) -> String {
    format!("{:?}", vt).to_lowercase()
}

/// Whether we should render this voxel type as a block in the gallery.
/// We skip Air (nothing to draw) and the tool items (not placeable blocks).
fn is_renderable_block(vt: VoxelType) -> bool {
    vt != VoxelType::Air && !vt.is_tool()
}

/// Build the 24-vertex / 36-index cube for one block type, choosing the proper
/// texture layer per face from the game's mapping.
fn build_cube(vt: VoxelType) -> (Vec<Vertex>, Vec<u32>) {
    // (normal, 4 corner positions in CCW order when viewed from outside).
    // Cube spans [-0.5, 0.5] on every axis.
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        // +X
        ([1.0, 0.0, 0.0], [
            [0.5, -0.5, 0.5], [0.5, -0.5, -0.5], [0.5, 0.5, -0.5], [0.5, 0.5, 0.5],
        ]),
        // -X
        ([-1.0, 0.0, 0.0], [
            [-0.5, -0.5, -0.5], [-0.5, -0.5, 0.5], [-0.5, 0.5, 0.5], [-0.5, 0.5, -0.5],
        ]),
        // +Y (top)
        ([0.0, 1.0, 0.0], [
            [-0.5, 0.5, 0.5], [0.5, 0.5, 0.5], [0.5, 0.5, -0.5], [-0.5, 0.5, -0.5],
        ]),
        // -Y (bottom)
        ([0.0, -1.0, 0.0], [
            [-0.5, -0.5, -0.5], [0.5, -0.5, -0.5], [0.5, -0.5, 0.5], [-0.5, -0.5, 0.5],
        ]),
        // +Z
        ([0.0, 0.0, 1.0], [
            [-0.5, -0.5, 0.5], [0.5, -0.5, 0.5], [0.5, 0.5, 0.5], [-0.5, 0.5, 0.5],
        ]),
        // -Z
        ([0.0, 0.0, -1.0], [
            [0.5, -0.5, -0.5], [-0.5, -0.5, -0.5], [-0.5, 0.5, -0.5], [0.5, 0.5, -0.5],
        ]),
    ];

    // Standard quad UVs (0,0 top-left .. 1,1 bottom-right).
    let uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];

    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);

    for (normal, corners) in faces.iter() {
        let face_dir = face_dir_for_normal(*normal);
        let tex_index = vt.texture_index(face_dir);
        let base = vertices.len() as u32;
        for (i, pos) in corners.iter().enumerate() {
            vertices.push(Vertex {
                position: *pos,
                normal: *normal,
                tex_coords: uvs[i],
                tex_index,
                _pad: 0,
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    (vertices, indices)
}

const SHADER_SRC: &str = r#"
struct Uniforms {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var atlas: texture_2d_array<f32>;
@group(1) @binding(1) var atlas_sampler: sampler;

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) layer: u32,
};

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) layer: u32,
) -> VsOut {
    var out: VsOut;
    out.clip_position = u.view_proj * vec4<f32>(position, 1.0);
    out.normal = normal;
    out.uv = uv;
    out.layer = layer;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let tex = textureSample(atlas, atlas_sampler, in.uv, in.layer);
    // Simple directional + ambient lighting so the cube reads as 3D.
    let n = normalize(in.normal);
    let diffuse = max(dot(n, normalize(-u.light_dir.xyz)), 0.0);
    let light = 0.45 + 0.55 * diffuse;
    // Alpha-cutout so foliage/cross textures don't draw their transparent areas.
    if (tex.a < 0.5) {
        discard;
    }
    return vec4<f32>(tex.rgb * light, 1.0);
}
"#;

fn main() {
    pollster::block_on(run());
}

async fn run() {
    // ---- wgpu init (headless: no surface) ----
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
        .expect("Failed to find a wgpu adapter");
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("block_gallery device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
        }, None)
        .await
        .expect("Failed to create wgpu device");

    println!("Using adapter: {}", adapter.get_info().name);

    // ---- Texture atlas (2D array) from the game's procedural generator ----
    let atlas_data = texture::generate_texture_data();
    let tex_size = texture::TEXTURE_SIZE;
    let tex_count = texture::TEXTURE_COUNT;

    let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("atlas"),
        size: wgpu::Extent3d {
            width: tex_size,
            height: tex_size,
            depth_or_array_layers: tex_count,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &atlas_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &atlas_data,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(4 * tex_size),
            rows_per_image: Some(tex_size),
        },
        wgpu::Extent3d {
            width: tex_size,
            height: tex_size,
            depth_or_array_layers: tex_count,
        },
    );
    let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("atlas sampler"),
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });

    // ---- Uniform buffer + bind groups ----
    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("uniforms"),
        size: std::mem::size_of::<Uniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("uniform layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("uniform bind group"),
        layout: &uniform_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        }],
    });

    let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("texture layout"),
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
    let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("texture bind group"),
        layout: &texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&atlas_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });

    // ---- Pipeline ----
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("gallery shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
    });

    let render_format = wgpu::TextureFormat::Rgba8UnormSrgb;

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("pipeline layout"),
        bind_group_layouts: &[&uniform_layout, &texture_layout],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("gallery pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
                    wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32x3 },
                    wgpu::VertexAttribute { offset: 24, shader_location: 2, format: wgpu::VertexFormat::Float32x2 },
                    wgpu::VertexAttribute { offset: 32, shader_location: 3, format: wgpu::VertexFormat::Uint32 },
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: render_format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: Some(wgpu::Face::Back),
            front_face: wgpu::FrontFace::Ccw,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    // ---- Offscreen render target + depth ----
    let color_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("color target"),
        size: wgpu::Extent3d { width: IMG_SIZE, height: IMG_SIZE, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: render_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth target"),
        size: wgpu::Extent3d { width: IMG_SIZE, height: IMG_SIZE, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

    // Readback buffer (256-byte row alignment required by wgpu copies).
    let bytes_per_pixel = 4u32;
    let unpadded_bytes_per_row = IMG_SIZE * bytes_per_pixel;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bytes_per_row = ((unpadded_bytes_per_row + align - 1) / align) * align;
    let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (padded_bytes_per_row * IMG_SIZE) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    // Projection is shared across all views.
    let proj = OPENGL_TO_WGPU * perspective(Deg(40.0), 1.0, 0.1, 100.0);

    // Output root directory.
    let out_root = std::path::Path::new("block_renders");
    std::fs::create_dir_all(out_root).expect("create block_renders dir");

    let mut total_images = 0usize;
    let mut block_count = 0usize;

    for raw in 0u8..(VOXEL_TYPE_COUNT as u8) {
        let vt = VoxelType::from(raw);
        if !is_renderable_block(vt) {
            continue;
        }
        block_count += 1;

        let (vertices, indices) = build_cube(vt);
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let index_count = indices.len() as u32;

        let name = block_name(vt);
        let block_dir = out_root.join(&name);
        std::fs::create_dir_all(&block_dir).expect("create block dir");

        for (label, dir) in VIEWS.iter() {
            // Eye sits along the requested axis with a small diagonal offset so
            // the cube reads as a 3D block (3/4 view) while still facing the
            // requested side head-on.
            let axis = Vector3::new(dir[0], dir[1], dir[2]);
            let up = if dir[1].abs() > 0.5 {
                Vector3::new(0.0, 0.0, if dir[1] > 0.0 { -1.0 } else { 1.0 })
            } else {
                Vector3::new(0.0, 1.0, 0.0)
            };
            let right = axis.cross(up).normalize();
            let dir_v = (axis + right * 0.35 + up * 0.30).normalize();
            let distance = 2.6f32;
            let eye = Point3::from_vec(dir_v * distance);
            let view = Matrix4::look_at_rh(eye, Point3::new(0.0, 0.0, 0.0), up);
            let view_proj = proj * view;

            // Light from the upper-left of the scene so faces are shaded.
            let light_dir = Vector3::new(-0.4f32, -0.8, -0.3).normalize();
            let uniforms = Uniforms {
                view_proj: view_proj.into(),
                light_dir: [light_dir.x, light_dir.y, light_dir.z, 0.0],
            };
            queue.write_buffer(&uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("encoder"),
            });
            {
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("render pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &color_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.1, g: 0.12, b: 0.16, a: 1.0 }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                rpass.set_pipeline(&pipeline);
                rpass.set_bind_group(0, &uniform_bind_group, &[]);
                rpass.set_bind_group(1, &texture_bind_group, &[]);
                rpass.set_vertex_buffer(0, vertex_buffer.slice(..));
                rpass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                rpass.draw_indexed(0..index_count, 0, 0..1);
            }

            // Copy the rendered color target into the readback buffer.
            encoder.copy_texture_to_buffer(
                wgpu::ImageCopyTexture {
                    texture: &color_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::ImageCopyBuffer {
                    buffer: &readback_buffer,
                    layout: wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_bytes_per_row),
                        rows_per_image: Some(IMG_SIZE),
                    },
                },
                wgpu::Extent3d { width: IMG_SIZE, height: IMG_SIZE, depth_or_array_layers: 1 },
            );

            queue.submit(std::iter::once(encoder.finish()));

            // Map + read the buffer back to CPU.
            let slice = readback_buffer.slice(..);
            let (tx, rx) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |res| {
                let _ = tx.send(res);
            });
            device.poll(wgpu::Maintain::Wait);
            rx.recv().expect("map channel").expect("map readback buffer");

            let data = slice.get_mapped_range();
            // Strip row padding into a tight RGBA8 buffer.
            let mut pixels = Vec::with_capacity((IMG_SIZE * IMG_SIZE * 4) as usize);
            for row in 0..IMG_SIZE {
                let start = (row * padded_bytes_per_row) as usize;
                let end = start + unpadded_bytes_per_row as usize;
                pixels.extend_from_slice(&data[start..end]);
            }
            drop(data);
            readback_buffer.unmap();

            let img: image::RgbaImage =
                image::ImageBuffer::from_raw(IMG_SIZE, IMG_SIZE, pixels)
                    .expect("construct image from pixels");
            let path = block_dir.join(format!("{}.png", label));
            img.save(&path).expect("save png");
            total_images += 1;
        }
        println!("Rendered {} ({} views)", name, VIEWS.len());
    }

    println!(
        "Done: {} blocks x {} views = {} images written to {}",
        block_count,
        VIEWS.len(),
        total_images,
        out_root.display()
    );
}