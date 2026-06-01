/// Renderer module - wgpu-based rendering pipeline for the ring world

use std::collections::HashMap;
use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::window::Window;
#[allow(unused_imports)]
use cgmath::{Matrix4, SquareMatrix};
use rayon::prelude::*;

use crate::chunk::{Chunk, ChunkManager, ChunkVertex};
use crate::distant_ring::DistantRing;
use crate::entity::EntityManager;
use crate::hud::Hud;
use crate::player::{Player, PlacementPreview};
use crate::lighting::LightingEngine;
use crate::ring_world::{ChunkCoord, RingWorldConfig, chunk_transform};
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
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    #[allow(dead_code)]
    transform_buffer: wgpu::Buffer,
    transform_bind_group: wgpu::BindGroup,
    /// Mesh version this GPU buffer was built from (chunk-mesh caching).
    mesh_version: u64,
    /// World-space center of the chunk (for frustum culling).
    center: [f32; 3],
    /// Whether this mesh is non-empty (used for occlusion culling).
    non_empty: bool,
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
            add(row3, row2), // near
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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
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
        // Update player physics (gravity, collision)
        self.player.update_physics(dt, &self.ring_config, &self.chunk_manager);

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

        // Update HUD crosshair color based on reach
        self.hud.update_crosshair_color(&self.queue, self.player.target_in_reach);

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
        // Collect coords of chunks that still need terrain generation, then remove
        // those chunks from the manager, generate (+light) them in parallel, and
        // re-insert them on the main thread.
        let ungenerated: Vec<ChunkCoord> = self
            .chunk_manager
            .chunks
            .iter()
            .filter(|(_, c)| !c.generated)
            .map(|(coord, _)| *coord)
            .collect();

        if !ungenerated.is_empty() {
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

            // ---- Task 6: Multithreaded mesh building (rayon) ----
            // Each task reads its chunk + 6 neighbors (all immutable borrows) and
            // produces a vertex/index buffer plus the LOD flag used for it.
            // ---- Task 4: LOD selection (chunks beyond distance 5 use the LOD mesh) ----
            let built: Vec<(ChunkCoord, Vec<ChunkVertex>, Vec<u32>)> = dirty_coords
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

                    let dist = chunk_manager.chunk_distance(&coord, &player_pos);
                    let (vertices, indices) = if dist > 5 {
                        chunk.generate_lod_mesh(&neighbors)
                    } else {
                        chunk.generate_mesh_with_neighbors(&neighbors)
                    };
                    (coord, vertices, indices)
                })
                .collect();

            // ---- Task 7: Chunk mesh caching ----
            // Now that meshes are built, increment each chunk's mesh_version and
            // clear its dirty flag (main thread), then upload to the GPU.
            for (coord, vertices, indices) in built {
                let new_version = {
                    let chunk = self.chunk_manager.chunks.get_mut(&coord).unwrap();
                    chunk.dirty = false;
                    chunk.mesh_version += 1;
                    chunk.mesh_version
                };

                // Compute chunk world-space center for frustum culling.
                let transform_matrix = chunk_transform(&coord, &self.ring_config);
                let half = self.ring_config.chunk_size as f32 * 0.5;
                let center_local = cgmath::Vector4::new(half, half, half, 1.0);
                let center_world = transform_matrix * center_local;
                let center = [center_world.x, center_world.y, center_world.z];

                if vertices.is_empty() || indices.is_empty() {
                    // Empty mesh: drop any existing GPU buffer.
                    self.chunk_meshes.remove(&coord);
                    continue;
                }

                let vertex_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Chunk Vertex Buffer"),
                            contents: bytemuck::cast_slice(&vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                let index_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Chunk Index Buffer"),
                            contents: bytemuck::cast_slice(&indices),
                            usage: wgpu::BufferUsages::INDEX,
                        });

                let transform_uniform = ChunkTransformUniform {
                    model: transform_matrix.into(),
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
                        num_indices: indices.len() as u32,
                        transform_buffer,
                        transform_bind_group,
                        mesh_version: new_version,
                        center,
                        non_empty: true,
                    },
                );
            }
        }

        // Remove meshes for chunks that are no longer loaded
        self.chunk_meshes.retain(|coord, _| self.chunk_manager.chunks.contains_key(coord));
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
        // Get the chunk transform to position the block in world space
        let transform_matrix = chunk_transform(&preview.chunk_coord, &self.ring_config);

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

        // Transform corners to world space using the chunk transform
        let transformed: Vec<[f32; 3]> = corners.iter().map(|c| {
            let v = cgmath::Vector4::new(c[0], c[1], c[2], 1.0);
            let result = transform_matrix * v;
            [result.x, result.y, result.z]
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

        // ---- Task 2: Frustum culling ----
        // Build the view frustum once per frame from the camera's view-projection.
        let frustum = Frustum::from_view_proj(self.player.camera_uniform.view_proj);
        // Bounding sphere radius for a 16^3 chunk: 16 * sqrt(3) / 2 ~= 13.86.
        let chunk_radius = self.ring_config.chunk_size as f32 * 1.7320508 / 2.0;

        // ---- Task 3: Occlusion culling (neighbor-based, conservative) ----
        // A chunk is considered fully occluded if all 6 of its face-neighbors are
        // loaded and have non-empty meshes (so it cannot be seen between gaps).
        let occluded: std::collections::HashSet<ChunkCoord> = self
            .chunk_meshes
            .keys()
            .copied()
            .filter(|coord| {
                let neighbor_offsets: [(i32, i32, i32); 6] = [
                    (1, 0, 0),
                    (-1, 0, 0),
                    (0, 0, 1),
                    (0, 0, -1),
                    (0, 1, 0),
                    (0, -1, 0),
                ];
                neighbor_offsets.iter().all(|(dr, dw, dh)| {
                    match coord.neighbor(*dr, *dw, *dh, &self.ring_config) {
                        Some(nc) => self
                            .chunk_meshes
                            .get(&nc)
                            .map(|m| m.non_empty)
                            .unwrap_or(false),
                        // No neighbor (edge of ring) => cannot be occluded from that side.
                        None => false,
                    }
                })
            })
            .collect();

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

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
            render_pass.set_bind_group(1, &self.sun_bind_group, &[]);
            render_pass.set_bind_group(3, &self.texture_bind_group, &[]);

            // Render distant ring first (background)
            render_pass.set_bind_group(2, &self.distant_ring_transform_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.distant_ring.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.distant_ring.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..self.distant_ring.num_indices, 0, 0..1);

            // Render all chunk meshes (foreground), applying frustum + occlusion culling
            for (coord, mesh) in &self.chunk_meshes {
                // Task 3: skip fully neighbor-occluded chunks.
                if occluded.contains(coord) {
                    continue;
                }
                // Task 2: skip chunks whose bounding sphere is outside the frustum.
                if !frustum.is_sphere_visible(mesh.center, chunk_radius) {
                    continue;
                }
                render_pass.set_bind_group(2, &mesh.transform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                render_pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..mesh.num_indices, 0, 0..1);
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
