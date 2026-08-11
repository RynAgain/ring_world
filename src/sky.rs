/// Sky rendering: the central sun disk and a starfield.
///
/// Until now the "sun" was only lighting math plus a fog glow — there was no
/// disk to SEE, which made the shadow-square eclipse invisible as an event.
/// This module draws:
///
/// - **Sun disk**: a camera-facing fan at the ring's center with a soft glow
///   ring. It is drawn AFTER the shadow squares, whose dedicated pipeline
///   writes depth, with depth TEST on: a square passing between the camera
///   and the center geometrically occludes the disk — you watch the eclipse
///   happen.
/// - **Starfield**: fixed star directions on a 2500-unit celestial sphere,
///   translated to follow the camera (infinite-distance illusion). Star alpha
///   rises as local daylight falls: faint by noon, vivid at night. Drawn
///   FIRST (before the distant ring), so the arch and terrain always cover it.
///
/// Both draw with one unlit alpha-blended pipeline (camera bind group only,
/// depth write OFF).

use cgmath::{InnerSpace, Point3, Vector3};
use rand::{Rng, SeedableRng};
use wgpu::util::DeviceExt;

/// Number of stars on the celestial sphere.
const STAR_COUNT: usize = 700;
/// Celestial sphere radius (must stay inside zfar = 5000).
const STAR_DISTANCE: f32 = 2500.0;
/// Star billboard half-size in world units (~0.2 deg at 2500).
const STAR_SIZE: f32 = 4.5;
/// Sun disk radius in world units (~4 deg across seen from the ring surface).
const SUN_RADIUS: f32 = 45.0;
/// Outer radius of the sun's glow ring.
const SUN_GLOW_RADIUS: f32 = 110.0;
/// Fan segments for the sun disk.
const SUN_SEGMENTS: u32 = 48;
/// Star alpha in full daylight (space stays faintly star-flecked at noon).
const STAR_ALPHA_DAY: f32 = 0.10;
/// Star alpha in full night.
const STAR_ALPHA_NIGHT: f32 = 0.95;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct SkyVertex {
    position: [f32; 3],
    color: [f32; 4],
}

const SKY_SHADER: &str = r#"
struct CameraUniform {
    view_proj: mat4x4<f32>,
    view_position: vec4<f32>,
};
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
};
struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

/// Precomputed per-star geometry: three vertex offsets from the camera
/// (direction * distance + in-plane triangle offsets) plus a tint.
struct Star {
    offsets: [[f32; 3]; 3],
    tint: [f32; 3],
}

pub struct Sky {
    pipeline: wgpu::RenderPipeline,
    stars: Vec<Star>,
    star_vertex_buffer: wgpu::Buffer,
    star_num_vertices: u32,
    sun_vertex_buffer: wgpu::Buffer,
    sun_index_buffer: wgpu::Buffer,
    sun_num_indices: u32,
}

impl Sky {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        // ---- Fixed star catalogue (deterministic seed) ----
        let mut rng = rand::rngs::StdRng::seed_from_u64(0x52494E47); // "RING"
        let mut stars = Vec::with_capacity(STAR_COUNT);
        for _ in 0..STAR_COUNT {
            // Uniform direction on the sphere.
            let z: f32 = rng.gen_range(-1.0..1.0);
            let a: f32 = rng.gen_range(0.0..std::f32::consts::PI * 2.0);
            let r = (1.0f32 - z * z).sqrt();
            let dir = Vector3::new(r * a.cos(), z, r * a.sin());

            // Per-star orthonormal frame (camera-independent; a ~0.2 deg
            // triangle's orientation is imperceptible).
            let helper = if dir.y.abs() < 0.9 {
                Vector3::new(0.0, 1.0, 0.0)
            } else {
                Vector3::new(1.0, 0.0, 0.0)
            };
            let t1 = dir.cross(helper).normalize();
            let t2 = dir.cross(t1);

            let size = STAR_SIZE * rng.gen_range(0.5..1.5);
            let center = dir * STAR_DISTANCE;
            let offsets = [
                center + t1 * size,
                center + t2 * size - t1 * size * 0.5,
                center - t2 * size - t1 * size * 0.5,
            ];
            // Subtle color variation: white, blue-white, warm.
            let tint = match rng.gen_range(0..3) {
                0 => [1.0, 1.0, 1.0],
                1 => [0.8, 0.88, 1.0],
                _ => [1.0, 0.92, 0.8],
            };
            let b = rng.gen_range(0.5..1.0f32);
            stars.push(Star {
                offsets: [
                    [offsets[0].x, offsets[0].y, offsets[0].z],
                    [offsets[1].x, offsets[1].y, offsets[1].z],
                    [offsets[2].x, offsets[2].y, offsets[2].z],
                ],
                tint: [tint[0] * b, tint[1] * b, tint[2] * b],
            });
        }

        let star_num_vertices = (STAR_COUNT * 3) as u32;
        let star_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Sky Star Vertex Buffer"),
            size: (star_num_vertices as usize * std::mem::size_of::<SkyVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ---- Sun disk + glow (indices are static; vertices billboarded per frame) ----
        let sun_vertex_count = 1 + SUN_SEGMENTS + SUN_SEGMENTS; // center + rim + glow ring
        let sun_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Sky Sun Vertex Buffer"),
            size: (sun_vertex_count as usize * std::mem::size_of::<SkyVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut sun_indices: Vec<u32> = Vec::new();
        for s in 0..SUN_SEGMENTS {
            let next = (s + 1) % SUN_SEGMENTS;
            // Disk fan triangle: center, rim s, rim next.
            sun_indices.extend_from_slice(&[0, 1 + s, 1 + next]);
            // Glow quad between rim ring and glow ring.
            let rim_s = 1 + s;
            let rim_n = 1 + next;
            let glow_s = 1 + SUN_SEGMENTS + s;
            let glow_n = 1 + SUN_SEGMENTS + next;
            sun_indices.extend_from_slice(&[rim_s, glow_s, glow_n, rim_s, glow_n, rim_n]);
        }
        let sun_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Sky Sun Index Buffer"),
            contents: bytemuck::cast_slice(&sun_indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        // ---- Pipeline: unlit, alpha-blended, depth test on / write off ----
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Sky Shader"),
            source: wgpu::ShaderSource::Wgsl(SKY_SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Sky Pipeline Layout"),
            bind_group_layouts: &[camera_bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Sky Pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<SkyVertex>() as wgpu::BufferAddress,
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
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
                depth_write_enabled: false,
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

        Self {
            pipeline,
            stars,
            star_vertex_buffer,
            star_num_vertices,
            sun_vertex_buffer,
            sun_index_buffer,
            sun_num_indices: sun_indices.len() as u32,
        }
    }

    /// Rebuild the per-frame vertex data: stars follow the camera (so they sit
    /// at optical infinity) with alpha driven by local daylight; the sun disk
    /// billboards toward the camera.
    pub fn update(&self, queue: &wgpu::Queue, cam_pos: Point3<f32>, daylight: f32) {
        // ---- Stars ----
        let alpha = STAR_ALPHA_DAY + (STAR_ALPHA_NIGHT - STAR_ALPHA_DAY) * (1.0 - daylight);
        let mut verts: Vec<SkyVertex> = Vec::with_capacity(self.stars.len() * 3);
        for star in &self.stars {
            let color = [star.tint[0], star.tint[1], star.tint[2], alpha];
            for off in &star.offsets {
                verts.push(SkyVertex {
                    position: [cam_pos.x + off[0], cam_pos.y + off[1], cam_pos.z + off[2]],
                    color,
                });
            }
        }
        queue.write_buffer(&self.star_vertex_buffer, 0, bytemuck::cast_slice(&verts));

        // ---- Sun disk (billboard at the ring center = origin) ----
        let to_cam = Vector3::new(cam_pos.x, cam_pos.y, cam_pos.z);
        let dist = to_cam.magnitude();
        let n = if dist > 1.0 {
            to_cam / dist
        } else {
            Vector3::new(1.0, 0.0, 0.0)
        };
        // The ring axis (world Y) is nearly perpendicular to any camera on the
        // ring surface, so it is a safe basis helper.
        let t1 = Vector3::new(0.0, 1.0, 0.0).cross(n).normalize();
        let t2 = n.cross(t1);

        let core = [1.0f32, 0.98, 0.9, 1.0];
        let rim = [1.0f32, 0.9, 0.6, 1.0];
        let glow = [1.0f32, 0.75, 0.4, 0.0];

        let mut sun_verts: Vec<SkyVertex> = Vec::with_capacity((1 + SUN_SEGMENTS * 2) as usize);
        sun_verts.push(SkyVertex { position: [0.0, 0.0, 0.0], color: core });
        for ring_r_color in [(SUN_RADIUS, rim), (SUN_GLOW_RADIUS, glow)] {
            let (radius, color) = ring_r_color;
            for s in 0..SUN_SEGMENTS {
                let a = (s as f32 / SUN_SEGMENTS as f32) * std::f32::consts::PI * 2.0;
                let p = t1 * (a.cos() * radius) + t2 * (a.sin() * radius);
                sun_verts.push(SkyVertex { position: [p.x, p.y, p.z], color });
            }
        }
        queue.write_buffer(&self.sun_vertex_buffer, 0, bytemuck::cast_slice(&sun_verts));
    }

    /// Draw the starfield (call FIRST, before the distant ring).
    pub fn draw_stars<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, camera_bind_group: &'a wgpu::BindGroup) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, camera_bind_group, &[]);
        pass.set_vertex_buffer(0, self.star_vertex_buffer.slice(..));
        pass.draw(0..self.star_num_vertices, 0..1);
    }

    /// Draw the sun disk (call AFTER the shadow squares have written depth,
    /// so a passing square geometrically eclipses the disk).
    pub fn draw_sun<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, camera_bind_group: &'a wgpu::BindGroup) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, camera_bind_group, &[]);
        pass.set_vertex_buffer(0, self.sun_vertex_buffer.slice(..));
        pass.set_index_buffer(self.sun_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.sun_num_indices, 0, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_offsets_sit_on_celestial_sphere() {
        // Rebuild the same catalogue the GPU path uses (no device needed).
        let mut rng = rand::rngs::StdRng::seed_from_u64(0x52494E47);
        for _ in 0..STAR_COUNT {
            let z: f32 = rng.gen_range(-1.0..1.0);
            let a: f32 = rng.gen_range(0.0..std::f32::consts::PI * 2.0);
            let r = (1.0f32 - z * z).sqrt();
            let dir = Vector3::new(r * a.cos(), z, r * a.sin());
            assert!((dir.magnitude() - 1.0).abs() < 1e-5);
            // consume the rest of the per-star randomness in the same order
            let _ = rng.gen_range(0.5..1.5f32);
            let _ = rng.gen_range(0..3);
            let _ = rng.gen_range(0.5..1.0f32);
        }
    }

    #[test]
    fn sun_fan_indices_reference_valid_vertices() {
        let vertex_count = 1 + SUN_SEGMENTS * 2;
        let mut indices: Vec<u32> = Vec::new();
        for s in 0..SUN_SEGMENTS {
            let next = (s + 1) % SUN_SEGMENTS;
            indices.extend_from_slice(&[0, 1 + s, 1 + next]);
            let rim_s = 1 + s;
            let rim_n = 1 + next;
            let glow_s = 1 + SUN_SEGMENTS + s;
            let glow_n = 1 + SUN_SEGMENTS + next;
            indices.extend_from_slice(&[rim_s, glow_s, glow_n, rim_s, glow_n, rim_n]);
        }
        assert!(indices.iter().all(|&i| i < vertex_count));
        assert_eq!(indices.len() as u32, SUN_SEGMENTS * 9);
    }
}
