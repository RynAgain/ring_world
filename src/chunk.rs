/// Chunk system for the ring world
/// Each chunk is a cubic section of voxels positioned on the ring

use crate::voxel::{Voxel, VoxelType, FaceDir};
use crate::texture;
use crate::ring_world::{ChunkCoord, RingPosition, RingWorldConfig};

/// Minimum normalized per-vertex light factor applied on every vertex-light
/// path (greedy, LOD, cross). Even a fully-unlit face is rendered at this
/// fraction of its texture brightness so terrain faces are never pure black
/// and always read as "textured, just dim". Raised from the old 0.1 (which
/// was too dark to read) to 0.28.
pub const MIN_LIGHT_FACTOR: f32 = 0.28;

/// Default light level (full sunlight, 0-15) used for a light sample that
/// falls in a not-yet-loaded neighbor chunk. Using the same value for ALL six
/// directions (including -Y) keeps the corner-average symmetric and avoids the
/// dark seams that a directional asymmetry (e.g. -Y => 0) produced on freshly
/// exposed cliff/canyon side faces.
const UNLOADED_NEIGHBOR_LIGHT: u8 = 15;

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

    /// Check if a face of this chunk is fully solid (opaque blocks on entire face)
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
            corner_lights[i] = (total_light / 60.0).max(MIN_LIGHT_FACTOR);
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
        // For every out-of-bounds direction, when the neighbor chunk is loaded
        // we sample its real edge light; when it is NOT loaded we fall back to a
        // single shared default (UNLOADED_NEIGHBOR_LIGHT) for ALL six directions.
        // The -Y case previously returned 0 here, which biased freshly exposed
        // downward-facing / cliff-side samples toward black and produced dark
        // seams at chunk borders. Treating an unloaded neighbor as "unknown =
        // full sunlight" symmetrically removes that asymmetry.
        if x >= size {
            if let Some(n) = neighbors[0] {
                let (s, b) = n.get_light(0, y.clamp(0, size-1) as u32, z.clamp(0, size-1) as u32);
                return s.max(b);
            }
            return UNLOADED_NEIGHBOR_LIGHT;
        }
        if x < 0 {
            if let Some(n) = neighbors[1] {
                let (s, b) = n.get_light(size as u32 - 1, y.clamp(0, size-1) as u32, z.clamp(0, size-1) as u32);
                return s.max(b);
            }
            return UNLOADED_NEIGHBOR_LIGHT;
        }
        if y >= size {
            if let Some(n) = neighbors[2] {
                let (s, b) = n.get_light(x.clamp(0, size-1) as u32, 0, z.clamp(0, size-1) as u32);
                return s.max(b);
            }
            return UNLOADED_NEIGHBOR_LIGHT;
        }
        if y < 0 {
            if let Some(n) = neighbors[3] {
                let (s, b) = n.get_light(x.clamp(0, size-1) as u32, size as u32 - 1, z.clamp(0, size-1) as u32);
                return s.max(b);
            }
            return UNLOADED_NEIGHBOR_LIGHT;
        }
        if z >= size {
            if let Some(n) = neighbors[4] {
                let (s, b) = n.get_light(x.clamp(0, size-1) as u32, y.clamp(0, size-1) as u32, 0);
                return s.max(b);
            }
            return UNLOADED_NEIGHBOR_LIGHT;
        }
        if z < 0 {
            if let Some(n) = neighbors[5] {
                let (s, b) = n.get_light(x.clamp(0, size-1) as u32, y.clamp(0, size-1) as u32, size as u32 - 1);
                return s.max(b);
            }
            return UNLOADED_NEIGHBOR_LIGHT;
        }
        UNLOADED_NEIGHBOR_LIGHT
    }

    /// Generate mesh with greedy meshing optimization.
    ///
    /// Returns `(opaque_vertices, opaque_indices)` for backward compatibility.
    /// Water (translucent) geometry is produced separately by
    /// [`generate_mesh_split`]; callers that need the two-pass split should use
    /// that instead.
    pub fn generate_mesh_with_neighbors(
        &self,
        neighbors: &[Option<&Chunk>; 6],
    ) -> (Vec<ChunkVertex>, Vec<u32>) {
        let mesh = self.generate_mesh_split(neighbors);
        (mesh.opaque_vertices, mesh.opaque_indices)
    }

    /// Generate the full chunk mesh, splitting opaque/cutout geometry from
    /// translucent water geometry so the renderer can draw them in two passes
    /// (Pass A: opaque, depth-write on; Pass B: water, depth-write off).
    pub fn generate_mesh_split(
        &self,
        neighbors: &[Option<&Chunk>; 6],
    ) -> ChunkMeshData {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut water_vertices = Vec::new();
        let mut water_indices = Vec::new();
        let size = self.size;

        for face_dir in 0..6u8 {
            let face = Face::from_index(face_dir);

            for d in 0..size {
                let mut mask: Vec<Option<(VoxelType, u32, [f32; 4], [f32; 4])>> = vec![None; (size * size) as usize];

                for v in 0..size {
                    for u in 0..size {
                        let (x, y, z) = face.map_coords(d, u, v);
                        let voxel = self.get_voxel(x, y, z);
                        if voxel.is_air() { continue; }
                        // Cross-rendered decorations (tall grass, flowers, etc.)
                        // are emitted separately as X-shaped billboards and must
                        // never participate in the greedy cube meshing.
                        if is_cross_render(voxel.voxel_type) { continue; }

                        let visible = self.is_face_visible(x, y, z, face, size, neighbors);
                        if visible {
                            let face_dir_enum = match face {
                                Face::PosY => FaceDir::Top,
                                Face::NegY => FaceDir::Bottom,
                                _ => FaceDir::Side,
                            };
                            let tex_idx = texture::texture_index(voxel.voxel_type, face_dir_enum);
                            let light = self.face_light_factor(x, y, z, face, neighbors);
                            let color = face_tint(voxel.voxel_type, face_dir_enum);
                            mask[(u + v * size) as usize] = Some((voxel.voxel_type, tex_idx, light, color));
                        }
                    }
                }

                // Greedy merge
                let mut visited = vec![false; (size * size) as usize];
                for v in 0..size {
                    for u in 0..size {
                        let idx = (u + v * size) as usize;
                        if visited[idx] || mask[idx].is_none() { continue; }

                        let (vtype, tex_idx, light, color) = mask[idx].unwrap();
                        visited[idx] = true;

                        let mut width = 1u32;
                        while u + width < size {
                            let ni = ((u + width) + v * size) as usize;
                            if visited[ni] { break; }
                            match mask[ni] {
                                Some((nt, ntex, nl, _)) if nt == vtype && ntex == tex_idx && lights_similar(&light, &nl) => {
                                    width += 1;
                                }
                                _ => break,
                            }
                        }

                        let mut height = 1u32;
                        'outer: while v + height < size {
                            for du in 0..width {
                                let ci = ((u + du) + (v + height) * size) as usize;
                                if visited[ci] { break 'outer; }
                                match mask[ci] {
                                    Some((nt, ntex, nl, _)) if nt == vtype && ntex == tex_idx && lights_similar(&light, &nl) => {}
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

                        // Route translucent water faces to the separate water
                        // buffer so they can be drawn in the depth-write-off pass.
                        let (vbuf, ibuf) = if is_translucent(vtype) {
                            (&mut water_vertices, &mut water_indices)
                        } else {
                            (&mut vertices, &mut indices)
                        };
                        add_greedy_quad(vbuf, ibuf, face, d, u, v, width, height, tex_idx, light, color, is_alpha_tested(vtype));
                    }
                }
            }
        }

        // Separate pass: emit cross-shaped billboards for decorative voxels
        // (tall grass, flowers, mushrooms, vines). These are skipped by the
        // greedy cube mesher above and rendered as two intersecting,
        // double-sided vertical quads forming an X within the voxel cell.
        for z in 0..size {
            for y in 0..size {
                for x in 0..size {
                    let voxel = self.get_voxel(x, y, z);
                    if voxel.is_air() { continue; }
                    if !is_cross_render(voxel.voxel_type) { continue; }

                    let tex_idx = texture::texture_index(voxel.voxel_type, FaceDir::Side);
                    // Sample the light level at this voxel's own cell.
                    let (sun, block) = self.get_light(x, y, z);
                    let light = ((sun.max(block) as f32) / 15.0).max(MIN_LIGHT_FACTOR);
                    let color = face_tint(voxel.voxel_type, FaceDir::Side);
                    // Cross-render plants are always alpha-tested (their textures
                    // have transparent holes around the billboard).
                    add_cross_quads(&mut vertices, &mut indices, x, y, z, tex_idx, light, color, is_alpha_tested(voxel.voxel_type));
                }
            }
        }

        ChunkMeshData {
            opaque_vertices: vertices,
            opaque_indices: indices,
            water_vertices,
            water_indices,
        }
    }

    /// Non-greedy variant of [`generate_mesh_split`]: emits EVERY visible block
    /// face as its own 1x1 quad with no merging. Used by the F7 A/B-test toggle
    /// to determine whether the greedy merge step is responsible for dropped
    /// faces. Visibility / culling / texturing / lighting are identical to the
    /// greedy path; only the merge is removed.
    pub fn generate_mesh_split_no_greedy(
        &self,
        neighbors: &[Option<&Chunk>; 6],
    ) -> ChunkMeshData {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut water_vertices = Vec::new();
        let mut water_indices = Vec::new();
        let size = self.size;

        for face_dir in 0..6u8 {
            let face = Face::from_index(face_dir);
            for d in 0..size {
                for v in 0..size {
                    for u in 0..size {
                        let (x, y, z) = face.map_coords(d, u, v);
                        let voxel = self.get_voxel(x, y, z);
                        if voxel.is_air() { continue; }
                        if is_cross_render(voxel.voxel_type) { continue; }
                        if !self.is_face_visible(x, y, z, face, size, neighbors) { continue; }

                        let face_dir_enum = match face {
                            Face::PosY => FaceDir::Top,
                            Face::NegY => FaceDir::Bottom,
                            _ => FaceDir::Side,
                        };
                        let tex_idx = texture::texture_index(voxel.voxel_type, face_dir_enum);
                        let light = self.face_light_factor(x, y, z, face, neighbors);
                        let color = face_tint(voxel.voxel_type, face_dir_enum);
                        let (vbuf, ibuf) = if is_translucent(voxel.voxel_type) {
                            (&mut water_vertices, &mut water_indices)
                        } else {
                            (&mut vertices, &mut indices)
                        };
                        // width = height = 1: one quad per face cell.
                        add_greedy_quad(vbuf, ibuf, face, d, u, v, 1, 1, tex_idx, light, color, is_alpha_tested(voxel.voxel_type));
                    }
                }
            }
        }

        // Cross-render decorations (same as the greedy path).
        for z in 0..size {
            for y in 0..size {
                for x in 0..size {
                    let voxel = self.get_voxel(x, y, z);
                    if voxel.is_air() { continue; }
                    if !is_cross_render(voxel.voxel_type) { continue; }
                    let tex_idx = texture::texture_index(voxel.voxel_type, FaceDir::Side);
                    let (sun, block) = self.get_light(x, y, z);
                    let light = ((sun.max(block) as f32) / 15.0).max(MIN_LIGHT_FACTOR);
                    let color = face_tint(voxel.voxel_type, FaceDir::Side);
                    add_cross_quads(&mut vertices, &mut indices, x, y, z, tex_idx, light, color, is_alpha_tested(voxel.voxel_type));
                }
            }
        }

        ChunkMeshData {
            opaque_vertices: vertices,
            opaque_indices: indices,
            water_vertices,
            water_indices,
        }
    }

    /// Sample a 2x2x2 block of voxels at block-grid coordinates (bx, by, bz)
    /// (each ranges over 0..size/step). Returns the most representative solid
    /// (non-air) voxel type in the block, or None if the whole block is air.
    /// This makes the LOD mesh treat each 2-voxel cell as a single super-voxel,
    /// which avoids the "comb"/spike artifacts caused by single-voxel sampling.
    fn lod_block_type(&self, bx: u32, by: u32, bz: u32, step: u32) -> Option<VoxelType> {
        let mut found: Option<VoxelType> = None;
        for dz in 0..step {
            for dy in 0..step {
                for dx in 0..step {
                    let v = self.get_voxel(bx * step + dx, by * step + dy, bz * step + dz);
                    if !v.is_air() {
                        // Prefer an opaque block for a more "solid" appearance; fall
                        // back to whatever non-air voxel we find first.
                        if !v.voxel_type.is_transparent() {
                            return Some(v.voxel_type);
                        }
                        if found.is_none() {
                            found = Some(v.voxel_type);
                        }
                    }
                }
            }
        }
        found
    }

    /// Whether a 2x2x2 super-voxel is FULLY solid: every one of its `step^3`
    /// constituent voxels is a non-transparent (opaque) block.
    ///
    /// This is the correct test for LOD OCCLUSION. The previous code reused
    /// `lod_block_type(..).is_some_opaque` (i.e. "ANY opaque voxel in the cell"),
    /// which over-occludes at cliffs: a half-filled super-voxel at a cliff edge
    /// (solid below, air above) counted as solid and CULLED the neighbor
    /// super-voxel's face pointing at it — leaving a see-through hole in the
    /// distant (LOD) cliff. A partially-filled cell does NOT fully cover the
    /// shared face, so it must NOT occlude. Rendering still uses the
    /// "any non-air" rule (`lod_block_type`) so the low-poly block is still
    /// emitted; only the occlusion test is tightened.
    fn lod_block_full_solid(&self, bx: u32, by: u32, bz: u32, step: u32) -> bool {
        for dz in 0..step {
            for dy in 0..step {
                for dx in 0..step {
                    let v = self.get_voxel(bx * step + dx, by * step + dy, bz * step + dz);
                    if v.voxel_type.is_transparent() {
                        // Air or any transparent voxel => not fully solid.
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Sample a 2x2x2 block in a neighbor chunk (block-grid coords may be outside
    /// this chunk's range). Returns whether that block FULLY occludes the shared
    /// face — i.e. the super-voxel is entirely opaque (see
    /// [`lod_block_full_solid`]). Used for LOD face culling across the
    /// half-resolution grid.
    fn lod_block_solid_at(&self, bx: i32, by: i32, bz: i32, step: u32, neighbors: &[Option<&Chunk>; 6]) -> bool {
        let blocks = (self.size / step) as i32;
        // Within this chunk.
        if bx >= 0 && bx < blocks && by >= 0 && by < blocks && bz >= 0 && bz < blocks {
            return self.lod_block_full_solid(bx as u32, by as u32, bz as u32, step);
        }
        // In a neighbor chunk: map the out-of-range block coordinate to the
        // corresponding edge block of the neighbor and sample there.
        let neighbor_and_block = |idx: usize, nbx: i32, nby: i32, nbz: i32| -> bool {
            match neighbors[idx] {
                Some(n) => n.lod_block_full_solid(nbx as u32, nby as u32, nbz as u32, step),
                // No neighbor loaded => treat as empty so the boundary face renders.
                None => false,
            }
        };
        let last = blocks - 1;
        if bx >= blocks {
            return neighbor_and_block(0, 0, by.clamp(0, last), bz.clamp(0, last));
        }
        if bx < 0 {
            return neighbor_and_block(1, last, by.clamp(0, last), bz.clamp(0, last));
        }
        if by >= blocks {
            return neighbor_and_block(2, bx.clamp(0, last), 0, bz.clamp(0, last));
        }
        if by < 0 {
            return neighbor_and_block(3, bx.clamp(0, last), last, bz.clamp(0, last));
        }
        if bz >= blocks {
            return neighbor_and_block(4, bx.clamp(0, last), by.clamp(0, last), 0);
        }
        if bz < 0 {
            return neighbor_and_block(5, bx.clamp(0, last), by.clamp(0, last), last);
        }
        false
    }

    /// Generate LOD mesh (opaque-only convenience wrapper). See
    /// [`generate_lod_mesh_split`].
    pub fn generate_lod_mesh(&self, neighbors: &[Option<&Chunk>; 6]) -> (Vec<ChunkVertex>, Vec<u32>) {
        let mesh = self.generate_lod_mesh_split(neighbors);
        (mesh.opaque_vertices, mesh.opaque_indices)
    }

    /// Generate LOD mesh by treating each 2x2x2 voxel cell as one super-voxel.
    /// Faces are emitted only when the adjacent super-voxel is not solid, which
    /// produces a watertight low-poly mesh without combing artifacts.
    ///
    /// Like [`generate_mesh_split`], translucent water faces are routed into the
    /// separate water buffer for the depth-write-off pass.
    pub fn generate_lod_mesh_split(&self, neighbors: &[Option<&Chunk>; 6]) -> ChunkMeshData {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut water_vertices = Vec::new();
        let mut water_indices = Vec::new();
        let step = 2u32;
        let blocks = self.size / step;
        let s = step as f32;

        for bz in 0..blocks {
            for by in 0..blocks {
                for bx in 0..blocks {
                    let vtype = match self.lod_block_type(bx, by, bz, step) {
                        Some(t) => t,
                        None => continue,
                    };

                    // World-space origin of this super-voxel (in voxel units).
                    let x = (bx * step) as f32;
                    let y = (by * step) as f32;
                    let z = (bz * step) as f32;
                    // Representative voxel coord for lighting (center-ish of block).
                    let lx = bx * step;
                    let ly = by * step;
                    let lz = bz * step;

                    let faces_and_dirs: [(Face, FaceDir, [f32; 3], (i32, i32, i32)); 6] = [
                        (Face::PosX, FaceDir::Side, [x + s, y, z], (1, 0, 0)),
                        (Face::NegX, FaceDir::Side, [x, y, z], (-1, 0, 0)),
                        (Face::PosY, FaceDir::Top, [x, y + s, z], (0, 1, 0)),
                        (Face::NegY, FaceDir::Bottom, [x, y, z], (0, -1, 0)),
                        (Face::PosZ, FaceDir::Side, [x, y, z + s], (0, 0, 1)),
                        (Face::NegZ, FaceDir::Side, [x, y, z], (0, 0, -1)),
                    ];

                    for (face, fdir, pos, (dbx, dby, dbz)) in &faces_and_dirs {
                        let neighbor_solid = self.lod_block_solid_at(
                            bx as i32 + dbx,
                            by as i32 + dby,
                            bz as i32 + dbz,
                            step,
                            neighbors,
                        );
                        // Render the face when the adjacent super-voxel is not solid,
                        // OR when this super-voxel itself is transparent (e.g. water)
                        // so transparent surfaces still show.
                        if !neighbor_solid {
                            let tex_idx = texture::texture_index(vtype, *fdir);
                            let light = self.face_light_factor(lx, ly, lz, *face, neighbors);
                            let color = face_tint(vtype, *fdir);
                            let (vbuf, ibuf) = if is_translucent(vtype) {
                                (&mut water_vertices, &mut water_indices)
                            } else {
                                (&mut vertices, &mut indices)
                            };
                            add_lod_quad(vbuf, ibuf, *face, *pos, tex_idx, light, s, color, is_alpha_tested(vtype));
                        }
                    }
                }
            }
        }

        ChunkMeshData {
            opaque_vertices: vertices,
            opaque_indices: indices,
            water_vertices,
            water_indices,
        }
    }

    /// Whether the face of the voxel at (x,y,z) toward `face` should be emitted.
    ///
    /// A face is emitted only when the neighbor does NOT occlude it (see
    /// [`occludes`]). The current voxel type is sampled here so the occlusion
    /// rule can compare same-vs-different translucent types (e.g. water/water
    /// culls its shared interior face, but water-vs-leaves does not). The
    /// boundary arms feed the REAL neighbor voxel (from the adjacent chunk, when
    /// loaded) into the same test; true world edges (neighbor chunk == None)
    /// keep drawing the face as before.
    fn is_face_visible(&self, x: u32, y: u32, z: u32, face: Face, size: u32, neighbors: &[Option<&Chunk>; 6]) -> bool {
        let current = self.get_voxel(x, y, z).voxel_type;
        let neighbor_type = match face {
            Face::PosX => {
                if x == size - 1 { neighbors[0].map(|n| n.get_voxel(0, y, z).voxel_type) }
                else { Some(self.get_voxel(x + 1, y, z).voxel_type) }
            }
            Face::NegX => {
                if x == 0 { neighbors[1].map(|n| n.get_voxel(size - 1, y, z).voxel_type) }
                else { Some(self.get_voxel(x - 1, y, z).voxel_type) }
            }
            Face::PosY => {
                if y == size - 1 { neighbors[2].map(|n| n.get_voxel(x, 0, z).voxel_type) }
                else { Some(self.get_voxel(x, y + 1, z).voxel_type) }
            }
            Face::NegY => {
                if y == 0 { neighbors[3].map(|n| n.get_voxel(x, size - 1, z).voxel_type) }
                else { Some(self.get_voxel(x, y - 1, z).voxel_type) }
            }
            Face::PosZ => {
                if z == size - 1 { neighbors[4].map(|n| n.get_voxel(x, y, 0).voxel_type) }
                else { Some(self.get_voxel(x, y, z + 1).voxel_type) }
            }
            Face::NegZ => {
                if z == 0 { neighbors[5].map(|n| n.get_voxel(x, y, size - 1).voxel_type) }
                else { Some(self.get_voxel(x, y, z - 1).voxel_type) }
            }
        };
        match neighbor_type {
            // Unloaded interior neighbor / true world edge: keep drawing the
            // face (matches the previous behavior at the loaded-area frontier;
            // Issue 1's re-mesh-on-neighbor-ready handles the interior case).
            None => true,
            Some(neighbor) => !occludes(current, neighbor),
        }
    }

}

/// Check if two light arrays are similar enough to merge in greedy meshing
fn lights_similar(a: &[f32; 4], b: &[f32; 4]) -> bool {
    const THRESHOLD: f32 = 0.05;
    (a[0] - b[0]).abs() < THRESHOLD
        && (a[1] - b[1]).abs() < THRESHOLD
        && (a[2] - b[2]).abs() < THRESHOLD
        && (a[3] - b[3]).abs() < THRESHOLD
}

/// Opaque + translucent geometry produced by [`Chunk::generate_mesh_split`].
/// Opaque/cutout geometry is drawn in Pass A (depth write on); water geometry
/// in Pass B (depth write off, no back-face culling).
pub struct ChunkMeshData {
    pub opaque_vertices: Vec<ChunkVertex>,
    pub opaque_indices: Vec<u32>,
    pub water_vertices: Vec<ChunkVertex>,
    pub water_indices: Vec<u32>,
}

/// Whether a voxel type is genuinely translucent (alpha-blended), as opposed to
/// opaque or alpha-cutout. Only these route to the depth-write-off water pass
/// and carry a sub-1.0 vertex alpha. Currently this is just water.
pub fn is_translucent(voxel_type: VoxelType) -> bool {
    matches!(voxel_type, VoxelType::Water)
}

/// Whether a voxel type's faces use ALPHA-CUTOUT rendering, i.e. the fragment
/// shader is allowed to `discard` its (nearly) transparent texels. This is true
/// only for leaves and the cross-render plants (tall grass, flowers, mushrooms,
/// vines), whose textures intentionally contain fully-transparent holes.
///
/// Solid/opaque blocks (grass, dirt, stone, wood, snow, ore, crafting blocks,
/// ...) return false: their faces must NEVER be discarded, so they can never
/// render see-through. This is the per-face flag the shader keys its discard on
/// (see `alpha_tested` in shader.wgsl) and is the root-cause fix for the
/// "grass sides are transparent / show nothing" bug.
pub fn is_alpha_tested(voxel_type: VoxelType) -> bool {
    is_cross_render(voxel_type) || matches!(voxel_type, VoxelType::Leaves)
}

/// Face-culling occlusion rule (Issue 2).
///
/// Returns true when `neighbor` fully hides the `current` voxel's face toward
/// it (so the face should be CULLED). A face is EMITTED when this returns false.
///
/// Rules:
/// - `neighbor == Air` → does not occlude (emit).
/// - cross-render plant neighbor (TallGrass/Flower/Mushroom/Vine) → never
///   occludes (emit), since billboards don't cover the face.
/// - fully opaque neighbor (`is_solid && !is_transparent`) → occludes (cull).
/// - same translucent type as current (water/water, leaves/leaves) → occludes
///   the shared interior face (cull) so we don't draw/blend interior planes.
/// - different translucent type than current → does not occlude (emit), so a
///   solid block's face toward water/leaves is drawn and water-vs-leaves shows.
pub fn occludes(current: VoxelType, neighbor: VoxelType) -> bool {
    if neighbor == VoxelType::Air {
        return false;
    }
    if is_cross_render(neighbor) {
        return false;
    }
    // Fully opaque (occludes everything).
    if neighbor.is_solid() && !neighbor.is_transparent() {
        return true;
    }
    // Translucent neighbor: occludes only the shared interior face of a
    // same-type translucent volume.
    if neighbor.is_transparent() {
        return neighbor == current;
    }
    // Non-solid, non-transparent (shouldn't normally occur for meshed blocks):
    // treat as non-occluding to be safe.
    false
}

/// The vertex tint for a face, derived from `VoxelType::face_color()`.
///
/// Opaque blocks have their alpha clamped to 1.0 so a stray sub-1.0 alpha in
/// `face_color()` can never make an opaque block translucent (and trip the
/// shader's alpha-cutout discard). Only genuinely translucent types (water)
/// keep their authored alpha so the two-pass blending works.
fn face_tint(voxel_type: VoxelType, face: FaceDir) -> [f32; 4] {
    let mut c = voxel_type.face_color(face);
    if !is_translucent(voxel_type) {
        c[3] = 1.0;
    }
    c
}

/// Whether a voxel type should be rendered as a cross-shaped billboard (two
/// intersecting vertical quads) instead of a full cube. These are non-solid
/// decorations: tall grass, flowers, mushrooms, and vines.
///
/// Note: Vine is treated as a cross here for simplicity. A wall-attached flat
/// quad would be more faithful, but the cross billboard is a correct and simple
/// option that avoids the "texture stamped on a box" artifact.
pub fn is_cross_render(voxel_type: VoxelType) -> bool {
    matches!(
        voxel_type,
        VoxelType::TallGrass | VoxelType::Flower | VoxelType::Mushroom | VoxelType::Vine
    )
}

/// Add two intersecting, double-sided vertical quads forming an X within the
/// voxel cell at local coordinates (x, y, z). Local Y is "up" (radial), so the
/// quads span the full height in Y and cross diagonally in the X/Z plane.
///
/// Each quad is emitted twice (with opposite winding) so it is visible from
/// both sides despite back-face culling. Full UV (0,0)-(1,1) is used.
fn add_cross_quads(
    vertices: &mut Vec<ChunkVertex>,
    indices: &mut Vec<u32>,
    x: u32, y: u32, z: u32,
    tex_index: u32,
    light: f32,
    color: [f32; 4],
    alpha_tested: bool,
) {
    let alpha_tested = alpha_tested as u32;
    let fx = x as f32;
    let fy = y as f32;
    let fz = z as f32;
    let y0 = fy;
    let y1 = fy + 1.0;

    // Quad A spans the diagonal from (x, *, z) to (x+1, *, z+1).
    // Quad B spans the other diagonal from (x, *, z+1) to (x+1, *, z).
    let diagonals: [([f32; 2], [f32; 2], [f32; 3]); 2] = [
        // (start_xz, end_xz, normal)
        ([fx, fz], [fx + 1.0, fz + 1.0], [0.70710677, 0.0, -0.70710677]),
        ([fx, fz + 1.0], [fx + 1.0, fz], [0.70710677, 0.0, 0.70710677]),
    ];

    for (start, end, normal) in diagonals.iter() {
        let (sx, sz) = (start[0], start[1]);
        let (ex, ez) = (end[0], end[1]);

        // Four corners of the vertical quad (bottom-start, top-start,
        // top-end, bottom-end).
        let p0 = [sx, y0, sz];
        let p1 = [sx, y1, sz];
        let p2 = [ex, y1, ez];
        let p3 = [ex, y0, ez];
        let uvs: [[f32; 2]; 4] = [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]];
        let positions = [p0, p1, p2, p3];

        // Front side.
        let base = vertices.len() as u32;
        for i in 0..4 {
            vertices.push(ChunkVertex {
                position: positions[i],
                normal: *normal,
                color,
                tex_coords: uvs[i],
                tex_index,
                light_level: light,
                alpha_tested,
            });
        }
        indices.push(base);
        indices.push(base + 1);
        indices.push(base + 2);
        indices.push(base);
        indices.push(base + 2);
        indices.push(base + 3);

        // Back side (opposite winding + flipped normal) so the quad is
        // visible from behind too.
        let back_normal = [-normal[0], -normal[1], -normal[2]];
        let base = vertices.len() as u32;
        for i in 0..4 {
            vertices.push(ChunkVertex {
                position: positions[i],
                normal: back_normal,
                color,
                tex_coords: uvs[i],
                tex_index,
                light_level: light,
                alpha_tested,
            });
        }
        indices.push(base);
        indices.push(base + 2);
        indices.push(base + 1);
        indices.push(base);
        indices.push(base + 3);
        indices.push(base + 2);
    }
}

/// Add a greedy-merged quad to the mesh
fn add_greedy_quad(
    vertices: &mut Vec<ChunkVertex>,
    indices: &mut Vec<u32>,
    face: Face,
    d: u32, u_start: u32, v_start: u32,
    width: u32, height: u32,
    tex_index: u32,
    light_values: [f32; 4],
    color: [f32; 4],
    alpha_tested: bool,
) {
    let alpha_tested = alpha_tested as u32;
    let base_index = vertices.len() as u32;
    let (positions, normal) = face.greedy_positions(d, u_start, v_start, width, height);

    // UV coordinates tile with the merged face size
    let uvs: [[f32; 2]; 4] = [
        [0.0, 0.0],
        [0.0, height as f32],
        [width as f32, height as f32],
        [width as f32, 0.0],
    ];

    for (i, pos) in positions.iter().enumerate() {
        vertices.push(ChunkVertex {
            position: *pos,
            normal,
            color,
            tex_coords: uvs[i],
            tex_index,
            light_level: light_values[i],
            alpha_tested,
        });
    }

    indices.push(base_index);
    indices.push(base_index + 1);
    indices.push(base_index + 2);
    indices.push(base_index);
    indices.push(base_index + 2);
    indices.push(base_index + 3);
}

/// Add a LOD quad (scaled by step size)
fn add_lod_quad(
    vertices: &mut Vec<ChunkVertex>,
    indices: &mut Vec<u32>,
    face: Face,
    pos: [f32; 3],
    tex_index: u32,
    light_values: [f32; 4],
    scale: f32,
    color: [f32; 4],
    alpha_tested: bool,
) {
    let alpha_tested = alpha_tested as u32;
    let base_index = vertices.len() as u32;
    let (face_verts, normal) = face.vertices_and_normal_scaled(pos, scale);

    let uvs: [[f32; 2]; 4] = [
        [0.0, 0.0],
        [0.0, scale],
        [scale, scale],
        [scale, 0.0],
    ];

    for (i, vert_pos) in face_verts.iter().enumerate() {
        vertices.push(ChunkVertex {
            position: *vert_pos,
            normal,
            color,
            tex_coords: uvs[i],
            tex_index,
            light_level: light_values[i],
            alpha_tested,
        });
    }

    indices.push(base_index);
    indices.push(base_index + 1);
    indices.push(base_index + 2);
    indices.push(base_index);
    indices.push(base_index + 2);
    indices.push(base_index + 3);
}

/// Count how many quads (groups of 4 vertices) face each of the 6 axis
/// directions in a mesh, keyed by the rounded integer normal. Test helper.
#[cfg(test)]
fn face_normal_counts(verts: &[ChunkVertex]) -> std::collections::HashMap<(i32, i32, i32), usize> {
    let mut counts = std::collections::HashMap::new();
    for v in verts.iter() {
        let key = (
            v.normal[0].round() as i32,
            v.normal[1].round() as i32,
            v.normal[2].round() as i32,
        );
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}

/// Vertex data for chunk meshes
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ChunkVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
    pub tex_coords: [f32; 2],
    pub tex_index: u32,
    pub light_level: f32,
    /// 1 = this face uses alpha-cutout (leaves + cross-render plants) and the
    /// shader may `discard` its (nearly) transparent texels. 0 = a solid/opaque
    /// face that must NEVER be discarded (so it can't render see-through). This
    /// is derived from [`is_alpha_tested`] per voxel type.
    pub alpha_tested: u32,
}

impl ChunkVertex {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ChunkVertex>() as wgpu::BufferAddress,
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
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 6]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 10]>() as wgpu::BufferAddress,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 12]>() as wgpu::BufferAddress,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Uint32,
                },
                wgpu::VertexAttribute {
                    offset: (std::mem::size_of::<[f32; 12]>() + std::mem::size_of::<u32>()) as wgpu::BufferAddress,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    offset: (std::mem::size_of::<[f32; 12]>() + std::mem::size_of::<u32>() + std::mem::size_of::<f32>()) as wgpu::BufferAddress,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Uint32,
                },
            ],
        }
    }
}

/// Face directions
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Face {
    PosX, NegX, PosY, NegY, PosZ, NegZ,
}

impl Face {
    fn from_index(i: u8) -> Self {
        match i {
            0 => Face::PosX,
            1 => Face::NegX,
            2 => Face::PosY,
            3 => Face::NegY,
            4 => Face::PosZ,
            _ => Face::NegZ,
        }
    }

    fn map_coords(&self, d: u32, u: u32, v: u32) -> (u32, u32, u32) {
        match self {
            Face::PosX | Face::NegX => (d, v, u),
            Face::PosY | Face::NegY => (u, d, v),
            Face::PosZ | Face::NegZ => (u, v, d),
        }
    }

    fn greedy_positions(&self, d: u32, u_start: u32, v_start: u32, width: u32, height: u32) -> ([[f32; 3]; 4], [f32; 3]) {
        match self {
            Face::PosX => {
                let x = d as f32 + 1.0;
                ([
                    [x, v_start as f32, u_start as f32],
                    [x, (v_start + height) as f32, u_start as f32],
                    [x, (v_start + height) as f32, (u_start + width) as f32],
                    [x, v_start as f32, (u_start + width) as f32],
                ], [1.0, 0.0, 0.0])
            }
            Face::NegX => {
                let x = d as f32;
                ([
                    [x, v_start as f32, (u_start + width) as f32],
                    [x, (v_start + height) as f32, (u_start + width) as f32],
                    [x, (v_start + height) as f32, u_start as f32],
                    [x, v_start as f32, u_start as f32],
                ], [-1.0, 0.0, 0.0])
            }
            Face::PosY => {
                let y = d as f32 + 1.0;
                ([
                    [u_start as f32, y, v_start as f32],
                    [u_start as f32, y, (v_start + height) as f32],
                    [(u_start + width) as f32, y, (v_start + height) as f32],
                    [(u_start + width) as f32, y, v_start as f32],
                ], [0.0, 1.0, 0.0])
            }
            Face::NegY => {
                let y = d as f32;
                ([
                    [(u_start + width) as f32, y, v_start as f32],
                    [(u_start + width) as f32, y, (v_start + height) as f32],
                    [u_start as f32, y, (v_start + height) as f32],
                    [u_start as f32, y, v_start as f32],
                ], [0.0, -1.0, 0.0])
            }
            // PosZ faces toward +Z; emit vertices counter-clockwise when viewed
            // from +Z (looking back toward -Z) so the geometric winding matches
            // the outward normal and the face is not back-face culled.
            Face::PosZ => {
                let z = d as f32 + 1.0;
                ([
                    [(u_start + width) as f32, v_start as f32, z],
                    [(u_start + width) as f32, (v_start + height) as f32, z],
                    [u_start as f32, (v_start + height) as f32, z],
                    [u_start as f32, v_start as f32, z],
                ], [0.0, 0.0, 1.0])
            }
            // NegZ faces toward -Z; emit vertices counter-clockwise when viewed
            // from -Z so the geometric winding matches the outward normal.
            Face::NegZ => {
                let z = d as f32;
                ([
                    [u_start as f32, v_start as f32, z],
                    [u_start as f32, (v_start + height) as f32, z],
                    [(u_start + width) as f32, (v_start + height) as f32, z],
                    [(u_start + width) as f32, v_start as f32, z],
                ], [0.0, 0.0, -1.0])
            }
        }
    }

    /// Vertices and normal for a single (LOD) face starting at `pos`, scaled by `scale`.
    /// Returns 4 vertices (CCW for front-facing) and the outward normal.
    fn vertices_and_normal_scaled(&self, pos: [f32; 3], scale: f32) -> ([[f32; 3]; 4], [f32; 3]) {
        let [x, y, z] = pos;
        match self {
            Face::PosX => (
                [
                    [x, y, z],
                    [x, y + scale, z],
                    [x, y + scale, z + scale],
                    [x, y, z + scale],
                ],
                [1.0, 0.0, 0.0],
            ),
            Face::NegX => (
                [
                    [x, y, z + scale],
                    [x, y + scale, z + scale],
                    [x, y + scale, z],
                    [x, y, z],
                ],
                [-1.0, 0.0, 0.0],
            ),
            Face::PosY => (
                [
                    [x, y, z],
                    [x, y, z + scale],
                    [x + scale, y, z + scale],
                    [x + scale, y, z],
                ],
                [0.0, 1.0, 0.0],
            ),
            Face::NegY => (
                [
                    [x + scale, y, z],
                    [x + scale, y, z + scale],
                    [x, y, z + scale],
                    [x, y, z],
                ],
                [0.0, -1.0, 0.0],
            ),
            // PosZ: counter-clockwise viewed from +Z so geometric winding
            // matches the outward +Z normal (was reversed previously).
            Face::PosZ => (
                [
                    [x + scale, y, z],
                    [x + scale, y + scale, z],
                    [x, y + scale, z],
                    [x, y, z],
                ],
                [0.0, 0.0, 1.0],
            ),
            // NegZ: counter-clockwise viewed from -Z so geometric winding
            // matches the outward -Z normal.
            Face::NegZ => (
                [
                    [x, y, z],
                    [x, y + scale, z],
                    [x + scale, y + scale, z],
                    [x + scale, y, z],
                ],
                [0.0, 0.0, -1.0],
            ),
        }
    }
}

/// Manages the set of loaded chunks around the player, handling loading/unloading
/// based on the player's position and the configured render distance.
pub struct ChunkManager {
    pub chunks: std::collections::HashMap<ChunkCoord, Chunk>,
    config: RingWorldConfig,
    /// Render distance in chunks (Chebyshev radius)
    render_distance: u32,
}

impl ChunkManager {
    pub fn new(config: RingWorldConfig, render_distance: u32) -> Self {
        Self {
            chunks: std::collections::HashMap::new(),
            config,
            render_distance,
        }
    }

    pub fn config(&self) -> &RingWorldConfig {
        &self.config
    }

    pub fn render_distance(&self) -> u32 {
        self.render_distance
    }

    pub fn get_chunk(&self, coord: &ChunkCoord) -> Option<&Chunk> {
        self.chunks.get(coord)
    }

    pub fn get_chunk_mut(&mut self, coord: &ChunkCoord) -> Option<&mut Chunk> {
        self.chunks.get_mut(coord)
    }

    /// Set a voxel at chunk-local coordinates, marking BOTH the edited chunk and
    /// any boundary-adjacent neighbor chunk dirty so their meshes are rebuilt.
    ///
    /// This fixes the "edited face doesn't update from the player's viewpoint"
    /// bug: when a block on a chunk boundary is placed/removed, the adjacent
    /// chunk's boundary face toward that block had its visibility computed
    /// against the *old* voxel. `Chunk::set_voxel` only dirties the chunk it
    /// belongs to, so without this the neighbor keeps a stale (culled or
    /// orphaned) face — a visible hole or a face that should have disappeared.
    ///
    /// Local axis -> ring axis mapping (see `chunk_transform`):
    ///   x -> ring (circumference),  y -> height (radial),  z -> width (axial)
    /// so a neighbor at x==0 is `neighbor(-1, 0, 0)`, x==size-1 is
    /// `neighbor(1, 0, 0)`, y==0 is `neighbor(0, 0, -1)`, etc.
    ///
    /// Returns true if the target chunk existed and was edited.
    pub fn set_voxel(&mut self, coord: &ChunkCoord, x: u32, y: u32, z: u32, voxel: Voxel) -> bool {
        let size = self.config.chunk_size;
        let edited = match self.chunks.get_mut(coord) {
            Some(chunk) => {
                chunk.set_voxel(x, y, z, voxel);
                true
            }
            None => false,
        };
        if !edited {
            return false;
        }

        // Mark boundary-adjacent neighbors dirty. A voxel may touch up to three
        // neighbor chunks at once (a corner), so check each axis independently.
        // (d_ring, d_width, d_height) per ChunkCoord::neighbor's signature.
        let mut neighbor_deltas: Vec<(i32, i32, i32)> = Vec::new();
        if x == 0 {
            neighbor_deltas.push((-1, 0, 0));
        } else if x == size - 1 {
            neighbor_deltas.push((1, 0, 0));
        }
        if z == 0 {
            neighbor_deltas.push((0, -1, 0));
        } else if z == size - 1 {
            neighbor_deltas.push((0, 1, 0));
        }
        if y == 0 {
            neighbor_deltas.push((0, 0, -1));
        } else if y == size - 1 {
            neighbor_deltas.push((0, 0, 1));
        }

        for (dr, dw, dh) in neighbor_deltas {
            if let Some(ncoord) = coord.neighbor(dr, dw, dh, &self.config) {
                if let Some(n) = self.chunks.get_mut(&ncoord) {
                    n.dirty = true;
                }
            }
        }

        true
    }

    /// Load/unload chunks based on the player's current ring position.
    /// Chunks within `render_distance` (Chebyshev) of the player's chunk are kept
    /// loaded; chunks outside that range are unloaded.
    pub fn update_loaded_chunks(&mut self, player_position: &RingPosition) {
        let center = ChunkCoord::from_ring_position(player_position, &self.config);
        let rd = self.render_distance as i32;
        let size = self.config.chunk_size;

        // Determine the set of coords that should be loaded.
        let mut wanted: std::collections::HashSet<ChunkCoord> = std::collections::HashSet::new();
        for d_ring in -rd..=rd {
            for d_width in -rd..=rd {
                for d_height in -rd..=rd {
                    if let Some(coord) = center.neighbor(d_ring, d_width, d_height, &self.config) {
                        wanted.insert(coord);
                    }
                }
            }
        }

        // Unload chunks that are no longer wanted.
        self.chunks.retain(|coord, _| wanted.contains(coord));

        // Load chunks that are wanted but not yet present.
        for coord in wanted {
            self.chunks.entry(coord).or_insert_with(|| Chunk::new(coord, size));
        }
    }

    /// Chebyshev distance (in chunks) from the player's chunk to the given chunk,
    /// accounting for ring wrap-around on the ring index.
    pub fn chunk_distance(&self, coord: &ChunkCoord, player_position: &RingPosition) -> u32 {
        let center = ChunkCoord::from_ring_position(player_position, &self.config);
        let circ = self.config.chunks_circumference as i32;
        let d_ring_raw = (coord.ring_index as i32 - center.ring_index as i32).rem_euclid(circ);
        let d_ring = d_ring_raw.min(circ - d_ring_raw);
        let d_width = (coord.width_index as i32 - center.width_index as i32).abs();
        let d_height = (coord.height_index as i32 - center.height_index as i32).abs();
        d_ring.max(d_width).max(d_height) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_coord() -> ChunkCoord {
        ChunkCoord::new(0, 0, 0)
    }

    // An array of six unloaded neighbors (all None).
    fn no_neighbors<'a>() -> [Option<&'a Chunk>; 6] {
        [None, None, None, None, None, None]
    }

    #[test]
    fn face_light_factor_never_below_floor() {
        // A fully-unlit chunk (all light levels 0) with no loaded neighbors:
        // every corner-light value on every face must still be >= the floor so
        // no terrain face can render pure black.
        let mut chunk = Chunk::new(test_coord(), 16);
        chunk.set_voxel(8, 8, 8, Voxel::new(VoxelType::Stone));
        let neighbors = no_neighbors();
        for face_idx in 0..6u8 {
            let face = Face::from_index(face_idx);
            let lights = chunk.face_light_factor(8, 8, 8, face, &neighbors);
            for l in lights {
                assert!(
                    l >= MIN_LIGHT_FACTOR,
                    "face index {} produced light factor {} below floor {}",
                    face_idx,
                    l,
                    MIN_LIGHT_FACTOR
                );
            }
        }
    }

    #[test]
    fn sample_light_at_unloaded_neighbor_is_symmetric() {
        // For an interior voxel whose neighbor samples fall into unloaded
        // (None) neighbor chunks, the unloaded-neighbor light must be identical
        // across all six directions (no -Y asymmetry that biased faces black).
        let chunk = Chunk::new(test_coord(), 16);
        let neighbors = no_neighbors();
        let size = chunk.size() as i32;

        let px = chunk.sample_light_at(size, 8, 8, &neighbors);
        let nx = chunk.sample_light_at(-1, 8, 8, &neighbors);
        let py = chunk.sample_light_at(8, size, 8, &neighbors);
        let ny = chunk.sample_light_at(8, -1, 8, &neighbors);
        let pz = chunk.sample_light_at(8, 8, size, &neighbors);
        let nz = chunk.sample_light_at(8, 8, -1, &neighbors);

        assert_eq!(px, nx);
        assert_eq!(px, py);
        assert_eq!(px, ny, "-Y unloaded-neighbor sample must match the others");
        assert_eq!(px, pz);
        assert_eq!(px, nz);
        assert_eq!(px, UNLOADED_NEIGHBOR_LIGHT);
    }

    #[test]
    fn new_chunk_is_all_air() {
        let chunk = Chunk::new(test_coord(), 16);
        assert!(chunk.is_empty());
        for x in 0..16 {
            for y in 0..16 {
                for z in 0..16 {
                    assert_eq!(chunk.get_voxel(x, y, z).voxel_type, VoxelType::Air);
                }
            }
        }
    }

    #[test]
    fn set_and_get_voxel_round_trip() {
        let mut chunk = Chunk::new(test_coord(), 16);
        chunk.set_voxel(3, 4, 5, Voxel::new(VoxelType::Stone));
        assert_eq!(chunk.get_voxel(3, 4, 5).voxel_type, VoxelType::Stone);
        // Neighbors remain air
        assert_eq!(chunk.get_voxel(3, 4, 6).voxel_type, VoxelType::Air);
    }

    #[test]
    fn set_voxel_marks_dirty() {
        let mut chunk = Chunk::new(test_coord(), 16);
        chunk.dirty = false;
        chunk.set_voxel(0, 0, 0, Voxel::new(VoxelType::Dirt));
        assert!(chunk.dirty);
    }

    #[test]
    fn out_of_bounds_get_returns_air() {
        let chunk = Chunk::new(test_coord(), 16);
        // Should not panic and should be Air
        assert_eq!(chunk.get_voxel(100, 0, 0).voxel_type, VoxelType::Air);
        assert_eq!(chunk.get_voxel(0, 100, 0).voxel_type, VoxelType::Air);
        assert_eq!(chunk.get_voxel(0, 0, 100).voxel_type, VoxelType::Air);
    }

    #[test]
    fn out_of_bounds_set_is_noop() {
        let mut chunk = Chunk::new(test_coord(), 16);
        // Should not panic
        chunk.set_voxel(100, 100, 100, Voxel::new(VoxelType::Stone));
        assert!(chunk.is_empty());
    }

    #[test]
    fn light_packing_round_trip() {
        let mut chunk = Chunk::new(test_coord(), 16);
        chunk.set_light(1, 2, 3, 12, 7);
        let (sun, block) = chunk.get_light(1, 2, 3);
        assert_eq!(sun, 12);
        assert_eq!(block, 7);
    }

    #[test]
    fn light_packing_independent_components() {
        let mut chunk = Chunk::new(test_coord(), 16);
        // Set max values for both
        chunk.set_light(0, 0, 0, 15, 15);
        assert_eq!(chunk.get_light(0, 0, 0), (15, 15));
        // Sunlight only
        chunk.set_light(0, 0, 0, 15, 0);
        assert_eq!(chunk.get_light(0, 0, 0), (15, 0));
        // Block only
        chunk.set_light(0, 0, 0, 0, 9);
        assert_eq!(chunk.get_light(0, 0, 0), (0, 9));
    }

    #[test]
    fn light_out_of_bounds_default() {
        let chunk = Chunk::new(test_coord(), 16);
        // Out-of-bounds returns full sunlight, no block light per implementation
        assert_eq!(chunk.get_light(100, 0, 0), (15, 0));
    }

    #[test]
    fn clear_light_resets_all() {
        let mut chunk = Chunk::new(test_coord(), 16);
        chunk.set_light(2, 2, 2, 10, 10);
        chunk.clear_light();
        assert_eq!(chunk.get_light(2, 2, 2), (0, 0));
    }

    #[test]
    fn is_empty_after_setting_air() {
        let mut chunk = Chunk::new(test_coord(), 16);
        chunk.set_voxel(0, 0, 0, Voxel::new(VoxelType::Stone));
        assert!(!chunk.is_empty());
        chunk.set_voxel(0, 0, 0, Voxel::air());
        assert!(chunk.is_empty());
    }

    #[test]
    fn size_reports_correctly() {
        let chunk = Chunk::new(test_coord(), 8);
        assert_eq!(chunk.size(), 8);
    }

    /// Cross product of (v1-v0) x (v2-v0).
    fn cross_normal(p: &[[f32; 3]; 4]) -> [f32; 3] {
        let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
        let e2 = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
        [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ]
    }

    fn same_direction(a: [f32; 3], b: [f32; 3]) -> bool {
        // Both should be non-degenerate and point the same way.
        let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let len = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
        len > 1e-3 && dot > 0.0
    }

    #[test]
    fn occludes_air_emits_face() {
        assert!(!occludes(VoxelType::Stone, VoxelType::Air));
        assert!(!occludes(VoxelType::Water, VoxelType::Air));
    }

    #[test]
    fn occludes_opaque_neighbor_culls_face() {
        assert!(occludes(VoxelType::Grass, VoxelType::Stone));
        assert!(occludes(VoxelType::Water, VoxelType::Stone));
    }

    #[test]
    fn occludes_same_translucent_type_culls_interior() {
        assert!(occludes(VoxelType::Water, VoxelType::Water));
        assert!(occludes(VoxelType::Leaves, VoxelType::Leaves));
    }

    #[test]
    fn occludes_different_translucent_types_emit() {
        assert!(!occludes(VoxelType::Water, VoxelType::Leaves));
        assert!(!occludes(VoxelType::Leaves, VoxelType::Water));
        assert!(!occludes(VoxelType::Stone, VoxelType::Water));
        assert!(!occludes(VoxelType::Stone, VoxelType::Leaves));
    }

    #[test]
    fn occludes_cross_render_plant_never_occludes() {
        assert!(!occludes(VoxelType::Stone, VoxelType::TallGrass));
        assert!(!occludes(VoxelType::Stone, VoxelType::Flower));
        assert!(!occludes(VoxelType::Leaves, VoxelType::Vine));
    }

    #[test]
    fn is_alpha_tested_only_foliage() {
        // Cross-render plants and leaves are alpha-tested (have transparent
        // holes); solid blocks are NOT (must never be discarded => never
        // see-through).
        assert!(is_alpha_tested(VoxelType::Leaves));
        assert!(is_alpha_tested(VoxelType::TallGrass));
        assert!(is_alpha_tested(VoxelType::Flower));
        assert!(is_alpha_tested(VoxelType::Mushroom));
        assert!(is_alpha_tested(VoxelType::Vine));
        assert!(!is_alpha_tested(VoxelType::Grass));
        assert!(!is_alpha_tested(VoxelType::Dirt));
        assert!(!is_alpha_tested(VoxelType::Stone));
        assert!(!is_alpha_tested(VoxelType::Wood));
        assert!(!is_alpha_tested(VoxelType::Snow));
        assert!(!is_alpha_tested(VoxelType::Water));
    }

    /// A lone Grass block must emit per-face textures: the +Y (Top) face uses
    /// TEX_GRASS_TOP, the four side faces use TEX_GRASS_SIDE, the -Y (Bottom)
    /// face uses TEX_DIRT. AND every grass vertex must carry alpha_tested == 0
    /// so the shader can never discard a grass face (no see-through sides).
    #[test]
    fn grass_block_emits_correct_per_face_textures_and_is_not_alpha_tested() {
        let mut chunk = Chunk::new(test_coord(), 16);
        chunk.set_voxel(8, 8, 8, Voxel::new(VoxelType::Grass));
        let neighbors = no_neighbors();
        let mesh = chunk.generate_mesh_split(&neighbors);

        assert_eq!(mesh.opaque_vertices.len(), 24, "grass block must emit all 6 faces");

        // Group tex_index by face normal.
        let mut top_tex = None;
        let mut bottom_tex = None;
        let mut side_texes = std::collections::HashSet::new();
        for v in &mesh.opaque_vertices {
            // No grass face may be alpha-tested (else it could be discarded).
            assert_eq!(v.alpha_tested, 0, "grass faces must not be alpha-tested");
            let ny = v.normal[1].round() as i32;
            if ny == 1 {
                top_tex = Some(v.tex_index);
            } else if ny == -1 {
                bottom_tex = Some(v.tex_index);
            } else {
                side_texes.insert(v.tex_index);
            }
        }
        assert_eq!(top_tex, Some(texture::TEX_GRASS_TOP), "top face must use grass_top");
        assert_eq!(bottom_tex, Some(texture::TEX_DIRT), "bottom face must use dirt");
        assert_eq!(
            side_texes,
            std::iter::once(texture::TEX_GRASS_SIDE).collect(),
            "all four side faces must use grass_side"
        );
    }

    /// Leaves (alpha-tested) must carry alpha_tested == 1 so their transparent
    /// texels are still discarded.
    #[test]
    fn leaves_faces_are_alpha_tested() {
        let mut chunk = Chunk::new(test_coord(), 16);
        chunk.set_voxel(8, 8, 8, Voxel::new(VoxelType::Leaves));
        let neighbors = no_neighbors();
        let mesh = chunk.generate_mesh_split(&neighbors);
        assert!(!mesh.opaque_vertices.is_empty());
        for v in &mesh.opaque_vertices {
            assert_eq!(v.alpha_tested, 1, "leaves faces must be alpha-tested");
        }
    }

    #[test]
    fn is_translucent_only_water() {
        assert!(is_translucent(VoxelType::Water));
        assert!(!is_translucent(VoxelType::Leaves));
        assert!(!is_translucent(VoxelType::Stone));
        assert!(!is_translucent(VoxelType::Air));
    }

    #[test]
    fn face_tint_clamps_opaque_alpha() {
        assert_eq!(face_tint(VoxelType::Leaves, FaceDir::Side)[3], 1.0);
        assert_eq!(face_tint(VoxelType::Stone, FaceDir::Side)[3], 1.0);
        let water_alpha = face_tint(VoxelType::Water, FaceDir::Side)[3];
        assert!(water_alpha < 1.0, "water should keep its translucent alpha");
    }

    /// Water faces must be routed into the water buffer; opaque blocks into the
    /// opaque buffer. Two adjacent water voxels cull their shared interior face.
    #[test]
    fn generate_mesh_split_routes_water_separately() {
        let mut chunk = Chunk::new(test_coord(), 16);
        // A single water voxel surrounded by air -> all 6 faces are water.
        chunk.set_voxel(8, 8, 8, Voxel::new(VoxelType::Water));
        let neighbors: [Option<&Chunk>; 6] = [None, None, None, None, None, None];
        let mesh = chunk.generate_mesh_split(&neighbors);
        assert!(mesh.opaque_vertices.is_empty(), "no opaque geometry for lone water");
        assert!(!mesh.water_vertices.is_empty(), "water voxel should emit water faces");

        // Two adjacent water voxels: the shared interior face is culled. After
        // greedy merging the run has 6 exterior quads (the two coplanar faces in
        // the long direction merge), i.e. 24 verts. Critically there are NO
        // interior faces (which would push the count well above this).
        let mut chunk2 = Chunk::new(test_coord(), 16);
        chunk2.set_voxel(8, 8, 8, Voxel::new(VoxelType::Water));
        chunk2.set_voxel(9, 8, 8, Voxel::new(VoxelType::Water));
        let mesh2 = chunk2.generate_mesh_split(&neighbors);
        assert_eq!(
            mesh2.water_vertices.len(),
            24,
            "shared water interior faces must be culled (greedy-merged exterior only)"
        );
    }

    /// An opaque block adjacent to water still draws its face toward the water.
    #[test]
    fn opaque_face_toward_water_is_drawn() {
        let mut chunk = Chunk::new(test_coord(), 16);
        chunk.set_voxel(8, 8, 8, Voxel::new(VoxelType::Stone));
        chunk.set_voxel(9, 8, 8, Voxel::new(VoxelType::Water));
        let neighbors: [Option<&Chunk>; 6] = [None, None, None, None, None, None];
        let mesh = chunk.generate_mesh_split(&neighbors);
        // Stone is fully surrounded by air except +X (water, non-occluding) ->
        // all 6 stone faces drawn = 24 verts.
        assert_eq!(mesh.opaque_vertices.len(), 24, "stone keeps all faces incl. toward water");
    }

    #[test]
    fn greedy_quad_winding_matches_outward_normal() {
        // For every face, the geometric winding (CCW) must produce a face
        // normal that points the SAME direction as the declared outward normal,
        // otherwise the quad gets back-face-culled (invisible terrain bug).
        for i in 0..6u8 {
            let face = Face::from_index(i);
            let (positions, normal) = face.greedy_positions(3, 1, 2, 2, 3);
            let geo = cross_normal(&positions);
            assert!(
                same_direction(geo, normal),
                "greedy face {} winding produces normal {:?} but declared {:?}",
                i, geo, normal
            );
        }
    }

    /// Collect the set of distinct face normals present in a vertex list,
    /// rounded to integers so the six axis normals are distinguishable.
    fn normal_set(verts: &[ChunkVertex]) -> std::collections::HashSet<(i32, i32, i32)> {
        verts
            .iter()
            .map(|v| {
                (
                    v.normal[0].round() as i32,
                    v.normal[1].round() as i32,
                    v.normal[2].round() as i32,
                )
            })
            .collect()
    }

    /// A single solid block surrounded by air (no neighbors) must emit ALL SIX
    /// cube faces. This is the core regression guard for the "missing side
    /// faces" bug: top (+Y) was visible while the four vertical sides (+X/-X/
    /// +Z/-Z) were dropped.
    #[test]
    fn lone_block_emits_all_six_faces() {
        let mut chunk = Chunk::new(test_coord(), 16);
        chunk.set_voxel(8, 8, 8, Voxel::new(VoxelType::Stone));
        let neighbors = no_neighbors();
        let mesh = chunk.generate_mesh_split(&neighbors);

        // 6 faces * 4 verts = 24.
        assert_eq!(
            mesh.opaque_vertices.len(),
            24,
            "lone block must emit all 6 faces (24 verts), got {}",
            mesh.opaque_vertices.len()
        );

        let normals = normal_set(&mesh.opaque_vertices);
        for expected in [
            (1, 0, 0),
            (-1, 0, 0),
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
        ] {
            assert!(
                normals.contains(&expected),
                "missing face with normal {:?}; present normals = {:?}",
                expected,
                normals
            );
        }
    }

    /// Stacked-blocks correctness: two stone voxels stacked vertically (a column
    /// of 2) must cull ONLY the shared interior faces (top of the lower block and
    /// bottom of the upper block) and emit ALL exposed faces. This directly tests
    /// your "is the cull picking the WRONG face when blocks stack?" theory:
    /// - There must be exactly ONE +Y (top) face: the upper block's top.
    /// - There must be exactly ONE -Y (bottom) face: the lower block's bottom.
    /// - The interior pair (lower's top, upper's bottom) must be CULLED.
    /// - Each of the 4 side directions must have exactly TWO faces (one per
    ///   block), since sides are exposed to air.
    /// If the cull logic dropped the wrong face (e.g. an exposed side instead of
    /// the hidden interior), these exact counts would fail.
    #[test]
    fn vertical_stack_culls_only_interior_faces() {
        let mut chunk = Chunk::new(test_coord(), 16);
        // Stack two stone blocks at (8,8,8) and (8,9,8) — adjacent along +Y.
        chunk.set_voxel(8, 8, 8, Voxel::new(VoxelType::Stone));
        chunk.set_voxel(8, 9, 8, Voxel::new(VoxelType::Stone));
        let neighbors = no_neighbors();
        let mesh = chunk.generate_mesh_split(&neighbors);
        let counts = face_normal_counts(&mesh.opaque_vertices);

        // Vertex counts per direction. Greedy meshing MERGES the two coplanar,
        // vertically-adjacent side faces of the stack into a single 1-wide,
        // 2-tall quad, so each exposed SIDE direction should have exactly one
        // merged quad = 4 verts (NOT two separate quads). Top and bottom each
        // have one 1x1 quad = 4 verts. The interior shared pair (lower's top,
        // upper's bottom) must be culled entirely.
        let verts = |n: (i32, i32, i32)| counts.get(&n).copied().unwrap_or(0);

        // Top and bottom: exactly one face each (4 verts).
        assert_eq!(verts((0, 1, 0)), 4, "exactly one TOP (+Y) face (upper block's top)");
        assert_eq!(verts((0, -1, 0)), 4, "exactly one BOTTOM (-Y) face (lower block's bottom)");

        // Each side direction must be PRESENT and cover BOTH blocks. Whether the
        // mesher merges them into one tall quad (4 verts) or emits two (8 verts),
        // the key correctness property is: the side faces exist and there are no
        // extra/interior faces. Accept either 4 (merged) or 8 (unmerged), but
        // NEVER 0 (that would be the "cull dropped an exposed side" bug your
        // theory describes).
        for (dir, name) in [
            ((1, 0, 0), "+X"), ((-1, 0, 0), "-X"),
            ((0, 0, 1), "+Z"), ((0, 0, -1), "-Z"),
        ] {
            let v = verts(dir);
            assert!(
                v == 4 || v == 8,
                "side {} must expose the stack as one merged (4 verts) or two (8) quads, got {} \
                 (0 = exposed side wrongly culled = the stacking bug)",
                name, v
            );
            assert!(v > 0, "side {} exposed faces must NOT be culled", name);
        }
    }

    /// Real-cliff regression (full-res mesher): two adjacent columns of
    /// DIFFERENT heights form a vertical step (a "cliff"). The taller column's
    /// exposed vertical wall — the part above the shorter column's top — must be
    /// emitted as side faces toward the shorter column. This models "almost
    /// every cliff" in natural terrain. If those step-wall faces are missing,
    /// you can see through the cliff.
    ///
    /// Layout (looking along Z): column x=8 is height 10, column x=9 is height 4.
    /// The wall facing +X at x=8 (toward the short column x=9) for y in 4..10 is
    /// exposed to air and MUST render.
    #[test]
    fn cliff_step_wall_faces_are_emitted() {
        let mut chunk = Chunk::new(test_coord(), 16);
        let z = 8u32;
        for y in 0..10 { chunk.set_voxel(8, y, z, Voxel::new(VoxelType::Stone)); }
        for y in 0..4  { chunk.set_voxel(9, y, z, Voxel::new(VoxelType::Stone)); }

        let neighbors = no_neighbors();
        let mesh = chunk.generate_mesh_split(&neighbors);

        // Find +X faces at x=9.0 (the tall column's +X wall) for y in [4,10).
        // A +X greedy quad sits at x = d+1 = 9.0. Collect the y-range covered by
        // any +X vertex at x≈9.0.
        let mut saw_exposed_wall = false;
        for v in &mesh.opaque_vertices {
            let is_pos_x = v.normal[0].round() as i32 == 1;
            let at_wall = (v.position[0] - 9.0).abs() < 0.01;
            let in_exposed_y = v.position[1] >= 4.0 - 0.01 && v.position[1] <= 10.0 + 0.01;
            if is_pos_x && at_wall && in_exposed_y && v.position[1] > 4.5 {
                saw_exposed_wall = true;
                break;
            }
        }
        assert!(
            saw_exposed_wall,
            "the tall column's exposed +X cliff wall (y in 4..10) must emit side faces; \
             missing = see-through cliff bug"
        );
    }

    /// Mirror of the cliff test for the THREE NEGATIVE directions (-X, -Z walls
    /// and the -Y underside), because the F6 diagnostic showed the negative
    /// faces (cyan -X, magenta -Y, yellow -Z) were the ones disappearing. For
    /// each, build a step where the exposed wall faces the NEGATIVE direction
    /// and assert that wall is emitted.
    #[test]
    fn cliff_negative_direction_walls_are_emitted() {
        let neighbors = no_neighbors();

        // --- -X wall: tall column at x=9, short at x=8. The tall column's wall
        // toward -X (at x=9.0 face pointing to the shorter x=8) must render. ---
        {
            let mut c = Chunk::new(test_coord(), 16);
            let z = 8u32;
            for y in 0..10 { c.set_voxel(9, y, z, Voxel::new(VoxelType::Stone)); }
            for y in 0..4  { c.set_voxel(8, y, z, Voxel::new(VoxelType::Stone)); }
            let mesh = c.generate_mesh_split(&neighbors);
            let saw = mesh.opaque_vertices.iter().any(|v| {
                v.normal[0].round() as i32 == -1
                    && (v.position[0] - 9.0).abs() < 0.01
                    && v.position[1] > 4.5 && v.position[1] <= 10.0 + 0.01
            });
            assert!(saw, "-X exposed cliff wall (cyan) must be emitted");
        }

        // --- -Z wall: tall column at z=9, short at z=8. ---
        {
            let mut c = Chunk::new(test_coord(), 16);
            let x = 8u32;
            for y in 0..10 { c.set_voxel(x, y, 9, Voxel::new(VoxelType::Stone)); }
            for y in 0..4  { c.set_voxel(x, y, 8, Voxel::new(VoxelType::Stone)); }
            let mesh = c.generate_mesh_split(&neighbors);
            let saw = mesh.opaque_vertices.iter().any(|v| {
                v.normal[2].round() as i32 == -1
                    && (v.position[2] - 9.0).abs() < 0.01
                    && v.position[1] > 4.5 && v.position[1] <= 10.0 + 0.01
            });
            assert!(saw, "-Z exposed cliff wall (yellow) must be emitted");
        }

        // --- -Y underside: a floating block has open air below; its -Y face
        // (magenta) must render. ---
        {
            let mut c = Chunk::new(test_coord(), 16);
            c.set_voxel(8, 8, 8, Voxel::new(VoxelType::Stone));
            let mesh = c.generate_mesh_split(&neighbors);
            let saw = mesh.opaque_vertices.iter().any(|v| v.normal[1].round() as i32 == -1);
            assert!(saw, "-Y underside face (magenta) must be emitted for a block with air below");
        }
    }

    /// Cross-chunk boundary correctness. Build chunk A whose +X edge column is
    /// solid. When the +X neighbor (index 0) has AIR on its touching (x=0) face,
    /// A's +X boundary faces MUST be emitted. When the neighbor's touching face
    /// is SOLID, those +X faces MUST be culled. This directly exercises the
    /// neighbor-array ordering against the Face convention.
    #[test]
    fn boundary_side_faces_respect_neighbor_voxel() {
        // Chunk A: fill the entire x = size-1 column-plane solid so its +X face
        // is a full slab of stone.
        let size = 16u32;
        let mut a = Chunk::new(test_coord(), size);
        for z in 0..size {
            for y in 0..size {
                a.set_voxel(size - 1, y, z, Voxel::new(VoxelType::Stone));
            }
        }

        // Neighbor B with AIR on its x=0 face => A's +X faces must be emitted.
        let air_neighbor = Chunk::new(ChunkCoord::new(1, 0, 0), size);
        let neighbors_air: [Option<&Chunk>; 6] =
            [Some(&air_neighbor), None, None, None, None, None];
        let mesh_air = a.generate_mesh_split(&neighbors_air);
        let normals_air = normal_set(&mesh_air.opaque_vertices);
        assert!(
            normals_air.contains(&(1, 0, 0)),
            "+X boundary face must be emitted when +X neighbor edge is air"
        );

        // Neighbor B with SOLID on its x=0 face => A's +X faces must be culled.
        let mut solid_neighbor = Chunk::new(ChunkCoord::new(1, 0, 0), size);
        for z in 0..size {
            for y in 0..size {
                solid_neighbor.set_voxel(0, y, z, Voxel::new(VoxelType::Stone));
            }
        }
        let neighbors_solid: [Option<&Chunk>; 6] =
            [Some(&solid_neighbor), None, None, None, None, None];
        let mesh_solid = a.generate_mesh_split(&neighbors_solid);
        let normals_solid = normal_set(&mesh_solid.opaque_vertices);
        assert!(
            !normals_solid.contains(&(1, 0, 0)),
            "+X boundary face must be culled when +X neighbor edge is solid"
        );
    }

    /// A FULLY SOLID chunk (every voxel stone) with no loaded neighbors must
    /// emit exactly the six outer chunk faces — all interior faces culled, but
    /// every one of the six outward surfaces present. This exercises the
    /// packed-terrain meshing path (the lone-block test does not). If any of the
    /// six surface normals is missing here, the mesher is dropping a whole face
    /// direction in dense terrain (the "blocks only render from some sides /
    /// geometry never built" bug).
    #[test]
    fn fully_solid_chunk_emits_all_six_outer_surfaces() {
        let size = 16u32;
        let mut chunk = Chunk::new(test_coord(), size);
        for z in 0..size {
            for y in 0..size {
                for x in 0..size {
                    chunk.set_voxel(x, y, z, Voxel::new(VoxelType::Stone));
                }
            }
        }
        let neighbors = no_neighbors();
        let mesh = chunk.generate_mesh_split(&neighbors);

        let normals = normal_set(&mesh.opaque_vertices);
        for expected in [
            (1, 0, 0), (-1, 0, 0),
            (0, 1, 0), (0, -1, 0),
            (0, 0, 1), (0, 0, -1),
        ] {
            assert!(
                normals.contains(&expected),
                "fully solid chunk missing outer surface with normal {:?}; present = {:?}",
                expected, normals
            );
        }

        // All six outer surfaces must be present (interior faces are culled).
        // The exact vertex count depends on how corner-light differences split
        // the greedy quads, so we only require every surface to exist, not an
        // exact count.
        assert!(
            !mesh.opaque_vertices.is_empty(),
            "fully solid chunk must emit its outer shell"
        );
    }

    /// LOD cliff regression: a fully-solid LOD super-voxel adjacent to a
    /// PARTIALLY-filled super-voxel (a cliff edge: solid bottom, air top) must
    /// still emit the face between them. The old code treated "any opaque voxel
    /// in the 2x2x2 cell" as occluding, so the half-filled cliff cell culled the
    /// neighbor's face and left a see-through hole in distant (LOD) terrain.
    /// With the full-solid occlusion rule, the vertical cliff faces are emitted.
    #[test]
    fn lod_partial_neighbor_does_not_occlude_cliff_face() {
        let size = 16u32;
        let mut chunk = Chunk::new(test_coord(), size);

        // Carve a cliff: columns at x >= 8 are tall (height 8), x < 8 are short
        // (height 4). Super-voxels straddling the height step are partially
        // filled. Fill across all z so the cliff is a long wall.
        for z in 0..size {
            for x in 0..size {
                let column_height = if x >= 8 { 8 } else { 4 };
                for y in 0..column_height {
                    chunk.set_voxel(x, y, z, Voxel::new(VoxelType::Stone));
                }
            }
        }

        let neighbors = no_neighbors();
        let lod = chunk.generate_lod_mesh_split(&neighbors);
        let counts = face_normal_counts(&lod.opaque_vertices);

        let pos_x = counts.get(&(1, 0, 0)).copied().unwrap_or(0);
        let neg_x = counts.get(&(-1, 0, 0)).copied().unwrap_or(0);
        let pos_y = counts.get(&(0, 1, 0)).copied().unwrap_or(0);
        assert!(pos_y > 0, "LOD ground must emit top (+Y) faces");
        assert!(
            pos_x > 0 && neg_x > 0,
            "LOD cliff must emit vertical side faces in both X directions \
             (got +X={}, -X={}); partial super-voxels must not over-occlude",
            pos_x, neg_x
        );
    }

    /// Cross-chunk boundary culling must be correct for ALL SIX face directions,
    /// using the SAME neighbor-array ordering the renderer builds:
    ///   [0]=neighbor(+ring), [1]=(-ring), [2]=(+height), [3]=(-height),
    ///   [4]=(+width),        [5]=(-width)
    /// and the Face convention [0]=PosX,1=NegX,2=PosY,3=NegY,4=PosZ,5=NegZ with
    /// the voxel-axis mapping x->ring, y->height, z->width. If any direction's
    /// slot is mismatched, that whole surface gets culled against the wrong
    /// neighbor in dense terrain — the "blocks only render from some sides"
    /// bug. For each direction we fill the touching edge plane of `a`, give it a
    /// SOLID neighbor in that direction's slot, and assert the touching face is
    /// culled; then an AIR neighbor and assert it is emitted.
    #[test]
    fn cross_chunk_culling_correct_for_all_six_directions() {
        let size = 16u32;

        // (face normal we test, which boundary plane of `a` to fill, the
        //  renderer neighbor slot index for that direction, and a closure that
        //  fills the neighbor's TOUCHING plane solid).
        // Boundary plane fill + neighbor touching-plane fill are expressed as
        // closures over (x,y,z) ranges via small helpers below.
        type Fill = fn(&mut Chunk, u32);
        // Fill helpers: set the named plane of a chunk solid.
        fn fill_x(c: &mut Chunk, xv: u32) { let s=c.size(); for z in 0..s { for y in 0..s { c.set_voxel(xv,y,z,Voxel::new(VoxelType::Stone)); } } }
        fn fill_y(c: &mut Chunk, yv: u32) { let s=c.size(); for z in 0..s { for x in 0..s { c.set_voxel(x,yv,z,Voxel::new(VoxelType::Stone)); } } }
        fn fill_z(c: &mut Chunk, zv: u32) { let s=c.size(); for y in 0..s { for x in 0..s { c.set_voxel(x,y,zv,Voxel::new(VoxelType::Stone)); } } }

        // direction tuples: (test_normal, a_plane_fill, a_plane_val,
        //   neighbor_slot, neighbor_plane_fill, neighbor_plane_val)
        let cases: [((i32,i32,i32), Fill, u32, usize, Fill, u32); 6] = [
            ((1,0,0),  fill_x as Fill, size-1, 0, fill_x as Fill, 0),       // +X / +ring
            ((-1,0,0), fill_x as Fill, 0,      1, fill_x as Fill, size-1),  // -X / -ring
            ((0,1,0),  fill_y as Fill, size-1, 2, fill_y as Fill, 0),       // +Y / +height
            ((0,-1,0), fill_y as Fill, 0,      3, fill_y as Fill, size-1),  // -Y / -height
            ((0,0,1),  fill_z as Fill, size-1, 4, fill_z as Fill, 0),       // +Z / +width
            ((0,0,-1), fill_z as Fill, 0,      5, fill_z as Fill, size-1),  // -Z / -width
        ];

        for (normal, a_fill, a_val, slot, n_fill, n_val) in cases {
            // Build chunk A with only its touching boundary plane solid.
            let mut a = Chunk::new(test_coord(), size);
            a_fill(&mut a, a_val);

            // SOLID neighbor in the correct slot -> touching face must be culled.
            let mut solid_n = Chunk::new(ChunkCoord::new(2, 2, 2), size);
            n_fill(&mut solid_n, n_val);
            let mut neighbors_solid: [Option<&Chunk>; 6] = [None, None, None, None, None, None];
            neighbors_solid[slot] = Some(&solid_n);
            let mesh_solid = a.generate_mesh_split(&neighbors_solid);
            let normals_solid = normal_set(&mesh_solid.opaque_vertices);
            assert!(
                !normals_solid.contains(&normal),
                "face {:?} must be CULLED when the neighbor in slot {} is solid \
                 (neighbor-array ordering mismatch); present = {:?}",
                normal, slot, normals_solid
            );

            // AIR neighbor in the correct slot -> touching face must be emitted.
            let air_n = Chunk::new(ChunkCoord::new(2, 2, 2), size);
            let mut neighbors_air: [Option<&Chunk>; 6] = [None, None, None, None, None, None];
            neighbors_air[slot] = Some(&air_n);
            let mesh_air = a.generate_mesh_split(&neighbors_air);
            let normals_air = normal_set(&mesh_air.opaque_vertices);
            assert!(
                normals_air.contains(&normal),
                "face {:?} must be EMITTED when the neighbor in slot {} is air; present = {:?}",
                normal, slot, normals_air
            );
        }
    }

    /// Editing a voxel on a chunk boundary via `ChunkManager::set_voxel` must
    /// mark BOTH the edited chunk and the boundary-adjacent neighbor dirty, so
    /// the neighbor re-meshes and the newly exposed/hidden seam face updates
    /// from the player's viewpoint. Editing an interior voxel must NOT dirty any
    /// neighbor. This is the regression guard for the "broken/placed face at a
    /// chunk seam doesn't update" bug.
    #[test]
    fn chunk_manager_set_voxel_dirties_boundary_neighbor() {
        let config = RingWorldConfig::default();
        let size = config.chunk_size;
        let mut mgr = ChunkManager::new(config.clone(), 8);

        // Center chunk (well inside ring bounds) and its +ring (+X local) neighbor.
        let center = ChunkCoord::new(0, 1, 1);
        let plus_x = center.neighbor(1, 0, 0, &config).expect("+X neighbor exists");

        // Insert both as generated, clean chunks.
        let mut c0 = Chunk::new(center, size);
        c0.generated = true;
        c0.dirty = false;
        let mut n0 = Chunk::new(plus_x, size);
        n0.generated = true;
        n0.dirty = false;
        mgr.chunks.insert(center, c0);
        mgr.chunks.insert(plus_x, n0);

        // Edit a voxel on the +X boundary (x == size-1) of the center chunk.
        let edited = mgr.set_voxel(&center, size - 1, 8, 8, Voxel::new(VoxelType::Stone));
        assert!(edited, "edit on a present chunk must succeed");
        assert!(mgr.get_chunk(&center).unwrap().dirty, "edited chunk must be dirty");
        assert!(
            mgr.get_chunk(&plus_x).unwrap().dirty,
            "+X boundary neighbor must be marked dirty so its seam face re-meshes"
        );

        // Reset both, then edit an INTERIOR voxel: neighbor must stay clean.
        mgr.get_chunk_mut(&center).unwrap().dirty = false;
        mgr.get_chunk_mut(&plus_x).unwrap().dirty = false;
        mgr.set_voxel(&center, 8, 8, 8, Voxel::new(VoxelType::Stone));
        assert!(mgr.get_chunk(&center).unwrap().dirty, "interior edit dirties its own chunk");
        assert!(
            !mgr.get_chunk(&plus_x).unwrap().dirty,
            "interior edit must NOT dirty neighbors"
        );
    }

    #[test]
    fn lod_quad_winding_matches_outward_normal() {
        for i in 0..6u8 {
            let face = Face::from_index(i);
            let (positions, normal) = face.vertices_and_normal_scaled([1.0, 2.0, 3.0], 2.0);
            let geo = cross_normal(&positions);
            assert!(
                same_direction(geo, normal),
                "lod face {} winding produces normal {:?} but declared {:?}",
                i, geo, normal
            );
        }
    }

    /// End-to-end realistic test: generate a REAL terrain chunk plus its 6 real
    /// generated neighbors (actual TerrainGenerator), then mesh the center chunk
    /// with those neighbors exactly as the renderer does. For every solid voxel
    /// in the center chunk that is exposed to AIR within the chunk, the mesh must
    /// contain a face in that direction. This reproduces the live multi-chunk
    /// path the synthetic single-chunk tests cannot, and would catch faces being
    /// dropped on real terrain cliffs.
    #[test]
    fn real_generated_chunk_emits_all_air_exposed_faces() {
        use crate::terrain::TerrainGenerator;
        let config = RingWorldConfig::default();
        let gen = TerrainGenerator::new(1234);

        let coord = ChunkCoord::new(3, 8, 1);
        let mut center = Chunk::new(coord, config.chunk_size);
        gen.generate_chunk(&mut center, &config);

        let deltas = [
            (1, 0, 0), (-1, 0, 0),
            (0, 0, 1), (0, 0, -1),
            (0, 1, 0), (0, -1, 0),
        ];
        let mut neighbor_chunks: Vec<Option<Chunk>> = Vec::new();
        for (dr, dw, dh) in deltas {
            match coord.neighbor(dr, dw, dh, &config) {
                Some(nc) => {
                    let mut n = Chunk::new(nc, config.chunk_size);
                    gen.generate_chunk(&mut n, &config);
                    neighbor_chunks.push(Some(n));
                }
                None => neighbor_chunks.push(None),
            }
        }
        let neighbors: [Option<&Chunk>; 6] = [
            neighbor_chunks[0].as_ref(),
            neighbor_chunks[1].as_ref(),
            neighbor_chunks[2].as_ref(),
            neighbor_chunks[3].as_ref(),
            neighbor_chunks[4].as_ref(),
            neighbor_chunks[5].as_ref(),
        ];

        let mesh = center.generate_mesh_split(&neighbors);
        let counts = face_normal_counts(&mesh.opaque_vertices);
        let size = config.chunk_size;

        let dirs: [(i32, i32, i32); 6] = [
            (1, 0, 0), (-1, 0, 0),
            (0, 1, 0), (0, -1, 0),
            (0, 0, 1), (0, 0, -1),
        ];
        let mut expected: std::collections::HashSet<(i32, i32, i32)> = std::collections::HashSet::new();
        for z in 0..size { for y in 0..size { for x in 0..size {
            let v = center.get_voxel(x, y, z).voxel_type;
            if v == VoxelType::Air || is_cross_render(v) || is_translucent(v) || v.is_transparent() {
                continue;
            }
            for (nx, ny, nz) in dirs {
                let ax = x as i32 + nx; let ay = y as i32 + ny; let az = z as i32 + nz;
                if ax < 0 || ay < 0 || az < 0
                    || ax >= size as i32 || ay >= size as i32 || az >= size as i32 { continue; }
                if center.get_voxel(ax as u32, ay as u32, az as u32).voxel_type == VoxelType::Air {
                    expected.insert((nx, ny, nz));
                }
            }
        }}}

        for dir in expected {
            let got = counts.get(&dir).copied().unwrap_or(0);
            assert!(
                got > 0,
                "real terrain has air-exposed solid faces in direction {:?} but the mesh \
                 emitted NONE — faces dropped in the live path. per-normal counts: {:?}",
                dir, counts
            );
        }
    }
}
