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
        let (mut vertices, mut indices) =
            build_inner_surface(config, terrain_generator, segments_around, segments_width);

        let half_width = config.width as f32 / 2.0;
        let r = config.radius as f32;
        let thick = thickness as f32;
        let default_tex_index = TEX_GRASS_TOP;

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

/// Build the heightmapped inner surface of the distant ring: vertices are
/// displaced inward by the real terrain height at each (theta, y) sample
/// (oceans held flat at sea level), colored from the terrain palette with a
/// touch of noise mottling, and given finite-difference normals so mountain
/// slopes on the arch catch sun light like real relief. Pure function so
/// tests can validate the mesh without a GPU.
pub fn build_inner_surface(
    config: &RingWorldConfig,
    terrain_generator: &TerrainGenerator,
    segments_around: u32,
    segments_width: u32,
) -> (Vec<ChunkVertex>, Vec<u32>) {
    use crate::terrain::SEA_LEVEL;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    // Half the axial width: vertex y must span [-width/2, +width/2] exactly
    // like real chunk coordinates (ChunkCoord::to_ring_position starts at
    // -width/2). Using the full width here shifted the whole arch by half
    // the world width so its relief/colors didn't line up with the terrain.
    let half_width = config.width / 2.0;
    let default_tex_index = TEX_GRASS_TOP;

    // Visible surface height at (theta, y): terrain height, but never below
    // the water line, so oceans read as a flat blue sheet on the arch.
    let surface_h = |theta: f64, y: f64| -> f64 {
        let noise_x = theta * config.radius * 0.01;
        let noise_z = y * 0.01;
        let h = terrain_generator.sample_terrain_height(noise_x, noise_z, config);
        h.max(SEA_LEVEL as f64)
    };

    for i in 0..=segments_around {
        // Sample the LAST row at exactly theta = 0 too (i == segments_around
        // wraps) so the seam vertices are bit-identical and the arch closes.
        let wrap_i = i % segments_around;
        let theta = (wrap_i as f64 / segments_around as f64) * std::f64::consts::TAU;
        let d_theta = std::f64::consts::TAU / segments_around as f64;

        for j in 0..=segments_width {
            let t = j as f64 / segments_width as f64;
            let y = -half_width + t * config.width;
            let dy = config.width / segments_width as f64;

            let h = surface_h(theta, y);
            let rad = config.radius - h;
            let (cos_t, sin_t) = (theta.cos(), theta.sin());
            let pos = [
                (rad * cos_t) as f32,
                y as f32,
                (rad * sin_t) as f32,
            ];

            // Finite-difference normal: tangent vectors along theta and y on
            // the displaced surface, crossed and oriented inward (toward the
            // sun) so relief shading works through the standard sun shader.
            let h_t = surface_h(theta + d_theta, y);
            let h_y = surface_h(theta, (y + dy).min(half_width));
            // d(pos)/d(theta) (per radian, scaled arbitrarily; only direction matters)
            let rad_t = config.radius - h_t;
            let p_t = [
                rad_t * (theta + d_theta).cos() - rad * cos_t,
                0.0,
                rad_t * (theta + d_theta).sin() - rad * sin_t,
            ];
            // d(pos)/dy
            let p_y = [
                (rad - (config.radius - h_y)) * cos_t,
                dy,
                (rad - (config.radius - h_y)) * sin_t,
            ];
            let mut n = [
                p_t[1] * p_y[2] - p_t[2] * p_y[1],
                p_t[2] * p_y[0] - p_t[0] * p_y[2],
                p_t[0] * p_y[1] - p_t[1] * p_y[0],
            ];
            // Orient inward: inward at this theta is (-cos, 0, -sin).
            if n[0] * -cos_t + n[2] * -sin_t < 0.0 {
                n = [-n[0], -n[1], -n[2]];
            }
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
            let normal = [(n[0] / len) as f32, (n[1] / len) as f32, (n[2] / len) as f32];

            // Terrain palette + subtle noise mottling to break up banding.
            let noise_x = theta * config.radius * 0.01;
            let noise_z = y * 0.01;
            let terrain_color = terrain_generator.sample_terrain_color(theta, y, config);
            let mottle = 1.0
                + terrain_generator.sample_mottle(noise_x, noise_z) as f32 * 0.08;
            let dist_shade = 0.85 * mottle;
            let color = [
                terrain_color[0] * dist_shade,
                terrain_color[1] * dist_shade,
                terrain_color[2] * dist_shade,
                terrain_color[3],
            ];

            vertices.push(ChunkVertex {
                position: pos,
                normal,
                color,
                tex_coords: [t as f32, i as f32 / segments_around as f32],
                tex_index: default_tex_index,
                light_level: 1.0,
                alpha_tested: 0,
            });
        }
    }

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

    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inner_surface_seam_is_closed_and_heights_sane() {
        let config = RingWorldConfig::default();
        let terrain = TerrainGenerator::new(42);
        let segs_a = 64;
        let segs_w = 8;
        let (verts, idx) = build_inner_surface(&config, &terrain, segs_a, segs_w);
        assert_eq!(verts.len() as u32, (segs_a + 1) * (segs_w + 1));
        assert_eq!(idx.len() as u32, segs_a * segs_w * 6);

        // Seam closure: first row (i=0) and last row (i=segs_a) must be
        // bit-identical positions.
        for j in 0..=segs_w {
            let a = &verts[j as usize];
            let b = &verts[(segs_a * (segs_w + 1) + j) as usize];
            assert_eq!(a.position, b.position, "seam vertex {} differs", j);
        }

        // Every vertex radius must be displaced by a plausible terrain
        // height: between sea level and the world height cap. Vertex y must
        // span the world's true axial extent [-width/2, +width/2] (an offset
        // here = arch visibly misaligned with the terrain under your feet).
        let half_w = config.width / 2.0;
        let min_y = verts.iter().map(|v| v.position[1]).fold(f32::MAX, f32::min);
        let max_y = verts.iter().map(|v| v.position[1]).fold(f32::MIN, f32::max);
        assert!((min_y as f64 + half_w).abs() < 1e-3, "min y {} != -width/2", min_y);
        assert!((max_y as f64 - half_w).abs() < 1e-3, "max y {} != +width/2", max_y);
        for v in verts.iter() {
            let rad = (v.position[0] as f64).hypot(v.position[2] as f64);
            let h = config.radius - rad;
            assert!(
                h >= crate::terrain::SEA_LEVEL as f64 - 1e-3 && h <= 63.0,
                "vertex height {} out of range",
                h
            );
            // Normals point inward-ish and are unit length.
            let n = v.normal;
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-3);
        }
    }
}
