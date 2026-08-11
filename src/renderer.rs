/// Renderer module - wgpu-based rendering pipeline for the ring world

use std::collections::HashMap;
use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::window::Window;
#[allow(unused_imports)]
use cgmath::{Matrix4, SquareMatrix};
use rayon::prelude::*;

use crate::chunk::{Chunk, ChunkManager, ChunkMeshData, ChunkVertex};
use crate::distant_ring::DistantRing;
use crate::entity::EntityManager;
use crate::hud::{Hud, HudRenderData};
use crate::player::{Player, PlacementPreview};
use crate::lighting::LightingEngine;
use crate::ring_world::{ChunkCoord, RingPosition, RingWorldConfig, curved_local_to_world};
use crate::sun::{Sun, SunUniform};
use crate::terrain::TerrainGenerator;
use crate::texture::TextureAtlas;
use crate::voxel::VoxelType;

/// Transform uniform for each chunk
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct ChunkTransformUniform {
    model: [[f32; 4]; 4],
}

/// GPU mesh data for a chunk
struct ChunkMesh {
    /// Opaque + alpha-cutout geometry (Pass A: depth write on).
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    /// Translucent water geometry (Pass B: depth write off, no back-face cull).
    /// `None` when this chunk contains no water faces.
    water_vertex_buffer: Option<wgpu::Buffer>,
    water_index_buffer: Option<wgpu::Buffer>,
    water_num_indices: u32,
    #[allow(dead_code)]
    transform_buffer: wgpu::Buffer,
    transform_bind_group: wgpu::BindGroup,
    /// Mesh version this GPU buffer was built from (chunk-mesh caching).
    mesh_version: u64,
    /// World-space center of the chunk (for frustum culling).
    center: [f32; 3],
    /// Whether this mesh is non-empty (used for occlusion culling).
    non_empty: bool,
    /// Whether this cached GPU mesh was built with the LOD (low-detail) mesher
    /// (true) or the full-resolution mesher (false). Compared against the
    /// chunk's CURRENT desired LOD level each frame so a chunk re-meshes when
    /// the player crosses the LOD distance boundary (fixes "valley faces missing
    /// until I edit a block").
    meshed_as_lod: bool,
}

/// A view frustum defined by its 6 clipping planes, extracted from a
/// view-projection matrix. Each plane is stored as (a, b, c, d) where
/// a*x + b*y + c*z + d >= 0 means the point is on the inside.
pub struct Frustum {
    planes: [[f32; 4]; 6],
}

impl Frustum {
    /// Extract the 6 frustum planes from a column-major view-projection matrix
    /// stored as `[[f32; 4]; 4]` (cgmath / wgpu convention: m[col][row]).
    pub fn from_view_proj(vp: [[f32; 4]; 4]) -> Self {
        // Access as m(row, col). vp is column-major: vp[col][row].
        let m = |row: usize, col: usize| vp[col][row];

        // Rows of the matrix.
        let row0 = [m(0, 0), m(0, 1), m(0, 2), m(0, 3)];
        let row1 = [m(1, 0), m(1, 1), m(1, 2), m(1, 3)];
        let row2 = [m(2, 0), m(2, 1), m(2, 2), m(2, 3)];
        let row3 = [m(3, 0), m(3, 1), m(3, 2), m(3, 3)];

        let add = |a: [f32; 4], b: [f32; 4]| [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]];
        let sub = |a: [f32; 4], b: [f32; 4]| [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]];

        let mut planes = [
            add(row3, row0), // left
            sub(row3, row0), // right
            add(row3, row1), // bottom
            sub(row3, row1), // top
            // wgpu/D3D clip space has NDC z in [0, 1] (not [-1, 1] like OpenGL),
            // so the near plane is just row2 and the far plane is row3 - row2.
            row2,            // near
            sub(row3, row2), // far
        ];

        // Normalize planes so distances are in world units.
        for p in planes.iter_mut() {
            let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            if len > 1e-6 {
                p[0] /= len;
                p[1] /= len;
                p[2] /= len;
                p[3] /= len;
            }
        }

        Self { planes }
    }

    /// Returns true if the sphere (center, radius) is at least partially inside
    /// the frustum (conservative).
    pub fn is_sphere_visible(&self, center: [f32; 3], radius: f32) -> bool {
        for p in &self.planes {
            let dist = p[0] * center[0] + p[1] * center[1] + p[2] * center[2] + p[3];
            if dist < -radius {
                return false;
            }
        }
        true
    }
}

/// Highlight box vertex (position + color with alpha)
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct HighlightVertex {
    position: [f32; 3],
    color: [f32; 4],
}

/// Main rendering state
pub struct State {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
    render_pipeline: wgpu::RenderPipeline,
    /// Translucency pass pipeline (water): depth TEST on, depth WRITE off,
    /// alpha blend on, no back-face culling. Used in Pass B.
    water_pipeline: wgpu::RenderPipeline,
    /// F6 render-diagnostic pipeline: identical to `render_pipeline` (depth
    /// test + WRITE on) but with back-face culling DISABLED so the F6 mode
    /// truly "disables all culling" as documented. Without this, Pass A still
    /// drew with `cull_mode: Back`, so any back-facing / ambiguously-wound face
    /// stayed hidden in F6 and looked like missing geometry.
    debug_pipeline: wgpu::RenderPipeline,
    /// Background pipeline for the distant ring: depth TEST on, depth WRITE
    /// OFF, so the full-world backdrop shell can never occlude the real loaded
    /// chunk terrain that is coincident with it.
    distant_ring_pipeline: wgpu::RenderPipeline,
    highlight_pipeline: wgpu::RenderPipeline,
    depth_texture_view: wgpu::TextureView,

    // Camera
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    camera_bind_group_layout: wgpu::BindGroupLayout,

    // Sun/Lighting
    #[allow(dead_code)]
    sun: Sun,
    sun_buffer: wgpu::Buffer,
    sun_bind_group: wgpu::BindGroup,

    // Texture Atlas
    #[allow(dead_code)]
    texture_atlas: TextureAtlas,
    texture_bind_group: wgpu::BindGroup,

    // World
    ring_config: RingWorldConfig,
    chunk_manager: ChunkManager,
    terrain_generator: TerrainGenerator,
    chunk_meshes: HashMap<ChunkCoord, ChunkMesh>,

    // Transform bind group layout (needed for creating per-chunk bind groups)
    transform_bind_group_layout: wgpu::BindGroupLayout,

    // Distant ring visualization
    distant_ring: DistantRing,
    distant_ring_transform_bind_group: wgpu::BindGroup,

    // HUD
    hud: Hud,

    // Player
    pub player: Player,

    // Entity system
    pub entity_manager: EntityManager,

    // Highlight box for placement preview / breaking target
    highlight_vertex_buffer: wgpu::Buffer,
    highlight_index_buffer: wgpu::Buffer,
    highlight_num_indices: u32,

    // Whether the player has been snapped onto the terrain surface after the
    // spawn-area chunks finished generating (prevents falling through
    // ungenerated terrain on the first frames).
    spawn_settled: bool,

    /// Whether the debug overlay is visible (toggled with F3).
    pub debug_visible: bool,

    /// Runtime toggle: frustum culling on/off (F4). Default on (verified correct).
    pub enable_frustum_cull: bool,
    /// Runtime toggle: neighbor occlusion culling on/off (F5). Default OFF:
    /// correctness over the optional optimization (it was hiding visible terrain).
    pub enable_occlusion_cull: bool,
    /// Runtime toggle: greedy meshing on/off. Default on (now winding-correct).
    /// Currently informational for the F3 overlay; meshing always uses the
    /// (fixed) greedy path.
    pub enable_greedy_mesh: bool,

    /// Runtime toggle: render-diagnostic mode (F6). Default off. When on, ALL
    /// chunk culling is disabled, lighting/fog are bypassed, the alpha discard
    /// is skipped, and each face is flat-tinted by its world-space normal sign
    /// (via the shader debug_mode flag) so missing geometry / bad textures /
    /// culling bugs can be told apart at a glance.
    pub debug_render: bool,

    /// Render distance in chunks (adjustable). Controls how far chunks load.
    pub render_distance: u32,

    /// Number of chunks actually drawn last frame (after frustum + occlusion
    /// culling). Used by the F3 debug overlay.
    rendered_chunks: u32,

    /// Smoothed frames-per-second estimate for the debug overlay.
    fps_smoothed: f32,
}

impl State {
    pub async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();

        // Create wgpu instance
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // Create surface from Arc<Window> for 'static lifetime
        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    label: Some("device"),
                },
                None,
            )
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // Depth texture
        let depth_texture_view = Self::create_depth_texture(&device, &config);

        // Ring world config
        let ring_config = RingWorldConfig::default();

        // Player
        let player = Player::new(&ring_config, size.width, size.height);

        // Camera uniform
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[player.camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
                label: Some("camera_bind_group_layout"),
            });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
            label: Some("camera_bind_group"),
        });

        // Sun uniform
        let sun = Sun::new();
        let sun_uniform = SunUniform::from_sun(&sun);
        let sun_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Sun Buffer"),
            contents: bytemuck::cast_slice(&[sun_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let sun_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
                label: Some("sun_bind_group_layout"),
            });

        let sun_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &sun_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: sun_buffer.as_entire_binding(),
            }],
            label: Some("sun_bind_group"),
        });

        // Transform bind group layout (per-chunk)
        let transform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
                label: Some("transform_bind_group_layout"),
            });

        // Texture atlas (bind group 3)
        let texture_atlas = TextureAtlas::new(&device, &queue);
        let texture_bind_group_layout = TextureAtlas::bind_group_layout(&device);
        let texture_bind_group = texture_atlas.bind_group(&device, &texture_bind_group_layout);

        // Shader
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        // Pipeline layout
        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[
                    &camera_bind_group_layout,
                    &sun_bind_group_layout,
                    &transform_bind_group_layout,
                    &texture_bind_group_layout,
                ],
                push_constant_ranges: &[],
            });

        // Render pipeline
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[ChunkVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    // Opaque geometry must NOT alpha-blend. With ALPHA_BLENDING
                    // here, any depth tie between two coplanar/adjacent faces let
                    // the blend mix in the cleared space color, producing the
                    // "flickering see-through holes on solid surfaces" artifact.
                    // REPLACE writes the fragment color directly (matches the
                    // block-gallery pipeline that renders correctly). Translucent
                    // water is drawn separately by `water_pipeline` (which keeps
                    // ALPHA_BLENDING).
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                // Back-face culling ON. Chunk meshes are pre-curved into
                // world space by `curve_mesh_data`, which also reverses each
                // triangle's index order to compensate for the ring frame's
                // reflection, so every front face is consistently CCW-outward
                // (guarded by chunk::tests::curved_mesh_triangles_wind_ccw_outward).
                // The historical "half the side faces missing at many ring
                // positions" symptom came from the old flat chunk_transform's
                // mirrored tangent, which is gone. Cross-render billboards emit
                // both sides explicitly and survive culling. (Water keeps its
                // own no-cull pipeline.)
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        // Water (translucency) pipeline (Pass B): same shader + bind groups as
        // the opaque pipeline, but with depth WRITE disabled (depth TEST stays
        // on so water is still occluded by nearer opaque geometry) and back-face
        // culling off so both sides of the water surface are visible. This fixes
        // the "missing blocks behind water" / per-frame flicker artifact caused
        // by drawing translucent water with depth-write on in HashMap order.
        let water_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Water Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[ChunkVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None, // show both faces of the water surface
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false, // translucency: test but don't write
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        // Distant-ring (background) pipeline. The distant ring is a full 360°
        // shell at the ring's true radius that visualizes the whole world as a
        // backdrop. The player stands ON that shell, so large parts of the
        // distant-ring mesh are COINCIDENT with (or nearer than) the real loaded
        // chunk terrain. Drawing it with depth-WRITE on (as the opaque pipeline
        // does) plants depth values exactly where the real terrain sits, and the
        // real terrain then fails the depth test against that coincident shell —
        // so terrain faces silently vanish in a viewpoint-dependent way (and in
        // F6 too, since the ring is drawn in every mode). Rendering the backdrop
        // with depth-WRITE OFF (test still on) makes it a pure background: real
        // chunks always draw over it and it can never occlude them.
        let distant_ring_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Distant Ring Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[ChunkVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false, // backdrop: never occlude real terrain
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        // F6 render-diagnostic pipeline: same as the opaque pipeline (depth
        // test + write on, alpha blend) but with back-face culling DISABLED so
        // the diagnostic mode genuinely shows every face regardless of winding.
        let debug_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Debug (F6) Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[ChunkVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None, // F6: disable back-face culling
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        // Highlight pipeline (for wireframe block outlines)
        let highlight_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Highlight Shader"),
            source: wgpu::ShaderSource::Wgsl(HIGHLIGHT_SHADER.into()),
        });

        let highlight_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Highlight Pipeline Layout"),
                bind_group_layouts: &[&camera_bind_group_layout],
                push_constant_ranges: &[],
            });

        let highlight_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Highlight Render Pipeline"),
            layout: Some(&highlight_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &highlight_shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<HighlightVertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                        wgpu::VertexAttribute {
                            offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                    ],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &highlight_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None, // No culling for wireframe
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false, // Don't write depth for highlight
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        // Create empty highlight buffers (will be updated each frame)
        let highlight_vertices: Vec<HighlightVertex> = Vec::new();
        let highlight_indices: Vec<u32> = vec![0]; // Need at least one element for buffer creation
        let highlight_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Highlight Vertex Buffer"),
            contents: bytemuck::cast_slice(&[HighlightVertex { position: [0.0; 3], color: [0.0; 4] }]),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let highlight_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Highlight Index Buffer"),
            contents: bytemuck::cast_slice(&highlight_indices),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        });
        let _ = highlight_vertices; // suppress unused warning

        // Chunk manager
        let chunk_manager = ChunkManager::new(ring_config.clone(), 8);
        let terrain_generator = TerrainGenerator::new(42);

        // Distant ring - visualization of the entire ring using terrain data
        let distant_ring = DistantRing::new(
            &device,
            &ring_config,
            &terrain_generator,
            30.0, // thickness
            128,  // segments around
            16,   // segments width (more for biome detail)
        );

        // Identity transform for the distant ring (it's already in world space)
        let identity_transform = ChunkTransformUniform {
            model: cgmath::Matrix4::from_scale(1.0f32).into(),
        };
        let distant_ring_transform_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Distant Ring Transform Buffer"),
                contents: bytemuck::cast_slice(&[identity_transform]),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let distant_ring_transform_bind_group =
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &transform_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: distant_ring_transform_buffer.as_entire_binding(),
                }],
                label: Some("distant_ring_transform_bind_group"),
            });

        // HUD
        let hud = Hud::new(&device, surface_format, size.width, size.height);

        Self {
            surface,
            device,
            queue,
            config,
            size,
            render_pipeline,
            water_pipeline,
            debug_pipeline,
            distant_ring_pipeline,
            highlight_pipeline,
            depth_texture_view,
            camera_buffer,
            camera_bind_group,
            camera_bind_group_layout,
            sun,
            sun_buffer,
            sun_bind_group,
            texture_atlas,
            texture_bind_group,
            ring_config,
            chunk_manager,
            terrain_generator,
            chunk_meshes: HashMap::new(),
            transform_bind_group_layout,
            distant_ring,
            distant_ring_transform_bind_group,
            hud,
            player,
            entity_manager: EntityManager::new(),
            highlight_vertex_buffer,
            highlight_index_buffer,
            highlight_num_indices: 0,
            spawn_settled: false,
            debug_visible: false,
            enable_frustum_cull: true,
            enable_occlusion_cull: false,
            enable_greedy_mesh: true,
            debug_render: false,
            render_distance: 8,
            rendered_chunks: 0,
            fps_smoothed: 0.0,
        }
    }

    fn create_depth_texture(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> wgpu::TextureView {
        let size = wgpu::Extent3d {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        };
        let desc = wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        let texture = device.create_texture(&desc);
        texture.create_view(&wgpu::TextureViewDescriptor::default())
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            self.depth_texture_view = Self::create_depth_texture(&self.device, &self.config);
            self.player.resize(new_size.width, new_size.height);
            self.hud.resize(&self.device, new_size.width, new_size.height);
        }
    }

    /// Toggle greedy meshing on/off (F7 A/B test) and force ALL loaded chunks to
    /// re-mesh so the change takes effect immediately. With greedy OFF, the
    /// mesher emits every visible block face as its own 1x1 quad (no merging).
    pub fn toggle_greedy_mesh(&mut self) {
        self.enable_greedy_mesh = !self.enable_greedy_mesh;
        for (_, chunk) in self.chunk_manager.chunks.iter_mut() {
            chunk.dirty = true;
        }
    }

    /// Destroy the block the player is looking at
    pub fn destroy_block(&mut self) -> bool {
        self.player.destroy_block(&self.ring_config, &mut self.chunk_manager)
    }

    /// Try to attack an entity the player is looking at (within 3 blocks).
    /// Returns true if an entity was hit.
    pub fn attack_entity(&mut self) -> bool {
        let camera_pos = self.player.camera.position;
        let look_dir = self.player.camera.forward();
        let look_vec = cgmath::Vector3::new(look_dir.x, look_dir.y, look_dir.z);

        // Only attack if no block is within 3 blocks
        let max_attack_distance = 3.0f32;

        if let Some((entity_id, _dist)) = self.entity_manager.raycast_hit_entity(
            &camera_pos,
            &look_vec,
            max_attack_distance,
            &self.ring_config,
        ) {
            // Deal 1.0 damage (fist) with knockback from player position
            self.entity_manager.damage_entity(
                entity_id,
                1.0,
                &self.player.ring_position,
                &self.ring_config,
            );
            true
        } else {
            false
        }
    }

    /// Place a block adjacent to the face the player is looking at
    pub fn place_block(&mut self, block_type: VoxelType) -> bool {
        self.player.place_block(block_type, &self.ring_config, &mut self.chunk_manager)
    }

    /// Continue the block breaking process (called each frame)
    pub fn continue_breaking(&mut self, dt: std::time::Duration) -> bool {
        let dt_secs = dt.as_secs_f32();
        self.player.continue_breaking(dt_secs, &self.ring_config, &mut self.chunk_manager)
    }

    pub fn update(&mut self, dt: std::time::Duration) {
        // Ensure the chunks around the player are loaded and generated BEFORE
        // running physics, so the player never falls through ungenerated terrain
        // on the first frames after spawn. Once the spawn area is ready, snap the
        // player onto the surface a single time.
        if !self.spawn_settled {
            self.chunk_manager
                .update_loaded_chunks(&self.player.ring_position);
            self.generate_pending_chunks();
            if self.try_settle_spawn() {
                self.spawn_settled = true;
            }
        }

        // Update player physics (gravity, collision) — only after the spawn area
        // has been generated and the player snapped onto the surface.
        if self.spawn_settled {
            self.player.update_physics(dt, &self.ring_config, &self.chunk_manager);
        }

        // Update player
        self.player.update(dt, &self.ring_config);

        // Update interaction state (raycast, reach, placement preview)
        self.player.update_interaction(&self.ring_config, &self.chunk_manager);

        // Continuous breaking
        if self.player.left_mouse_held {
            let dt_secs = dt.as_secs_f32();
            self.player.continue_breaking(dt_secs, &self.ring_config, &mut self.chunk_manager);
        }

        // Update entity system
        let dt_secs = dt.as_secs_f32();
        self.entity_manager.update(
            dt_secs,
            &self.player.ring_position,
            &mut self.player.health,
            &self.chunk_manager,
            &self.ring_config,
            &mut self.player.inventory,
        );

        // Smooth the FPS estimate for the debug overlay (exponential moving avg).
        let dt_secs_f = dt.as_secs_f32();
        if dt_secs_f > 0.0 {
            let inst_fps = 1.0 / dt_secs_f;
            if self.fps_smoothed <= 0.0 {
                self.fps_smoothed = inst_fps;
            } else {
                self.fps_smoothed = self.fps_smoothed * 0.9 + inst_fps * 0.1;
            }
        }

        // Build debug overlay text lines (only when the overlay is visible).
        let debug_lines = if self.debug_visible {
            self.build_debug_lines()
        } else {
            Vec::new()
        };

        // Update the full HUD (crosshair, health bar, hotbar) each frame.
        let hud_data = HudRenderData {
            health: self.player.health,
            max_health: 20.0,
            hotbar: self.player.hotbar,
            hotbar_index: self.player.hotbar_index,
            debug_visible: self.debug_visible,
            target_in_reach: self.player.target_in_reach,
            debug_lines,
        };
        self.hud.update(&self.queue, &hud_data);

        // Update highlight box for placement preview or breaking target
        self.update_highlight_box();

        // Update camera uniform on GPU
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&[self.player.camera_uniform]),
        );

        // Update loaded chunks based on player position
        self.chunk_manager.update_loaded_chunks(&self.player.ring_position);

        // ---- Task 5: Multithreaded chunk generation (rayon) ----
        self.generate_pending_chunks();

        // ---- LOD transition re-mesh ----
        // A chunk is meshed as LOD (low detail) when its distance > 7, else
        // full-resolution. But a chunk only re-meshes when `dirty`, and moving
        // the player across the distance-7 boundary does NOT by itself set the
        // flag. So a chunk meshed at low detail when far stays low-detail up
        // close — and the LOD mesher legitimately produces different (coarser)
        // geometry that drops faces on uneven valley/cliff terrain. The result
        // was "valley walls missing until I break a block" (breaking forces a
        // re-mesh). Here we compare each chunk's CURRENT desired LOD level
        // against what its cached GPU mesh was actually built with, and mark any
        // mismatch dirty so it re-meshes at the correct detail.
        {
            let player_pos = self.player.ring_position;
            let mut to_dirty: Vec<ChunkCoord> = Vec::new();
            for (coord, mesh) in self.chunk_meshes.iter() {
                let dist = self.chunk_manager.chunk_distance(coord, &player_pos);
                let want_lod = dist > 7;
                if mesh.meshed_as_lod != want_lod {
                    to_dirty.push(*coord);
                }
            }
            for coord in to_dirty {
                if let Some(chunk) = self.chunk_manager.chunks.get_mut(&coord) {
                    chunk.dirty = true;
                }
            }
        }

        // ---- Determine which chunks are dirty and need a mesh rebuild ----
        let dirty_coords: Vec<ChunkCoord> = self
            .chunk_manager
            .chunks
            .iter()
            .filter(|(_, chunk)| chunk.dirty && chunk.generated)
            .map(|(coord, _)| *coord)
            .collect();

        // Recompute lighting for dirty chunks before mesh generation (sequential,
        // since it only touches the single chunk and is cheap).
        for coord in &dirty_coords {
            if let Some(chunk) = self.chunk_manager.chunks.get_mut(coord) {
                LightingEngine::recompute_lighting(chunk);
            }
        }

        if !dirty_coords.is_empty() {
            let config = &self.ring_config;
            let chunk_manager = &self.chunk_manager;
            let chunks = &self.chunk_manager.chunks;
            let player_pos = self.player.ring_position;
            let greedy = self.enable_greedy_mesh;

            // ---- Task 6: Multithreaded mesh building (rayon) ----
            // Each task reads its chunk + 6 neighbors (all immutable borrows) and
            // produces a vertex/index buffer plus the LOD flag used for it.
            // ---- Task 4: LOD selection (chunks beyond distance 5 use the LOD mesh) ----
            let built: Vec<(ChunkCoord, ChunkMeshData, bool)> = dirty_coords
                .par_iter()
                .map(|coord| {
                    let coord = *coord;
                    // Neighbor order: [+X, -X, +Y, -Y, +Z, -Z]
                    let neighbor_coords: [Option<ChunkCoord>; 6] = [
                        coord.neighbor(1, 0, 0, config),
                        coord.neighbor(-1, 0, 0, config),
                        coord.neighbor(0, 0, 1, config),
                        coord.neighbor(0, 0, -1, config),
                        coord.neighbor(0, 1, 0, config),
                        coord.neighbor(0, -1, 0, config),
                    ];
                    let neighbors: [Option<&Chunk>; 6] = [
                        neighbor_coords[0].and_then(|c| chunks.get(&c)),
                        neighbor_coords[1].and_then(|c| chunks.get(&c)),
                        neighbor_coords[2].and_then(|c| chunks.get(&c)),
                        neighbor_coords[3].and_then(|c| chunks.get(&c)),
                        neighbor_coords[4].and_then(|c| chunks.get(&c)),
                        neighbor_coords[5].and_then(|c| chunks.get(&c)),
                    ];
                    let chunk = chunks.get(&coord).unwrap();

                    // Use the full-resolution mesher for everything the player
                    // can clearly see; only the farthest ring of chunks uses LOD.
                    // (render_distance is 8, so dist > 7 = only the outermost
                    // shell.) The LOD mesher is lower fidelity and, even with the
                    // full-solid occlusion fix, is best reserved for far chunks.
                    let dist = chunk_manager.chunk_distance(&coord, &player_pos);
                    let is_lod = dist > 7;
                    let mut mesh = if is_lod {
                        chunk.generate_lod_mesh_split(&neighbors)
                    } else if greedy {
                        chunk.generate_mesh_split(&neighbors)
                    } else {
                        // F7 A/B test: non-greedy meshing emits every visible
                        // face as its own quad (no merging).
                        chunk.generate_mesh_split_no_greedy(&neighbors)
                    };
                    // Bake the exact ring curvature into the vertices so the
                    // rendered geometry matches the ANGULAR collision grid
                    // exactly (no flat-chunk seams, no visual/collision skew).
                    // The mesh is now world-space: drawn with identity model.
                    crate::chunk::curve_mesh_data(&mut mesh, &coord, config);
                    (coord, mesh, is_lod)
                })
                .collect();

            // ---- Task 7: Chunk mesh caching ----
            // Now that meshes are built, increment each chunk's mesh_version and
            // clear its dirty flag (main thread), then upload to the GPU.
            for (coord, mesh, is_lod) in built {
                let new_version = {
                    let chunk = self.chunk_manager.chunks.get_mut(&coord).unwrap();
                    chunk.dirty = false;
                    chunk.mesh_version += 1;
                    chunk.mesh_version
                };

                // Chunk world-space center for frustum culling, via the same
                // curved mapping the vertices use.
                let half = self.ring_config.chunk_size as f32 * 0.5;
                let center = curved_local_to_world(&coord, [half, half, half], &self.ring_config);

                let has_opaque = !mesh.opaque_vertices.is_empty() && !mesh.opaque_indices.is_empty();
                let has_water = !mesh.water_vertices.is_empty() && !mesh.water_indices.is_empty();

                if !has_opaque && !has_water {
                    // Empty mesh: drop any existing GPU buffer.
                    self.chunk_meshes.remove(&coord);
                    continue;
                }

                // The opaque pipeline always needs a (possibly empty) vertex /
                // index buffer bound. When a chunk is water-only we still create
                // a 1-element placeholder so the ChunkMesh struct is valid; its
                // num_indices is 0 so the opaque draw is skipped.
                let (vertex_buffer, index_buffer, num_indices) = if has_opaque {
                    let vb = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Chunk Vertex Buffer"),
                        contents: bytemuck::cast_slice(&mesh.opaque_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    let ib = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Chunk Index Buffer"),
                        contents: bytemuck::cast_slice(&mesh.opaque_indices),
                        usage: wgpu::BufferUsages::INDEX,
                    });
                    (vb, ib, mesh.opaque_indices.len() as u32)
                } else {
                    let vb = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Chunk Vertex Buffer (empty)"),
                        contents: bytemuck::cast_slice(&[ChunkVertex {
                            position: [0.0; 3], normal: [0.0; 3], color: [0.0; 4],
                            tex_coords: [0.0; 2], tex_index: 0, light_level: 0.0,
                            alpha_tested: 0,
                        }]),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    let ib = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Chunk Index Buffer (empty)"),
                        contents: bytemuck::cast_slice(&[0u32]),
                        usage: wgpu::BufferUsages::INDEX,
                    });
                    (vb, ib, 0u32)
                };

                let (water_vertex_buffer, water_index_buffer, water_num_indices) = if has_water {
                    let vb = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Chunk Water Vertex Buffer"),
                        contents: bytemuck::cast_slice(&mesh.water_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    let ib = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Chunk Water Index Buffer"),
                        contents: bytemuck::cast_slice(&mesh.water_indices),
                        usage: wgpu::BufferUsages::INDEX,
                    });
                    (Some(vb), Some(ib), mesh.water_indices.len() as u32)
                } else {
                    (None, None, 0u32)
                };

                // Vertices are pre-curved into world space; the model
                // transform is identity.
                let transform_uniform = ChunkTransformUniform {
                    model: cgmath::Matrix4::from_scale(1.0f32).into(),
                };

                let transform_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Chunk Transform Buffer"),
                            contents: bytemuck::cast_slice(&[transform_uniform]),
                            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                        });

                let transform_bind_group =
                    self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        layout: &self.transform_bind_group_layout,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: transform_buffer.as_entire_binding(),
                        }],
                        label: Some("chunk_transform_bind_group"),
                    });

                self.chunk_meshes.insert(
                    coord,
                    ChunkMesh {
                        vertex_buffer,
                        index_buffer,
                        num_indices,
                        water_vertex_buffer,
                        water_index_buffer,
                        water_num_indices,
                        transform_buffer,
                        transform_bind_group,
                        mesh_version: new_version,
                        center,
                        non_empty: true,
                        meshed_as_lod: is_lod,
                    },
                );
            }
        }

        // Remove meshes for chunks that are no longer loaded
        self.chunk_meshes.retain(|coord, _| self.chunk_manager.chunks.contains_key(coord));
    }

    /// Generate terrain + lighting for any loaded-but-ungenerated chunks, using
    /// rayon to parallelise across chunks. (Task 5: Multithreaded chunk generation.)
    fn generate_pending_chunks(&mut self) {
        let ungenerated: Vec<ChunkCoord> = self
            .chunk_manager
            .chunks
            .iter()
            .filter(|(_, c)| !c.generated)
            .map(|(coord, _)| *coord)
            .collect();

        if ungenerated.is_empty() {
            return;
        }

        // Move the chunks out of the manager so we can mutate them off-thread.
        let mut to_generate: Vec<Chunk> = Vec::with_capacity(ungenerated.len());
        for coord in &ungenerated {
            if let Some(chunk) = self.chunk_manager.chunks.remove(coord) {
                to_generate.push(chunk);
            }
        }

        // Generate terrain + lighting in parallel. TerrainGenerator's Perlin
        // fields are Send + Sync, so a shared &self reference is fine.
        let generator = &self.terrain_generator;
        let config = &self.ring_config;
        to_generate.par_iter_mut().for_each(|chunk| {
            generator.generate_chunk(chunk, config);
            LightingEngine::compute_lighting(chunk);
        });

        // Re-insert generated chunks on the main thread.
        for chunk in to_generate {
            self.chunk_manager.chunks.insert(chunk.coord, chunk);
        }

        // ---- Issue 1: re-mesh neighbors on neighbor readiness ----
        // A freshly generated chunk changes the boundary-face visibility of its
        // 6 already-generated neighbors (their boundary faces were previously
        // evaluated against missing data). Mark those neighbors dirty so they
        // re-mesh against the now-present neighbor, removing the load-frontier
        // "shell" / seam faces. True world edges (neighbor() == None for the
        // width/height bounds) are unaffected and still draw their edge faces.
        let config = self.ring_config.clone();
        let neighbor_deltas: [(i32, i32, i32); 6] = [
            (1, 0, 0), (-1, 0, 0),
            (0, 0, 1), (0, 0, -1),
            (0, 1, 0), (0, -1, 0),
        ];
        for coord in &ungenerated {
            for (dr, dw, dh) in &neighbor_deltas {
                if let Some(ncoord) = coord.neighbor(*dr, *dw, *dh, &config) {
                    // Don't re-dirty the chunks we just generated in this batch
                    // (they are already dirty and will mesh against real data).
                    if ungenerated.contains(&ncoord) {
                        continue;
                    }
                    if let Some(n) = self.chunk_manager.chunks.get_mut(&ncoord) {
                        if n.generated {
                            n.dirty = true;
                        }
                    }
                }
            }
        }
    }

    /// Once the chunk containing the player's spawn column has been generated,
    /// snap the player to just above the highest solid voxel in that column so
    /// they don't begin embedded in (or falling through) the terrain.
    /// Returns true when the player has been settled.
    fn try_settle_spawn(&mut self) -> bool {
        let config = &self.ring_config;

        // Pick a spawn column near the origin that is on dry land (not Ocean
        // and above sea level), so the player doesn't deterministically spawn
        // in water / on an ocean floor for a given seed. Falls back to (0, 0).
        let (theta, y) = self.choose_spawn_column();

        // Make sure the spawn column's surface chunk is loaded and generated
        // before attempting to settle. We check the chunk at the player's
        // current (spawn) height to gate readiness.
        let probe = RingPosition::new(theta, y, config.max_height - 0.5);
        let spawn_coord = ChunkCoord::from_ring_position(&probe, config);
        let ready = self
            .chunk_manager
            .get_chunk(&spawn_coord)
            .map(|c| c.generated)
            .unwrap_or(false);
        if !ready {
            return false;
        }

        // Spawn near the top of the world (the radial ceiling) and let gravity
        // drop the player onto the terrain. `height` is measured radially inward
        // from the ring surface toward the sun, so `max_height` is the ceiling.
        // We sit a little below it so the player's head (body_center + half
        // PLAYER_HEIGHT) stays within the valid range and isn't clamped.
        let player_height = 2.0f64; // matches PLAYER_HEIGHT
        // Leave 1 block of clearance below the ceiling for the player's head.
        let body_center = config.max_height - 1.0 - player_height * 0.5;

        // Keep the safe-landing column as the player's RESPAWN point so that on
        // death they reappear on the ground rather than skydiving every time.
        let feet = crate::player::find_safe_spawn_height(theta, y, &self.chunk_manager, config);
        let ground_body_center = feet + player_height * 0.5;

        self.player.ring_position = RingPosition::new(theta, y, body_center);
        // Respawn point = on the ground at the same column.
        self.player.spawn_position = RingPosition::new(theta, y, ground_body_center);
        self.player.vertical_velocity = 0.0;
        // Not grounded: physics gravity will pull the player down to the surface.
        self.player.grounded = false;
        true
    }

    /// Choose a spawn column (theta, y) near the world origin that sits on dry,
    /// habitable land. Scans an outward spiral of candidate columns, sampling
    /// the biome + terrain height with the same noise convention the terrain
    /// generator uses, and returns the first column that is NOT Ocean and whose
    /// surface is at or above sea level. Falls back to the origin (0, 0) if no
    /// suitable land column is found within the search radius.
    fn choose_spawn_column(&self) -> (f64, f64) {
        let config = &self.ring_config;
        let sea_level = crate::terrain::SEA_LEVEL as f64;

        // Candidate columns: the origin first, then expanding rings of offsets
        // in world-space blocks. We step in whole blocks so each candidate maps
        // to a distinct surface column.
        let max_radius_blocks = 24i32;
        let step = 2i32;

        // Helper closure: is this (theta, y) good dry land?
        let is_good_land = |theta: f64, y: f64| -> bool {
            // Match TerrainGenerator's noise coordinate convention.
            let noise_x = theta * config.radius * 0.01;
            let noise_z = y * 0.01;
            let biome = self.terrain_generator.sample_biome(noise_x, noise_z);
            if biome == crate::terrain::Biome::Ocean {
                return false;
            }
            let height = self
                .terrain_generator
                .sample_terrain_height(noise_x, noise_z, config);
            // Require the surface to be on/above sea level so we never spawn in
            // a depression that floods (Beach/river edges can dip below it).
            height >= sea_level
        };

        // Origin first.
        if is_good_land(0.0, 0.0) {
            return (0.0, 0.0);
        }

        // Expanding square rings around the origin (in world blocks). Convert a
        // world-block arc offset to a theta delta via the ring radius.
        let half_width = config.width / 2.0;
        for radius in (step..=max_radius_blocks).step_by(step as usize) {
            // Walk the perimeter of the square ring at this radius.
            let mut candidates: Vec<(i32, i32)> = Vec::new();
            let mut a = -radius;
            while a <= radius {
                candidates.push((a, -radius));
                candidates.push((a, radius));
                candidates.push((-radius, a));
                candidates.push((radius, a));
                a += step;
            }
            for (arc_off, y_off) in candidates {
                let theta = arc_off as f64 / config.radius;
                let y = (y_off as f64).clamp(-half_width, half_width);
                if is_good_land(theta, y) {
                    let mut pos = RingPosition::new(theta, y, 0.0);
                    pos.normalize_theta();
                    return (pos.theta, y);
                }
            }
        }

        // No dry land found nearby: fall back to the origin.
        (0.0, 0.0)
    }

    /// Update the highlight box mesh for placement preview or breaking target
    fn update_highlight_box(&mut self) {
        // Determine which block to highlight:
        // 1. If breaking, highlight the breaking target
        // 2. Otherwise, highlight the placement preview
        let highlight_target: Option<PlacementPreview> = if self.player.breaking_target.is_some() {
            // Show highlight on the block being broken
            self.player.breaking_target.map(|t| PlacementPreview {
                chunk_coord: t.chunk_coord,
                local_x: t.local_x,
                local_y: t.local_y,
                local_z: t.local_z,
            })
        } else {
            self.player.placement_preview
        };

        if let Some(preview) = highlight_target {
            // Generate wireframe cube vertices in world space
            let (vertices, indices) = self.generate_highlight_mesh(&preview);
            
            if !vertices.is_empty() && !indices.is_empty() {
                self.highlight_vertex_buffer = self.device.create_buffer_init(
                    &wgpu::util::BufferInitDescriptor {
                        label: Some("Highlight Vertex Buffer"),
                        contents: bytemuck::cast_slice(&vertices),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    },
                );
                self.highlight_index_buffer = self.device.create_buffer_init(
                    &wgpu::util::BufferInitDescriptor {
                        label: Some("Highlight Index Buffer"),
                        contents: bytemuck::cast_slice(&indices),
                        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                    },
                );
                self.highlight_num_indices = indices.len() as u32;
            } else {
                self.highlight_num_indices = 0;
            }
        } else {
            self.highlight_num_indices = 0;
        }
    }

    /// Generate a wireframe cube mesh at the given block position
    /// Returns vertices and indices for 12 edges rendered as thin quads
    fn generate_highlight_mesh(&self, preview: &PlacementPreview) -> (Vec<HighlightVertex>, Vec<u32>) {

        // Block local position within chunk (each block is 1 unit)
        let bx = preview.local_x as f32;
        let by = preview.local_y as f32;
        let bz = preview.local_z as f32;

        // Determine color based on whether we're breaking or previewing placement
        let color = if self.player.breaking_target.is_some() {
            // Red-ish tint for breaking, intensity based on progress
            let p = self.player.breaking_progress;
            [1.0, 1.0 - p * 0.5, 1.0 - p * 0.5, 0.4 + p * 0.4]
        } else {
            // Semi-transparent white/yellow for placement preview
            [1.0, 1.0, 0.8, 0.35]
        };

        // The 8 corners of the cube (slightly expanded to avoid z-fighting)
        let e = 0.005; // expansion amount
        let corners: [[f32; 3]; 8] = [
            [bx - e,       by - e,       bz - e],       // 0: ---
            [bx + 1.0 + e, by - e,       bz - e],       // 1: +--
            [bx + 1.0 + e, by + 1.0 + e, bz - e],       // 2: ++-
            [bx - e,       by + 1.0 + e, bz - e],       // 3: -+-
            [bx - e,       by - e,       bz + 1.0 + e], // 4: --+
            [bx + 1.0 + e, by - e,       bz + 1.0 + e], // 5: +-+
            [bx + 1.0 + e, by + 1.0 + e, bz + 1.0 + e], // 6: +++
            [bx - e,       by + 1.0 + e, bz + 1.0 + e], // 7: -++
        ];

        // Map corners to world space through the exact curved ring mapping,
        // matching the curved chunk meshes (the old flat chunk_transform put
        // the highlight box up to ~0.5 blocks away from the rendered block).
        let transformed: Vec<[f32; 3]> = corners.iter().map(|c| {
            curved_local_to_world(&preview.chunk_coord, *c, &self.ring_config)
        }).collect();

        // 12 edges of a cube, each edge becomes a thin quad (2 triangles)
        let edges: [(usize, usize); 12] = [
            // Bottom face edges
            (0, 1), (1, 2), (2, 3), (3, 0),
            // Top face edges
            (4, 5), (5, 6), (6, 7), (7, 4),
            // Vertical edges
            (0, 4), (1, 5), (2, 6), (3, 7),
        ];

        let thickness = 0.02; // Edge thickness in world units
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        for (i, (a, b)) in edges.iter().enumerate() {
            let p0 = transformed[*a];
            let p1 = transformed[*b];

            // Create a thin quad for this edge
            // We need a perpendicular direction to give the edge width
            let dx = p1[0] - p0[0];
            let dy = p1[1] - p0[1];
            let dz = p1[2] - p0[2];

            // Find a perpendicular vector (cross with an arbitrary axis)
            let (px, py, pz) = if dx.abs() < 0.9 {
                // Cross with X axis
                (0.0, -dz, dy)
            } else {
                // Cross with Y axis
                (dz, 0.0, -dx)
            };
            let len = (px * px + py * py + pz * pz).sqrt();
            if len < 0.0001 {
                continue;
            }
            let (px, py, pz) = (px / len * thickness, py / len * thickness, pz / len * thickness);

            let base_idx = vertices.len() as u32;

            // 4 vertices for this edge quad
            vertices.push(HighlightVertex { position: [p0[0] - px, p0[1] - py, p0[2] - pz], color });
            vertices.push(HighlightVertex { position: [p0[0] + px, p0[1] + py, p0[2] + pz], color });
            vertices.push(HighlightVertex { position: [p1[0] + px, p1[1] + py, p1[2] + pz], color });
            vertices.push(HighlightVertex { position: [p1[0] - px, p1[1] - py, p1[2] - pz], color });

            // 2 triangles
            indices.push(base_idx);
            indices.push(base_idx + 1);
            indices.push(base_idx + 2);
            indices.push(base_idx);
            indices.push(base_idx + 2);
            indices.push(base_idx + 3);

            // Back face (for visibility from both sides)
            indices.push(base_idx);
            indices.push(base_idx + 2);
            indices.push(base_idx + 1);
            indices.push(base_idx);
            indices.push(base_idx + 3);
            indices.push(base_idx + 2);

            let _ = i; // suppress unused
        }

        (vertices, indices)
    }

    /// Build the lines of debug text shown in the F3 overlay from real state.
    fn build_debug_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        let config = &self.ring_config;
        let pos = self.player.ring_position;

        // FPS
        lines.push(format!("FPS: {}", self.fps_smoothed.round() as i32));

        // Ring position (theta / y / height).
        lines.push(format!(
            "XYZ: theta={:.2} y={:.1} h={:.1}",
            pos.theta, pos.y, pos.height
        ));

        // Cartesian world position.
        let cart = pos.to_cartesian(config);
        lines.push(format!(
            "Cartesian: x={:.1} y={:.1} z={:.1}",
            cart.x, cart.y, cart.z
        ));

        // Chunk the player is in.
        let chunk_coord = ChunkCoord::from_ring_position(&pos, config);
        lines.push(format!(
            "Chunk: ring={} width={} height={}",
            chunk_coord.ring_index, chunk_coord.width_index, chunk_coord.height_index
        ));

        // Biome sampled at the player's (theta, y) using the terrain generator's
        // noise coordinate convention (see TerrainGenerator).
        let noise_x = pos.theta * config.radius * 0.01;
        let noise_z = pos.y * 0.01;
        let biome = self.terrain_generator.sample_biome(noise_x, noise_z);
        lines.push(format!("Biome: {}", biome.name()));

        // What the player is looking at (raycast hit).
        match self.player.current_raycast_hit {
            Some(hit) => {
                let voxel_type = self
                    .chunk_manager
                    .get_chunk(&hit.chunk_coord)
                    .map(|c| c.get_voxel(hit.local_x, hit.local_y, hit.local_z).voxel_type)
                    .unwrap_or(VoxelType::Air);
                lines.push(format!(
                    "Looking at: {:?} ({},{},{})",
                    voxel_type, hit.local_x, hit.local_y, hit.local_z
                ));
            }
            None => lines.push("Looking at: none".to_string()),
        }

        // Chunk counts.
        lines.push(format!("Loaded chunks: {}", self.chunk_manager.chunks.len()));
        lines.push(format!("Rendered chunks: {}", self.rendered_chunks));

        // Optimization toggles (A/B testing via F4 / F5).
        let on_off = |b: bool| if b { "on" } else { "off" };
        lines.push(format!("FrustumCull: {} (F4)", on_off(self.enable_frustum_cull)));
        lines.push(format!("OcclusionCull: {} (F5)", on_off(self.enable_occlusion_cull)));
        lines.push(format!("RenderDebug: {} (F6)", on_off(self.debug_render)));
        lines.push(format!("GreedyMesh: {}", on_off(self.enable_greedy_mesh)));

        // Entities.
        lines.push(format!("Entities: {}", self.entity_manager.entities.len()));

        // Health.
        lines.push(format!("Health: {}/20", self.player.health.round() as i32));

        // Game mode / flying.
        let mode = if self.player.creative_mode { "Creative" } else { "Survival" };
        let flying = if self.player.is_flying { "yes" } else { "no" };
        lines.push(format!("Mode: {}, Flying: {}", mode, flying));

        // Grounded / in water.
        let grounded = if self.player.grounded { "yes" } else { "no" };
        let in_water = if self.player.in_water { "yes" } else { "no" };
        lines.push(format!("Grounded: {}, In water: {}", grounded, in_water));

        lines
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // ---- F6 render-diagnostic mode ----
        // Push the current debug flag into the sun uniform (ambient.w) so the
        // fragment shader can switch into full-bright per-face-normal tinting.
        // Cheap to write every frame; the shader branch is free when off.
        {
            let mut sun_uniform = SunUniform::from_sun(&self.sun);
            sun_uniform.set_debug_mode(self.debug_render);
            self.queue
                .write_buffer(&self.sun_buffer, 0, bytemuck::cast_slice(&[sun_uniform]));
        }

        // Number of chunks actually drawn this frame (for the F3 debug overlay).
        let mut rendered_chunks: u32 = 0;

        // ---- Task 2: Frustum culling ----
        // Build the view frustum once per frame from the camera's view-projection.
        let frustum = Frustum::from_view_proj(self.player.camera_uniform.view_proj);
        // Bounding sphere radius for a 16^3 chunk: 16 * sqrt(3) / 2 ~= 13.86.
        let chunk_radius = self.ring_config.chunk_size as f32 * 1.7320508 / 2.0;

        // ---- Task 3: Occlusion culling (neighbor-based, CONSERVATIVE) ----
        // Default OFF (toggle with F5). Correctness first: the previous version
        // culled any chunk whose 6 neighbors merely had non-empty meshes, which
        // hid plainly-visible surface terrain.
        //
        // When enabled, a chunk is only considered occluded if ALL 6 face
        // neighbors are loaded AND each neighbor is fully opaque on the face it
        // shares with this chunk (so there is genuinely no line of sight in).
        let occluded: std::collections::HashSet<ChunkCoord> = if self.enable_occlusion_cull {
            // (offset toward neighbor, index of the neighbor's face that points
            // back at this chunk). Face indices: 0=+X,1=-X,2=+Y,3=-Y,4=+Z,5=-Z.
            // Local voxel axes map to ring axes as x->ring, y->height, z->width
            // (see chunk_transform). is_face_solid's face index convention is
            // 0=+X,1=-X,2=+Y,3=-Y,4=+Z,5=-Z. So for each neighbor reached via
            // neighbor(d_ring, d_width, d_height), the neighbor's face that
            // points BACK at this chunk is:
            //   +ring  (1,0,0)  -> neighbor -X = 1
            //   -ring (-1,0,0)  -> neighbor +X = 0
            //   +height(0,0,1)  -> neighbor -Y = 3
            //   -height(0,0,-1) -> neighbor +Y = 2
            //   +width (0,1,0)  -> neighbor -Z = 5
            //   -width (0,-1,0) -> neighbor +Z = 4
            let neighbor_offsets: [((i32, i32, i32), usize); 6] = [
                ((1, 0, 0), 1),
                ((-1, 0, 0), 0),
                ((0, 0, 1), 3),
                ((0, 0, -1), 2),
                ((0, 1, 0), 5),
                ((0, -1, 0), 4),
            ];
            self.chunk_meshes
                .keys()
                .copied()
                .filter(|coord| {
                    neighbor_offsets.iter().all(|((dr, dw, dh), shared_face)| {
                        match coord.neighbor(*dr, *dw, *dh, &self.ring_config) {
                            Some(nc) => self
                                .chunk_manager
                                .get_chunk(&nc)
                                .map(|c| c.generated && c.is_face_solid(*shared_face))
                                .unwrap_or(false),
                            // No neighbor (edge of ring) => visible from that side.
                            None => false,
                        }
                    })
                })
                .collect()
        } else {
            std::collections::HashSet::new()
        };

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // Dark space color with slight blue tint
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.01,
                            g: 0.01,
                            b: 0.03,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Render distant ring FIRST as a background, using its dedicated
            // depth-WRITE-OFF pipeline. Critical: the distant ring is a full
            // world-radius shell that is coincident with the real loaded chunk
            // terrain the player stands on. If it wrote depth (as it did when it
            // shared the opaque pipeline), the real terrain failed the depth test
            // against the coincident backdrop and whole faces vanished in a
            // viewpoint-dependent way (and in F6 too). Drawing it with depth
            // write off makes real chunks always render over it.
            render_pass.set_pipeline(&self.distant_ring_pipeline);
            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
            render_pass.set_bind_group(1, &self.sun_bind_group, &[]);
            render_pass.set_bind_group(3, &self.texture_bind_group, &[]);
            render_pass.set_bind_group(2, &self.distant_ring_transform_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.distant_ring.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.distant_ring.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..self.distant_ring.num_indices, 0, 0..1);

            // Now select the chunk pipeline. In F6 render-diagnostic mode use the
            // no-cull pipeline so EVERY face is drawn regardless of winding
            // ("disables all culling"); the normal opaque pipeline is used
            // otherwise.
            if self.debug_render {
                render_pass.set_pipeline(&self.debug_pipeline);
            } else {
                render_pass.set_pipeline(&self.render_pipeline);
            }
            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
            render_pass.set_bind_group(1, &self.sun_bind_group, &[]);
            render_pass.set_bind_group(3, &self.texture_bind_group, &[]);

            // ---- Pass A: opaque + alpha-cutout chunk geometry ----
            // Depth write + test ON. In F6 render-debug mode ALL culling is
            // disabled so "geometry exists but is culled" can be told apart from
            // "geometry was never built".
            for (coord, mesh) in &self.chunk_meshes {
                if !self.debug_render {
                    // Task 3: skip fully neighbor-occluded chunks.
                    if occluded.contains(coord) {
                        continue;
                    }
                    // Task 2: skip chunks whose bounding sphere is outside the
                    // frustum (when frustum culling is enabled; toggle with F4).
                    if self.enable_frustum_cull
                        && !frustum.is_sphere_visible(mesh.center, chunk_radius)
                    {
                        continue;
                    }
                }
                if mesh.num_indices == 0 {
                    continue;
                }
                render_pass.set_bind_group(2, &mesh.transform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                render_pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..mesh.num_indices, 0, 0..1);
                rendered_chunks += 1;
            }

            // ---- Pass B: translucent water geometry ----
            // Drawn AFTER all opaque geometry with the water pipeline (depth
            // TEST on, depth WRITE off, alpha blend, no back-face cull) so water
            // never occludes terrain behind it and both surfaces show. In F6
            // debug mode water was already drawn full-bright in Pass A's pipeline
            // is irrelevant; we still draw it here (depth-write-off is harmless)
            // so its faces get the per-face tint too.
            render_pass.set_pipeline(&self.water_pipeline);
            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
            render_pass.set_bind_group(1, &self.sun_bind_group, &[]);
            render_pass.set_bind_group(3, &self.texture_bind_group, &[]);
            for (coord, mesh) in &self.chunk_meshes {
                if mesh.water_num_indices == 0 {
                    continue;
                }
                if !self.debug_render {
                    if occluded.contains(coord) {
                        continue;
                    }
                    if self.enable_frustum_cull
                        && !frustum.is_sphere_visible(mesh.center, chunk_radius)
                    {
                        continue;
                    }
                }
                if let (Some(wvb), Some(wib)) =
                    (&mesh.water_vertex_buffer, &mesh.water_index_buffer)
                {
                    render_pass.set_bind_group(2, &mesh.transform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, wvb.slice(..));
                    render_pass.set_index_buffer(wib.slice(..), wgpu::IndexFormat::Uint32);
                    render_pass.draw_indexed(0..mesh.water_num_indices, 0, 0..1);
                }
            }

            // Render highlight box (placement preview / breaking target)
            if self.highlight_num_indices > 0 {
                render_pass.set_pipeline(&self.highlight_pipeline);
                render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
                render_pass.set_vertex_buffer(0, self.highlight_vertex_buffer.slice(..));
                render_pass.set_index_buffer(self.highlight_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..self.highlight_num_indices, 0, 0..1);
            }
        }

        // HUD pass (no depth test, rendered on top)
        {
            let mut hud_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("HUD Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Don't clear - draw on top
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            self.hud.render(&mut hud_pass);
        }

        // Record the rendered-chunk count for the next frame's debug overlay.
        self.rendered_chunks = rendered_chunks;

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

/// WGSL shader for highlight box rendering (simple position + color passthrough)
const HIGHLIGHT_SHADER: &str = r#"
struct CameraUniform {
    view_proj: mat4x4<f32>,
}
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
"#;
