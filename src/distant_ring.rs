/// Distant ring renderer - renders a low-poly representation of the entire ring
/// so the player can see it curving overhead like in Halo
/// Uses the terrain generator to color the ring based on actual biome/terrain data

use wgpu::util::DeviceExt;
use crate::chunk::ChunkVertex;
use crate::terrain::TerrainGenerator;
use crate::ring_world::RingWorldConfig;
use crate::texture::TEX_GRASS_TOP;

/// The distant ring mesh
pub struct DistantRing {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_indices: u32,
}

impl DistantRing {
    pub fn new(
        device: &wgpu::Device,
        config: &RingWorldConfig,
        terrain_generator: &TerrainGenerator,
        thickness: f64,
        segments_around: u32,
        segments_width: u32,
    ) -> Self {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let half_width = config.width as f32 / 2.0;
        let r = config.radius as f32;
        let thick = thickness as f32;

        // Use a "solid color" texture index - we use TEX_GRASS_TOP as default
        // but set the vertex color to the terrain color so tinting handles it
        let default_tex_index = TEX_GRASS_TOP;

        // Generate the inner surface of the ring with terrain-based coloring
        for i in 0..=segments_around {
            let theta = (i as f32 / segments_around as f32) * std::f32::consts::PI * 2.0;
            let cos_t = theta.cos();
            let sin_t = theta.sin();

            for j in 0..=segments_width {
                let t = j as f32 / segments_width as f32;
                let y = -half_width + t * config.width as f32;

                let x = r * cos_t;
                let z = r * sin_t;

                // Normal points inward (toward center)
                let nx = -cos_t;
                let nz = -sin_t;

                // Sample terrain color at this (theta, y) position
                let sample_theta = theta as f64;
                let sample_y = y as f64;
                let terrain_color = terrain_generator.sample_terrain_color(
                    sample_theta,
                    sample_y,
                    config,
                );

                // Apply a slight distance darkening for depth perception
                let dist_shade = 0.85;
                let color = [
                    terrain_color[0] * dist_shade,
                    terrain_color[1] * dist_shade,
                    terrain_color[2] * dist_shade,
                    terrain_color[3],
                ];

                vertices.push(ChunkVertex {
                    position: [x, y, z],
                    normal: [nx, 0.0, nz],
                    color,
                    tex_coords: [t, i as f32 / segments_around as f32],
                    tex_index: default_tex_index,
                    light_level: 1.0,
                    alpha_tested: 0,
                });
            }
        }

        // Generate indices for the inner surface
        for i in 0..segments_around {
            for j in 0..segments_width {
                let row_start = i * (segments_width + 1);
                let next_row_start = (i + 1) * (segments_width + 1);

                let tl = row_start + j;
                let tr = row_start + j + 1;
                let bl = next_row_start + j;
                let br = next_row_start + j + 1;

                indices.push(tl);
                indices.push(bl);
                indices.push(tr);
                indices.push(tr);
                indices.push(bl);
                indices.push(br);
            }
        }

        // Add edge walls (sides of the ring)
        let inner_vertex_count = vertices.len() as u32;
        
        // Top edge strip
        for i in 0..=segments_around {
            let theta = (i as f32 / segments_around as f32) * std::f32::consts::PI * 2.0;
            let cos_t = theta.cos();
            let sin_t = theta.sin();

            vertices.push(ChunkVertex {
                position: [r * cos_t, half_width, r * sin_t],
                normal: [0.0, 1.0, 0.0],
                color: [0.4, 0.35, 0.3, 1.0],
                tex_coords: [0.0, 0.0],
                tex_index: default_tex_index,
                light_level: 1.0,
                alpha_tested: 0,
            });
            vertices.push(ChunkVertex {
                position: [(r + thick) * cos_t, half_width, (r + thick) * sin_t],
                normal: [0.0, 1.0, 0.0],
                color: [0.3, 0.25, 0.2, 1.0],
                tex_coords: [1.0, 0.0],
                tex_index: default_tex_index,
                light_level: 1.0,
                alpha_tested: 0,
            });
        }

        for i in 0..segments_around {
            let base = inner_vertex_count + i * 2;
            indices.push(base);
            indices.push(base + 2);
            indices.push(base + 1);
            indices.push(base + 1);
            indices.push(base + 2);
            indices.push(base + 3);
        }

        // Bottom edge strip
        let bottom_start = vertices.len() as u32;
        for i in 0..=segments_around {
            let theta = (i as f32 / segments_around as f32) * std::f32::consts::PI * 2.0;
            let cos_t = theta.cos();
            let sin_t = theta.sin();

            vertices.push(ChunkVertex {
                position: [r * cos_t, -half_width, r * sin_t],
                normal: [0.0, -1.0, 0.0],
                color: [0.4, 0.35, 0.3, 1.0],
                tex_coords: [0.0, 0.0],
                tex_index: default_tex_index,
                light_level: 1.0,
                alpha_tested: 0,
            });
            vertices.push(ChunkVertex {
                position: [(r + thick) * cos_t, -half_width, (r + thick) * sin_t],
                normal: [0.0, -1.0, 0.0],
                color: [0.3, 0.25, 0.2, 1.0],
                tex_coords: [1.0, 0.0],
                tex_index: default_tex_index,
                light_level: 1.0,
                alpha_tested: 0,
            });
        }

        for i in 0..segments_around {
            let base = bottom_start + i * 2;
            indices.push(base);
            indices.push(base + 1);
            indices.push(base + 2);
            indices.push(base + 2);
            indices.push(base + 1);
            indices.push(base + 3);
        }

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Distant Ring Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Distant Ring Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            vertex_buffer,
            index_buffer,
            num_indices: indices.len() as u32,
        }
    }
}
