/// Structure generation for the ring world
/// Includes villages, ruins, dungeons, ring edge walls, and a sun tower

use crate::chunk::Chunk;
use crate::terrain::{TerrainGenerator, Biome, SEA_LEVEL};
use crate::voxel::{Voxel, VoxelType};
use crate::ring_world::RingWorldConfig;

/// Structure generator that places large-scale structures in the world
pub struct StructureGenerator {
    seed: u32,
}

impl StructureGenerator {
    pub fn new(seed: u32) -> Self {
        Self { seed }
    }

    fn hash(x: i32, z: i32, seed: u32) -> u32 {
        let mut h = (x as u32).wrapping_mul(374761393)
            .wrapping_add((z as u32).wrapping_mul(668265263))
            .wrapping_add(seed.wrapping_mul(1274126177));
        h = (h ^ (h >> 13)).wrapping_mul(1103515245);
        h ^ (h >> 16)
    }

    fn hash3(x: i32, z: i32, extra: i32, seed: u32) -> u32 {
        Self::hash(
            x.wrapping_add(extra.wrapping_mul(7919)),
            z.wrapping_add(extra.wrapping_mul(6271)),
            seed,
        )
    }

    fn world_to_noise_x(&self, world_x: i32, config: &RingWorldConfig) -> f64 {
        let total = (config.chunks_circumference * config.chunk_size) as f64;
        let theta = (world_x as f64 / total) * std::f64::consts::PI * 2.0;
        theta * config.radius * 0.01
    }

    fn world_to_noise_z(&self, world_z: i32, config: &RingWorldConfig) -> f64 {
        let y = -(config.width / 2.0) + world_z as f64;
        y * 0.01
    }

    /// Generate all structures for a chunk
    pub fn generate_structures(&self, chunk: &mut Chunk, config: &RingWorldConfig, terrain: &TerrainGenerator) {
        self.generate_ring_edge_walls(chunk, config);
        self.generate_sun_tower(chunk, config, terrain);
        self.generate_dungeons(chunk, config, terrain);
        self.generate_ruins(chunk, config, terrain);
        self.generate_villages(chunk, config, terrain);
    }

    /// Deterministic query: the center (world blocks x, z) of a village
    /// within `radius` blocks of the given position, if any. Mirrors
    /// generate_villages' cell hashing exactly, so entity spawning can put
    /// Ringkin natives around the villages they build without scanning
    /// chunks. Ring-wrapped on x.
    pub fn village_center_near(
        &self,
        world_x: i32,
        world_z: i32,
        config: &RingWorldConfig,
        terrain: &TerrainGenerator,
        radius: i32,
    ) -> Option<(i32, i32)> {
        let cs = config.chunk_size as i32;
        let circ_blocks = config.chunks_circumference as i32 * cs;
        let cell_size = 64i32;
        let n_cells = (config.chunks_circumference as i32 + cell_size - 1) / cell_size;
        for ci in 0..n_cells {
            let cell = ci * cell_size;
            let ch = Self::hash(cell, 2, self.seed.wrapping_add(4000));
            if ch % 2 != 0 { continue; } // MUST match generate_villages
            let ph = Self::hash3(cell, 2, 1, self.seed.wrapping_add(4001));
            let vr = (cell + (ph % cell_size as u32) as i32)
                .rem_euclid(config.chunks_circumference as i32);
            let wh = Self::hash3(cell, 2, 2, self.seed.wrapping_add(4002));
            let vw = 3 + (wh % (config.chunks_width - 6) as u32) as i32;
            let vx = vr * cs + cs / 2;
            let vz = vw * cs + cs / 2;
            let nx = self.world_to_noise_x(vx, config);
            let nz = self.world_to_noise_z(vz, config);
            if terrain.sample_biome(nx, nz) != Biome::Plains { continue; }
            if terrain.sample_terrain_height(nx, nz, config) as i32 <= SEA_LEVEL as i32 { continue; }
            let dxr = (world_x - vx).rem_euclid(circ_blocks);
            let dx = dxr.min(circ_blocks - dxr);
            if dx <= radius && (world_z - vz).abs() <= radius {
                return Some((vx, vz));
            }
        }
        None
    }

    /// Deterministic query: the center of a surface FACILITY (ancient ruin
    /// or the sun tower) within `radius` blocks. Sentinel machines guard
    /// these installations day and night. Mirrors generate_ruins /
    /// generate_sun_tower placement hashing exactly.
    pub fn facility_center_near(
        &self,
        world_x: i32,
        world_z: i32,
        config: &RingWorldConfig,
        terrain: &TerrainGenerator,
        radius: i32,
    ) -> Option<(i32, i32)> {
        let cs = config.chunk_size as i32;
        let circ_blocks = config.chunks_circumference as i32 * cs;
        let wrapped_close = |x: i32, cx: i32, z: i32, cz: i32| -> bool {
            let dxr = (x - cx).rem_euclid(circ_blocks);
            let dx = dxr.min(circ_blocks - dxr);
            dx <= radius && (z - cz).abs() <= radius
        };

        // Sun tower (fixed installation).
        let tower_cx = 8i32;
        let tower_cz = (config.chunks_width * config.chunk_size / 2) as i32;
        if wrapped_close(world_x, tower_cx, world_z, tower_cz) {
            return Some((tower_cx, tower_cz));
        }

        // Ruins (cell hashing mirror of generate_ruins).
        let cell_size = 32i32;
        let n_cells = (config.chunks_circumference as i32 + cell_size - 1) / cell_size;
        for ci in 0..n_cells {
            let cell = ci * cell_size;
            let ch = Self::hash(cell, 1, self.seed.wrapping_add(3000));
            if ch % 3 != 0 { continue; } // MUST match generate_ruins
            let ph = Self::hash3(cell, 1, 1, self.seed.wrapping_add(3001));
            let rr = (cell + (ph % cell_size as u32) as i32)
                .rem_euclid(config.chunks_circumference as i32);
            let wh = Self::hash3(cell, 1, 2, self.seed.wrapping_add(3002));
            let rw = 2 + (wh % (config.chunks_width - 4) as u32) as i32;
            let rx = rr * cs + cs / 2;
            let rz = rw * cs + cs / 2;
            let nx = self.world_to_noise_x(rx, config);
            let nz = self.world_to_noise_z(rz, config);
            if terrain.sample_biome(nx, nz) == Biome::Ocean { continue; }
            if terrain.sample_terrain_height(nx, nz, config) as i32 <= SEA_LEVEL as i32 { continue; }
            if wrapped_close(world_x, rx, world_z, rz) {
                return Some((rx, rz));
            }
        }
        None
    }

    /// Force-place a block at world coords if within this chunk (overwrites anything)
    fn force_block_in_chunk(&self, chunk: &mut Chunk, world_x: i32, world_z: i32, world_y: i32, vt: VoxelType) {
        let cs = chunk.size() as i32;
        let coord = chunk.coord;
        let lx = world_x - coord.ring_index as i32 * cs;
        let lz = world_z - coord.width_index as i32 * cs;
        let ly = world_y - coord.height_index as i32 * cs;
        if lx >= 0 && lx < cs && lz >= 0 && lz < cs && ly >= 0 && ly < cs {
            chunk.set_voxel(lx as u32, ly as u32, lz as u32, Voxel::new(vt));
        }
    }

    /// Place a block only if current is Air
    fn place_block_on_surface(&self, chunk: &mut Chunk, _config: &RingWorldConfig, world_x: i32, world_z: i32, world_y: i32, vt: VoxelType) {
        let cs = chunk.size() as i32;
        let coord = chunk.coord;
        let lx = world_x - coord.ring_index as i32 * cs;
        let lz = world_z - coord.width_index as i32 * cs;
        let ly = world_y - coord.height_index as i32 * cs;
        if lx >= 0 && lx < cs && lz >= 0 && lz < cs && ly >= 0 && ly < cs {
            let current = chunk.get_voxel(lx as u32, ly as u32, lz as u32).voxel_type;
            if current == VoxelType::Air || current == VoxelType::TallGrass || current == VoxelType::Flower {
                chunk.set_voxel(lx as u32, ly as u32, lz as u32, Voxel::new(vt));
            }
        }
    }

    // ========== Ring Edge Walls ==========

    fn generate_ring_edge_walls(&self, chunk: &mut Chunk, config: &RingWorldConfig) {
        let coord = chunk.coord;
        let cs = chunk.size();
        if coord.width_index != 0 && coord.width_index != config.chunks_width - 1 {
            return;
        }
        for ly in 0..cs {
            for lx in 0..cs {
                if coord.width_index == 0 {
                    for lz in 0..3u32.min(cs) {
                        chunk.set_voxel(lx, ly, lz, Voxel::new(VoxelType::Bedrock));
                    }
                } else {
                    let start = if cs >= 3 { cs - 3 } else { 0 };
                    for lz in start..cs {
                        chunk.set_voxel(lx, ly, lz, Voxel::new(VoxelType::Bedrock));
                    }
                }
            }
        }
    }

    // ========== Sun Tower ==========

    fn generate_sun_tower(&self, chunk: &mut Chunk, config: &RingWorldConfig, terrain: &TerrainGenerator) {
        let tower_cx = 8i32;
        let tower_cz = (config.chunks_width * config.chunk_size / 2) as i32;
        let coord = chunk.coord;
        let cs = chunk.size() as i32;
        let cwx = coord.ring_index as i32 * cs;
        let cwz = coord.width_index as i32 * cs;
        let cwy = coord.height_index as i32 * cs;

        if cwx + cs <= tower_cx - 2 || cwx > tower_cx + 2 { return; }
        if cwz + cs <= tower_cz - 2 || cwz > tower_cz + 2 { return; }

        let nx = self.world_to_noise_x(tower_cx, config);
        let nz = self.world_to_noise_z(tower_cz, config);
        let surface = terrain.sample_terrain_height(nx, nz, config) as i32;
        let tower_h = 45;
        let tower_top = surface + tower_h;

        if cwy + cs <= surface || cwy > tower_top + 1 { return; }

        for wx in (tower_cx - 2)..=(tower_cx + 2) {
            for wz in (tower_cz - 2)..=(tower_cz + 2) {
                let dx = wx - tower_cx;
                let dz = wz - tower_cz;
                for wy in surface..=(tower_top + 1) {
                    let ry = wy - surface;
                    let vt = if ry == 0 {
                        Some(VoxelType::Stone)
                    } else if ry <= tower_h {
                        if dx.abs() <= 1 && dz.abs() <= 1 {
                            if dx.abs() == 1 && dz.abs() == 1 {
                                Some(VoxelType::Stone) // corners
                            } else if self.is_spiral_stair(dx, dz, ry) {
                                Some(VoxelType::Stone)
                            } else if dx.abs() == 1 || dz.abs() == 1 {
                                Some(VoxelType::Stone) // walls
                            } else {
                                Some(VoxelType::Air) // center hollow
                            }
                        } else { None }
                    } else {
                        Some(VoxelType::GoldOre) // top platform
                    };
                    if let Some(v) = vt {
                        self.force_block_in_chunk(chunk, wx, wz, wy, v);
                    }
                }
            }
        }
    }

    fn is_spiral_stair(&self, dx: i32, dz: i32, height: i32) -> bool {
        let step = ((height - 1) % 8 + 8) % 8;
        let (sx, sz) = match step {
            0 => (1, 0), 1 => (1, 1), 2 => (0, 1), 3 => (-1, 1),
            4 => (-1, 0), 5 => (-1, -1), 6 => (0, -1), 7 => (1, -1),
            _ => (0, 0),
        };
        dx == sx && dz == sz
    }

    // ========== Dungeons ==========

    fn generate_dungeons(&self, chunk: &mut Chunk, config: &RingWorldConfig, terrain: &TerrainGenerator) {
        let coord = chunk.coord;
        let cs = chunk.size() as i32;
        let cell_size = 16i32;
        let chunk_ring = coord.ring_index as i32;
        let cell_start = (chunk_ring / cell_size) * cell_size;

        for cell_offset in -1..=1 {
            let cell = (cell_start + cell_offset * cell_size).rem_euclid(config.chunks_circumference as i32);
            let ch = Self::hash(cell, 0, self.seed.wrapping_add(2000));
            if ch % 16 != 0 { continue; }

            let ph = Self::hash3(cell, 0, 1, self.seed.wrapping_add(2001));
            let dr = (cell + (ph % cell_size as u32) as i32).rem_euclid(config.chunks_circumference as i32);
            let wh = Self::hash3(cell, 0, 2, self.seed.wrapping_add(2002));
            let dw = 2 + (wh % (config.chunks_width - 4) as u32) as i32;

            let dung_x = dr * cs + cs / 2;
            let dung_z = dw * cs + cs / 2;
            let hh = Self::hash3(cell, 0, 3, self.seed.wrapping_add(2003));
            let dung_y = 5 + (hh % 20) as i32;
            let sh = Self::hash3(cell, 0, 4, self.seed.wrapping_add(2004));
            let rw = 7 + (sh % 6) as i32;
            let rd = 7 + ((sh >> 3) % 6) as i32;
            let rh = 4 + ((sh >> 6) % 2) as i32;

            let min_x = dung_x - rw / 2;
            let max_x = dung_x + rw / 2;
            let min_z = dung_z - rd / 2;
            let max_z = dung_z + rd / 2;
            let cwx = coord.ring_index as i32 * cs;
            let cwz = coord.width_index as i32 * cs;
            let cwy = coord.height_index as i32 * cs;

            let cl = 8i32;
            if cwx + cs <= min_x - cl || cwx > max_x + cl { continue; }
            if cwz + cs <= min_z - cl || cwz > max_z + cl { continue; }
            if cwy + cs <= dung_y || cwy > dung_y + rh + 1 { continue; }

            let nx = self.world_to_noise_x(dung_x, config);
            let nz = self.world_to_noise_z(dung_z, config);
            let surface = terrain.sample_terrain_height(nx, nz, config) as i32;
            if dung_y + rh >= surface - 3 { continue; }

            // Main room
            for rx in 0..rw {
                for rz in 0..rd {
                    let wx = min_x + rx;
                    let wz = min_z + rz;
                    let is_wall = rx == 0 || rx == rw - 1 || rz == 0 || rz == rd - 1;
                    self.force_block_in_chunk(chunk, wx, wz, dung_y, VoxelType::Stone);
                    for ry in 1..=rh {
                        if ry == rh || is_wall {
                            self.force_block_in_chunk(chunk, wx, wz, dung_y + ry, VoxelType::Stone);
                        } else {
                            self.force_block_in_chunk(chunk, wx, wz, dung_y + ry, VoxelType::Air);
                        }
                    }
                }
            }
            // Chest
            self.force_block_in_chunk(chunk, dung_x, dung_z, dung_y + 1, VoxelType::GoldOre);

            // Corridors
            let corh = Self::hash3(cell, 0, 5, self.seed.wrapping_add(2005));
            let ncor = 1 + (corh % 2) as i32;
            for c in 0..ncor {
                let dh = Self::hash3(cell, c, 6, self.seed.wrapping_add(2006));
                let dir = dh % 4;
                let clen = 5 + ((dh >> 4) % 4) as i32;
                let (sx, sz, stepx, stepz) = match dir {
                    0 => (max_x + 1, dung_z, 1i32, 0i32),
                    1 => (min_x - 1, dung_z, -1, 0),
                    2 => (dung_x, max_z + 1, 0, 1),
                    _ => (dung_x, min_z - 1, 0, -1),
                };
                for s in 0..clen {
                    let cx = sx + stepx * s;
                    let cz = sz + stepz * s;
                    for oy in 1..=3i32 {
                        for ow in -1..=1i32 {
                            let bx = if stepz != 0 { cx + ow } else { cx };
                            let bz = if stepx != 0 { cz + ow } else { cz };
                            self.force_block_in_chunk(chunk, bx, bz, dung_y + oy, VoxelType::Air);
                        }
                    }
                    for ow in -1..=1i32 {
                        let bx = if stepz != 0 { cx + ow } else { cx };
                        let bz = if stepx != 0 { cz + ow } else { cz };
                        self.force_block_in_chunk(chunk, bx, bz, dung_y, VoxelType::Stone);
                    }
                }
            }
        }
    }

    // ========== Ruins ==========

    fn generate_ruins(&self, chunk: &mut Chunk, config: &RingWorldConfig, terrain: &TerrainGenerator) {
        let coord = chunk.coord;
        let cs = chunk.size() as i32;
        let cell_size = 32i32;
        let chunk_ring = coord.ring_index as i32;
        let cell_start = (chunk_ring / cell_size) * cell_size;

        for cell_offset in -1..=1 {
            let cell = (cell_start + cell_offset * cell_size).rem_euclid(config.chunks_circumference as i32);
            let ch = Self::hash(cell, 1, self.seed.wrapping_add(3000));
            // 1/3 per 32-chunk cell (~2-3 ruins/ring; was ~25% per ring).
            if ch % 3 != 0 { continue; }

            let ph = Self::hash3(cell, 1, 1, self.seed.wrapping_add(3001));
            let rr = (cell + (ph % cell_size as u32) as i32).rem_euclid(config.chunks_circumference as i32);
            let wh = Self::hash3(cell, 1, 2, self.seed.wrapping_add(3002));
            let rw = 2 + (wh % (config.chunks_width - 4) as u32) as i32;

            let ruin_x = rr * cs + cs / 2;
            let ruin_z = rw * cs + cs / 2;
            let sh = Self::hash3(cell, 1, 3, self.seed.wrapping_add(3003));
            let half = (10 + (sh % 11) as i32) / 2;

            let cwx = coord.ring_index as i32 * cs;
            let cwz = coord.width_index as i32 * cs;
            if cwx + cs <= ruin_x - half || cwx > ruin_x + half { continue; }
            if cwz + cs <= ruin_z - half || cwz > ruin_z + half { continue; }

            let nx = self.world_to_noise_x(ruin_x, config);
            let nz = self.world_to_noise_z(ruin_z, config);
            let biome = terrain.sample_biome(nx, nz);
            if biome == Biome::Ocean { continue; }
            let surface = terrain.sample_terrain_height(nx, nz, config) as i32;
            if surface <= SEA_LEVEL as i32 { continue; }

            let cwy = coord.height_index as i32 * cs;
            if cwy > surface + 10 { continue; }

            // Broken walls
            for side in 0..4i32 {
                let wah = Self::hash3(cell, side, 10, self.seed.wrapping_add(3010));
                if wah % 3 == 0 { continue; }
                for i in -half..=half {
                    let (wx, wz) = match side {
                        0 => (ruin_x + i, ruin_z - half),
                        1 => (ruin_x + i, ruin_z + half),
                        2 => (ruin_x - half, ruin_z + i),
                        _ => (ruin_x + half, ruin_z + i),
                    };
                    let gh = Self::hash(wx, wz, self.seed.wrapping_add(3020));
                    if gh % 4 == 0 { continue; }
                    let hh = Self::hash3(wx, wz, 11, self.seed.wrapping_add(3021));
                    let wht = 1 + (hh % 4) as i32;
                    let lnx = self.world_to_noise_x(wx, config);
                    let lnz = self.world_to_noise_z(wz, config);
                    let ls = terrain.sample_terrain_height(lnx, lnz, config) as i32;
                    for h in 1..=wht {
                        let bt = if hh % 7 == 0 { VoxelType::Bedrock } else { VoxelType::Stone };
                        self.place_block_on_surface(chunk, config, wx, wz, ls + h, bt);
                    }
                }
            }
            // Pillars
            let np = 3 + (Self::hash(cell, 2, self.seed.wrapping_add(3030)) % 3) as i32;
            for p in 0..np {
                let pih = Self::hash3(cell, p, 12, self.seed.wrapping_add(3031));
                let px = ruin_x - half + (pih % (half as u32 * 2)) as i32;
                let pz = ruin_z - half + ((pih >> 8) % (half as u32 * 2)) as i32;
                let ph2 = 3 + ((pih >> 16) % 4) as i32;
                let lnx = self.world_to_noise_x(px, config);
                let lnz = self.world_to_noise_z(pz, config);
                let ls = terrain.sample_terrain_height(lnx, lnz, config) as i32;
                for h in 1..=ph2 {
                    self.place_block_on_surface(chunk, config, px, pz, ls + h, VoxelType::Stone);
                }
            }
            // Scattered blocks
            let ns = 5 + (Self::hash(cell, 3, self.seed.wrapping_add(3040)) % 8) as i32;
            for s in 0..ns {
                let sch = Self::hash3(cell, s, 13, self.seed.wrapping_add(3041));
                let sx = ruin_x - half + (sch % (half as u32 * 2)) as i32;
                let sz = ruin_z - half + ((sch >> 8) % (half as u32 * 2)) as i32;
                let lnx = self.world_to_noise_x(sx, config);
                let lnz = self.world_to_noise_z(sz, config);
                let ls = terrain.sample_terrain_height(lnx, lnz, config) as i32;
                let bt = if sch % 10 == 0 { VoxelType::Bedrock } else { VoxelType::Stone };
                self.place_block_on_surface(chunk, config, sx, sz, ls + 1, bt);
            }
        }
    }

    // ========== Villages ==========

    fn generate_villages(&self, chunk: &mut Chunk, config: &RingWorldConfig, terrain: &TerrainGenerator) {
        let coord = chunk.coord;
        let cs = chunk.size() as i32;
        let cell_size = 64i32;
        let chunk_ring = coord.ring_index as i32;
        let cell_start = (chunk_ring / cell_size) * cell_size;

        for cell_offset in -1..=1 {
            let cell = (cell_start + cell_offset * cell_size).rem_euclid(config.chunks_circumference as i32);
            let ch = Self::hash(cell, 2, self.seed.wrapping_add(4000));
            // 1/2 per 64-chunk cell (~2-3 villages/ring). The old 1/64 made
            // villages a ~6% chance PER RING (seed 42 had zero, ever).
            if ch % 2 != 0 { continue; }

            let ph = Self::hash3(cell, 2, 1, self.seed.wrapping_add(4001));
            let vr = (cell + (ph % cell_size as u32) as i32).rem_euclid(config.chunks_circumference as i32);
            let wh = Self::hash3(cell, 2, 2, self.seed.wrapping_add(4002));
            let vw = 3 + (wh % (config.chunks_width - 6) as u32) as i32;

            let village_x = vr * cs + cs / 2;
            let village_z = vw * cs + cs / 2;
            let vrad = 25i32;

            let cwx = coord.ring_index as i32 * cs;
            let cwz = coord.width_index as i32 * cs;
            if cwx + cs <= village_x - vrad || cwx > village_x + vrad { continue; }
            if cwz + cs <= village_z - vrad || cwz > village_z + vrad { continue; }

            let nx = self.world_to_noise_x(village_x, config);
            let nz = self.world_to_noise_z(village_z, config);
            let biome = terrain.sample_biome(nx, nz);
            if biome != Biome::Plains { continue; }
            let surface = terrain.sample_terrain_height(nx, nz, config) as i32;
            if surface <= SEA_LEVEL as i32 { continue; }

            let cwy = coord.height_index as i32 * cs;
            if cwy > surface + 10 { continue; }

            // Number of houses (3-6)
            let nh = 3 + (Self::hash(cell, 4, self.seed.wrapping_add(4003)) % 4) as i32;

            // Place well at center
            self.place_well(chunk, config, terrain, village_x, village_z);

            // Place houses around center
            for h in 0..nh {
                let hh = Self::hash3(cell, h, 20, self.seed.wrapping_add(4010));
                let angle = (h as f64 / nh as f64) * std::f64::consts::PI * 2.0;
                let dist = 8.0 + (hh % 12) as f64;
                let hx = village_x + (angle.cos() * dist) as i32;
                let hz = village_z + (angle.sin() * dist) as i32;

                // Path from house to center
                self.place_path(chunk, config, terrain, hx, hz, village_x, village_z);
                // House
                self.place_house(chunk, config, terrain, hx, hz, hh);
            }
        }
    }

    /// Place a well (3x3 hole with water, stone rim)
    fn place_well(&self, chunk: &mut Chunk, config: &RingWorldConfig, terrain: &TerrainGenerator, cx: i32, cz: i32) {
        let nx = self.world_to_noise_x(cx, config);
        let nz = self.world_to_noise_z(cz, config);
        let surface = terrain.sample_terrain_height(nx, nz, config) as i32;

        for dx in -1..=1i32 {
            for dz in -1..=1i32 {
                let wx = cx + dx;
                let wz = cz + dz;
                let is_rim = dx.abs() == 1 || dz.abs() == 1;
                if is_rim {
                    // Stone rim at surface+1
                    self.place_block_on_surface(chunk, config, wx, wz, surface + 1, VoxelType::Stone);
                } else {
                    // Center: dig down and fill with water
                    for wy in (surface - 3)..=surface {
                        self.force_block_in_chunk(chunk, wx, wz, wy, VoxelType::Water);
                    }
                }
            }
        }
    }

    /// Place a gravel path between two points
    fn place_path(&self, chunk: &mut Chunk, config: &RingWorldConfig, terrain: &TerrainGenerator, x1: i32, z1: i32, x2: i32, z2: i32) {
        let dx = x2 - x1;
        let dz = z2 - z1;
        let steps = dx.abs().max(dz.abs());
        if steps == 0 { return; }

        for s in 0..=steps {
            let t = s as f64 / steps as f64;
            let px = x1 + (dx as f64 * t) as i32;
            let pz = z1 + (dz as f64 * t) as i32;

            // 2-block wide path
            for pw in 0..2i32 {
                let path_x = px + pw;
                let lnx = self.world_to_noise_x(path_x, config);
                let lnz = self.world_to_noise_z(pz, config);
                let ls = terrain.sample_terrain_height(lnx, lnz, config) as i32;
                self.force_block_in_chunk(chunk, path_x, pz, ls, VoxelType::Gravel);
                // Clear above path
                self.force_block_in_chunk(chunk, path_x, pz, ls + 1, VoxelType::Air);
                self.force_block_in_chunk(chunk, path_x, pz, ls + 2, VoxelType::Air);
            }
        }
    }

    /// Place a small house (5x5x4 footprint)
    fn place_house(&self, chunk: &mut Chunk, config: &RingWorldConfig, terrain: &TerrainGenerator, cx: i32, cz: i32, hash: u32) {
        let nx = self.world_to_noise_x(cx, config);
        let nz = self.world_to_noise_z(cz, config);
        let surface = terrain.sample_terrain_height(nx, nz, config) as i32;
        let door_side = hash % 4; // 0=+x, 1=-x, 2=+z, 3=-z

        // 5x5 footprint centered on (cx, cz)
        for dx in -2..=2i32 {
            for dz in -2..=2i32 {
                let wx = cx + dx;
                let wz = cz + dz;
                let is_edge = dx.abs() == 2 || dz.abs() == 2;

                // Floor (wood)
                self.force_block_in_chunk(chunk, wx, wz, surface + 1, VoxelType::Wood);

                // Walls (3 high) or interior
                for h in 2..=4i32 {
                    let wy = surface + h;
                    if is_edge {
                        // Check for door opening
                        let is_door = h <= 3 && match door_side {
                            0 => dx == 2 && dz == 0,
                            1 => dx == -2 && dz == 0,
                            2 => dz == 2 && dx == 0,
                            _ => dz == -2 && dx == 0,
                        };
                        if is_door {
                            self.force_block_in_chunk(chunk, wx, wz, wy, VoxelType::Air);
                        } else {
                            self.force_block_in_chunk(chunk, wx, wz, wy, VoxelType::Stone);
                        }
                    } else {
                        // Interior air
                        self.force_block_in_chunk(chunk, wx, wz, wy, VoxelType::Air);
                    }
                }

                // Roof (wood, flat)
                self.force_block_in_chunk(chunk, wx, wz, surface + 5, VoxelType::Wood);
            }
        }
    }
}
