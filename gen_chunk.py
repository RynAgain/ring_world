#!/usr/bin/env python3
"""Generate src/chunk.rs with greedy meshing, LOD, and mesh_version."""

content = r'''/// Chunk system for the ring world
/// Each chunk is a cubic section of voxels positioned on the ring

use crate::voxel::{Voxel, VoxelType, FaceDir};
use crate::texture;
use crate::ring_world::{ChunkCoord, RingPosition, RingWorldConfig};

/// A chunk of voxels in the ring world
pub struct Chunk {
    pub coord: ChunkCoord,
    voxels: Vec<Voxel>,
    light_levels: Vec<u8>,
    size: u32,
    pub dirty: bool,
    pub generated: bool,
    pub mesh_version: u64,
}

impl Chunk {
    pub fn new(coord: ChunkCoord, size: u32) -> Self {
        let total = (size * size * size) as usize;
        Self {
            coord,
            voxels: vec![Voxel::air(); total],
            light_levels: vec![0u8; total],
            size,
            dirty: true,
            generated: false,
            mesh_version: 0,
        }
    }

    pub fn get_voxel(&self, x: u32, y: u32, z: u32) -> Voxel {
        if x >= self.size || y >= self.size || z >= self.size {
            return Voxel::air();
        }
        self.voxels[self.index(x, y, z)]
    }

    pub fn set_voxel(&mut self, x: u32, y: u32, z: u32, voxel: Voxel) {
        if x >= self.size || y >= self.size || z >= self.size {
            return;
        }
        let idx = self.index(x, y, z);
        self.voxels[idx] = voxel;
        self.dirty = true;
    }

    pub fn get_light(&self, x: u32, y: u32, z: u32) -> (u8, u8) {
        if x >= self.size || y >= self.size || z >= self.size {
            return (15, 0);
        }
        let idx = self.index(x, y, z);
        let packed = self.light_levels[idx];
        ((packed >> 4) & 0x0F, packed & 0x0F)
    }

    pub fn set_light(&mut self, x: u32, y: u32, z: u32, sun: u8, block: u8) {
        if x >= self.size || y >= self.size || z >= self.size {
            return;
        }
        let idx = self.index(x, y, z);
        self.light_levels[idx] = ((sun & 0x0F) << 4) | (block & 0x0F);
    }

    pub fn clear_light(&mut self) {
        for val in self.light_levels.iter_mut() {
            *val = 0;
        }
    }

    fn index(&self, x: u32, y: u32, z: u32) -> usize {
        (x + y * self.size + z * self.size * self.size) as usize
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.voxels.iter().all(|v| v.is_air())
    }

    /// Check if a face of this chunk is fully solid
    /// face_index: 0=+X, 1=-X, 2=+Y, 3=-Y, 4=+Z, 5=-Z
    pub fn is_face_solid(&self, face_index: usize) -> bool {
        let s = self.size;
        match face_index {
            0 => (0..s).all(|z| (0..s).all(|y| !self.get_voxel(s-1, y, z).voxel_type.is_transparent())),
            1 => (0..s).all(|z| (0..s).all(|y| !self.get_voxel(0, y, z).voxel_type.is_transparent())),
            2 => (0..s).all(|z| (0..s).all(|x| !self.get_voxel(x, s-1, z).voxel_type.is_transparent())),
            3 => (0..s).all(|z| (0..s).all(|x| !self.get_voxel(x, 0, z).voxel_type.is_transparent())),
            4 => (0..s).all(|y| (0..s).all(|x| !self.get_voxel(x, y, s-1).voxel_type.is_transparent())),
            5 => (0..s).all(|y| (0..s).all(|x| !self.get_voxel(x, y, 0).voxel_type.is_transparent())),
            _ => false,
        }
    }

    fn face_light_factor(&self, x: u32, y: u32, z: u32, face: Face, neighbors: &[Option<&Chunk>; 6]) -> [f32; 4] {
        let (adj_x, adj_y, adj_z) = match face {
            Face::PosX => (x as i32 + 1, y as i32, z as i32),
            Face::NegX => (x as i32 - 1, y as i32, z as i32),
            Face::PosY => (x as i32, y as i32 + 1, z as i32),
            Face::NegY => (x as i32, y as i32 - 1, z as i32),
            Face::PosZ => (x as i32, y as i32, z as i32 + 1),
            Face::NegZ => (x as i32, y as i32, z as i32 - 1),
        };
        let base_light = self.sample_light_at(adj_x, adj_y, adj_z, neighbors);
        let corner_offsets = self.get_corner_offsets(face);
        let mut corner_lights = [0.0f32; 4];
        for (i, (dx, dy, dz)) in corner_offsets.iter().enumerate() {
            let mut total_light = base_light as f32;
            let edge1 = self.sample_light_at(adj_x + dx[0], adj_y + dy[0], adj_z + dz[0], neighbors);
            let edge2 = self.sample_light_at(adj_x + dx[1], adj_y + dy[1], adj_z + dz[1], neighbors);
            let corner = self.sample_light_at(adj_x + dx[0] + dx[1], adj_y + dy[0] + dy[1], adj_z + dz[0] + dz[1], neighbors);
            total_light += edge1 as f32 + edge2 as f32 + corner as f32;
            corner_lights[i] = (total_light / 60.0).max(0.1);
        }
        corner_lights
    }

    fn get_corner_offsets(&self, face: Face) -> [([i32; 2], [i32; 2], [i32; 2]); 4] {
        match face {
            Face::PosX | Face::NegX => [
                ([0, -1], [0, 0], [0, -1]),
                ([0, 1], [0, 0], [0, -1]),
                ([0, 1], [0, 0], [0, 1]),
                ([0, -1], [0, 0], [0, 1]),
            ],
            Face::PosY | Face::NegY => [
                ([-1, 0], [0, 0], [0, -1]),
                ([-1, 0], [0, 0], [0, 1]),
                ([1, 0], [0, 0], [0, 1]),
                ([1, 0], [0, 0], [0, -1]),
            ],
            Face::PosZ | Face::NegZ => [
                ([1, 0], [0, -1], [0, 0]),
                ([1, 0], [0, 1], [0, 0]),
                ([-1, 0], [0, 1], [0, 0]),
                ([-1, 0], [0, -1], [0, 0]),
            ],
        }
    }

    fn sample_light_at(&self, x: i32, y: i32, z: i32, neighbors: &[Option<&Chunk>; 6]) -> u8 {
        let size = self.size as i32;
        if x >= 0 && x < size && y >= 0 && y < size && z >= 0 && z < size {
            let (sun, block) = self.get_light(x as u32, y as u32, z as u32);
            return sun.max(block);
        }
        if x >= size {
            if let Some(n) = neighbors[0] { let (s, b) = n.get_light(0, y.clamp(0, size-1) as u32, z.clamp(0, size-1) as u32); return s.max(b); }
            return 15;
        }
        if x < 0 {
            if let Some(n) = neighbors[1] { let (s, b) = n.get_light(size as u32 - 1, y.clamp(0, size-1) as u32, z.clamp(0, size-1) as u32); return s.max(b); }
            return 15;
        }
        if y >= size {
            if let Some(n) = neighbors[2] { let (s, b) = n.get_light(x.clamp(0, size-1) as u32, 0, z.clamp(0, size-1) as u32); return s.max(b); }
            return 15;
        }
        if y < 0 {
            if let Some(n) = neighbors[3] { let (s, b) = n.get_light(x.clamp(0, size-1) as u32, size as u32 - 1, z.clamp(0, size-1) as u32); return s.max(b); }
            return 0;
        }
        if z >= size {
            if let Some(n) = neighbors[4] { let (s, b) = n.get_light(x.clamp(0, size-1) as u32, y.clamp(0, size-1) as u32, 0); return s.max(b); }
            return 15;
        }
        if z < 0 {
            if let Some(n) = neighbors[5] { let (s, b) = n.get_light(x.clamp(0, size-1) as u32, y.clamp(0, size-1) as u32, size as u32 - 1); return s.max(b); }
            return 15;
        }
        15
    }

    /// Generate mesh with greedy meshing optimization
    pub fn generate_mesh_with_neighbors(
        &self,
        neighbors: &[Option<&Chunk>; 6],
    ) -> (Vec<ChunkVertex>, Vec<u32>) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let size = self.size;

        for face_dir in 0..6u8 {
            let face = Face::from_index(face_dir);
            for d in 0..size {
                let mut mask: Vec<Option<(VoxelType, u32, [f32; 4])>> = vec![None; (size * size) as usize];
                for v in 0..size {
                    for u in 0..size {
                        let (x, y, z) = face.map_coords(d, u, v);
                        let voxel = self.get_voxel(x, y, z);
                        if voxel.is_air() { continue; }
                        let visible = self.is_face_visible(x, y, z, face, size, neighbors);
                        if visible {
                            let fdir = match face { Face::PosY => FaceDir::Top, Face::NegY => FaceDir::Bottom, _ => FaceDir::Side };
                            let tex_idx = texture::texture_index(voxel.voxel_type, fdir);
                            let light = self.face_light_factor(x, y, z, face, neighbors);
                            mask[(u + v * size) as usize] = Some((voxel.voxel_type, tex_idx, light));
                        }
                    }
                }
                // Greedy merge
                let mut visited = vec![false; (size * size) as usize];
                for v in 0..size {
                    for u in 0..size {
                        let idx = (u + v * size) as usize;
                        if visited[idx] || mask[idx].is_none() { continue; }
                        let (vtype, tex_idx, light) = mask[idx].unwrap();
                        visited[idx] = true;
                        let mut width = 1u32;
                        while u + width < size {
                            let ni = ((u + width) + v * size) as usize;
                            if visited[ni] { break; }
                            match mask[ni] {
                                Some((nt, ntex, nl)) if nt == vtype && ntex == tex_idx && lights_similar(&light, &nl) => width += 1,
                                _ => break,
                            }
                        }
                        let mut height = 1u32;
                        'outer: while v + height < size {
                            for du in 0..width {
                                let ci = ((u + du) + (v + height) * size) as usize;
                                if visited[ci] { break 'outer; }
                                match mask[ci] {
                                    Some((nt, ntex, nl)) if nt == vtype && ntex == tex_idx && lights_similar(&light, &nl) => {}
                                    _ => break 'outer,
                                }
                            }
                            height += 1;
                        }
                        for dv in 0..height {
                            for du in 0..width {
                                visited[((u + du) + (v + dv) * size) as usize] = true;
                            }
                        }
                        add_greedy_quad(&mut vertices, &mut indices, face, d, u, v, width, height, tex_idx, light);
                    }
                }
            }
        }
        (vertices, indices)
    }

    /// Generate LOD mesh (every 2nd voxel)
    pub fn generate_lod_mesh(&self, neighbors: &[Option<&Chunk>; 6]) -> (Vec<ChunkVertex>, Vec<u32>) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let size = self.size;
        let step = 2u32;
        for z in (0..size).step_by(step as usize) {
            for y in (0..size).step_by(step as usize) {
                for x in (0..size).step_by(step as usize) {
                    let voxel = self.get_voxel(x, y, z);
                    if voxel.is_air() { continue; }
                    let vtype = voxel.voxel_type;
                    for fi in 0..6u8 {
                        let face = Face::from_index(fi);
                        let visible = self.is_lod_face_visible(x, y, z, face, size, step, neighbors);
                        if visible {
                            let fdir = match face { Face::PosY => FaceDir::Top, Face::NegY => FaceDir::Bottom, _ => FaceDir::Side };
                            let tex_idx = texture::texture_index(vtype, fdir);
                            let light = self.face_light_factor(x, y, z, face, neighbors);
                            let pos = face.lod_position(x, y, z, step);
                            add_lod_quad(&mut vertices, &mut indices, face, pos, tex_idx, light, step as f32);
                        }
                    }
                }
            }
        }
        (vertices, indices)
    }

    fn is_face_visible(&self, x: u32, y: u32, z: u32, face: Face, size: u32, neighbors: &[Option<&Chunk>; 6]) -> bool {
        match face {
            Face::PosX => if x == size-1 { neighbors[0].map_or(true, |n| n.get_voxel(0, y, z).voxel_type.is_transparent()) } else { self.get_voxel(x+1, y, z).voxel_type.is_transparent() },
            Face::NegX => if x == 0 { neighbors[1].map_or(true, |n| n.get_voxel(size-1, y, z).voxel_type.is_transparent()) } else { self.get_voxel(x-1, y, z).voxel_type.is_transparent() },
            Face::PosY => if y == size-1 { neighbors[2].map_or(true, |n| n.get_voxel(x, 0, z).voxel_type.is_transparent()) } else { self.get_voxel(x, y+1, z).voxel_type.is_transparent() },
            Face::NegY => if y == 0 { neighbors[3].map_or(true, |n| n.get_voxel(x, size-1, z).voxel_type.is_transparent()) } else { self.get_voxel(x, y-1, z).voxel_type.is_transparent() },
            Face::PosZ => if z == size-1 { neighbors[4].map_or(true, |n| n.get_voxel(x, y, 0).voxel_type.is_transparent()) } else { self.get_voxel(x, y, z+1).voxel_type.is_transparent() },
            Face::NegZ => if z == 0 { neighbors[5].map_or(true, |n| n.get_voxel(x, y, size-1).voxel_type.is_transparent()) } else { self.get_voxel(x, y, z-1).voxel_type.is_transparent() },
        }
    }

    fn is_lod_face_visible(&self, x: u32, y: u32, z: u32, face: Face, size: u32, step: u32, neighbors: &[Option<&Chunk>; 6]) -> bool {
        match face {
            Face::PosX => if x+step >= size { neighbors[0].map_or(true, |n| n.get_voxel(0, y, z).voxel_type.is_transparent()) } else { self.get_voxel(x+step, y, z).voxel_type.is_transparent() },
            Face::NegX => if x == 0 { neighbors[1].map_or(true, |n| n.get_voxel(size-1, y, z).voxel_type.is_transparent()) } else { self.get_voxel(x.saturating_sub(step), y, z).voxel_type.is_transparent() },
            Face::PosY => if y+step >= size { neighbors[2].map_or(true, |n| n.get_voxel(x, 0, z).voxel_type.is_transparent()) } else { self.get_voxel(x, y+step, z).voxel_type.is_transparent() },
            Face::NegY => if y == 0 { neighbors[3].map_or(true, |n| n.get_voxel(x, size-1, z).voxel_type.is_transparent()) } else { self.get_voxel(x, y.saturating_sub(step), z).voxel_type.is_transparent() },
            Face::PosZ => if z+step >= size { neighbors[4].map_or(true, |n| n.get_voxel(x, y, 0).voxel_type.is_transparent()) } else { self.get_voxel(x, y, z+step).voxel_type.is_transparent() },
            Face::NegZ => if z == 0 { neighbors[5].map_or(true, |n| n.get_voxel(x, y, size-1).voxel_type.is_transparent()) } else { self.get_voxel(x, y, z.saturating_sub(step)).voxel_type.is_transparent() },
        }
    }
}

fn lights_similar(a: &[f32; 4], b: &[f32; 4]) -> bool {
    const T: f32 = 0.05;
    (a[0]-b[0]).abs() < T && (a[1]-b[1]).abs() < T && (a[2]-b[2]).abs() < T && (a[3]-b[3]).abs() < T
}

fn add_greedy_quad(
    vertices: &mut Vec<ChunkVertex>, indices: &mut Vec<u32>,
    face: Face, d: u32, u_start: u32, v_start: u32,
    width: u32, height: u32, tex_index: u32, light_values: [f32; 4],
) {
    let base_index = vertices.len() as u32;
    let (positions, normal) = face.greedy_positions(d, u_start, v_start, width, height);
    let uvs = [[0.0, 0.0], [0.0, height as f32], [width as f32, height as f32], [width as f32, 0.0]];
    for (i, pos) in positions.iter().enumerate() {
        vertices.push(ChunkVertex { position: *pos, normal, color: [1.0, 1.0, 1.0, 1.0], tex_coords: uvs[i], tex_index, light_level: light_values[i] });
    }
    indices.push(base_index); indices.push(base_index + 1); indices.push(base_index + 2);
    indices.push(base_index); indices.push(base_index + 2); indices.push(base_index + 3);
}

fn add_lod_quad(
    vertices: &mut Vec<ChunkVertex>, indices: &mut Vec<u32>,
    face: Face, pos: [f32; 3], tex_index: u32, light_values: [f32; 4], scale: f32,
) {
    let base_index = vertices.len() as u32;
    let (face_verts, normal) = face.vertices_and_normal_scaled(pos, scale);
    let uvs = [[0.0, 0.0], [0.0, scale], [scale, scale], [scale, 0.0]];
    for (i, vert_pos) in face_verts.iter().enumerate() {
        vertices.push(ChunkVertex { position: *vert_pos, normal, color: [1.0, 1.0, 1.0, 1.0], tex_coords: uvs[i], tex_index, light_level: light_values[i] });
    }
    indices.push(base_index); indices.push(base_index + 1); indices.push(base_index + 2);
    indices.push(base_index); indices.push(base_index + 2); indices.push(base_index + 3);
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ChunkVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
    pub tex_coords: [f32; 2],
    pub tex_index: u32,
    pub light_level: f32,
}

impl ChunkVertex {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ChunkVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
                wgpu::VertexAttribute { offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress, shader_location: 1, format: wgpu::VertexFormat::Float32x3 },
                wgpu::VertexAttribute { offset: std::mem::size_of::<[f32; 6]>() as wgpu::BufferAddress, shader_location: 2, format: wgpu::VertexFormat::Float32x4 },
                wgpu::VertexAttribute { offset: std::mem::size_of::<[f32; 10]>() as wgpu::BufferAddress, shader_location: 3, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: std::mem::size_of::<[f32; 12]>() as wgpu::BufferAddress, shader_location: 4, format: wgpu::VertexFormat::Uint32 },
                wgpu::VertexAttribute { offset: (std::mem::size_of::<[f32; 12]>() + std::mem::size_of::<u32>()) as wgpu::BufferAddress, shader_location: 5, format: wgpu::VertexFormat::Float32 },
            ],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Face { PosX, NegX, PosY, NegY, PosZ, NegZ }

impl Face {
    pub fn from_index(i: u8) -> Self {
        match i { 0 => Face::PosX, 1 => Face::NegX, 2 => Face::PosY, 3 => Face::NegY, 4 => Face::PosZ, _ => Face::NegZ }
    }

    fn map_coords(&self, d: u32, u: u32, v: u32) -> (u32, u32, u32) {
        match self { Face::PosX | Face::NegX => (d, v, u), Face::PosY | Face::NegY => (u, d, v), Face::PosZ | Face::NegZ => (u, v, d) }
    }

    fn lod_position(&self, x: u32, y: u32, z: u32, step: u32) -> [f32; 3] {
        match self {
            Face::PosX => [x as f32 + step as f32, y as f32, z as f32],
            Face::NegX => [x as f32, y as f32, z as f32],
            Face::PosY => [x as f32, y as f32 + step as f32, z as f32],
            Face::NegY => [x as f32, y as f32, z as f32],
            Face::PosZ => [x as f32, y as f32, z as f32 + step as f32],
            Face::NegZ => [x as f32, y as f32, z as f32],
        }
    }

    fn greedy_positions(&self, d: u32, u_s: u32, v_s: u32, w: u32, h: u32) -> ([[f32; 3]; 4], [f32; 3]) {
        let (us, vs, ue, ve) = (u_s as f32, v_s as f32, (u_s+w) as f32, (v_s+h) as f32);
        match self {
            Face::PosX => { let x = d as f32 + 1.0; ([[x,vs,us],[x,ve,us],[x,ve,ue],[x,vs,ue]], [1.0,0.0,0.0]) }
            Face::NegX => { let x = d as f32; ([[x,vs,ue],[x,ve,ue],[x,ve,us],[x,vs,us]], [-1.0,0.0,0.0]) }
            Face::PosY => { let y = d as f32 + 1.0; ([[us,y,vs],[us,y,ve],[ue,y,ve],[ue,y,vs]], [0.0,1.0,0.0]) }
            Face::NegY => { let y = d as f32; ([[ue,y,vs],[ue,y,ve],[us,y,ve],[us,y,vs]], [0.0,-1.0,0.0]) }
            Face::PosZ => { let z = d as f32 + 1.0; ([[ue,vs,z],[ue,ve,z],[us,ve,z],[us,vs,z]], [0.0,0.0,1.0]) }
            Face::NegZ => { let z = d as f32; ([[us,vs,z],[us,ve,z],[ue,ve,z],[ue,vs,z]], [0.0,0.0,-1.0]) }
        }
    }

    fn vertices_and_normal_scaled(&self, pos: [f32; 3], s: f32) -> ([[f32; 3]; 4], [f32; 3]) {
        let [x, y, z] = pos;
        match self {
            Face::PosX => ([[x,y,z],[x,y+s,z],[x,y+s,z+s],[x,y,z+s]], [1.0,0.0,0.0]),
            Face::NegX => ([[x,y,z+s],[x,y+s,z+s],[x,y+s,z],[x,y,z]], [-1.0,0.0,0.0]),
            Face::PosY => ([[x,y,z],[x,y,z+s],[x+s,y,z+s],[x+s,y,z]], [0.0,1.0,0.0]),
            Face::NegY => ([[x+s,y,z],[x+s,y,z+s],[x,y,z+s],[x,y,z]], [0.0,-1.0,0.0]),
            Face::PosZ => ([[x+s,y,z],[x+s,y+s,z],[x,y+s,z],[x,y,z]], [0.0,0.0,1.0]),
            Face::NegZ => ([[x,y,z],[x,y+s,z],[x+s,y+s,z],[x+s,y,z]], [0.0,0.0,-1.0]),
        }
    }
}

/// Manages all loaded chunks
pub struct ChunkManager {
    pub chunks: std::collections::HashMap<ChunkCoord, Chunk>,
    pub config: Ring
