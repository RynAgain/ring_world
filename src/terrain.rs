/// Terrain generation for the ring world using noise functions
/// Includes biome system, water generation, caves, ores, ravines, rivers,
/// cliffs/overhangs, underwater terrain detail, and vegetation (trees, cactus, etc.)

use noise::{NoiseFn, Perlin};
use crate::chunk::Chunk;
use crate::voxel::{Voxel, VoxelType};
use crate::ring_world::{ChunkCoord, RingWorldConfig};
use crate::structures::StructureGenerator;

/// Sea level height (out of 64 max height)
pub const SEA_LEVEL: u32 = 25;

/// Biome types that determine terrain characteristics
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Biome {
    Ocean,
    Beach,
    Plains,
    Forest,
    Mountains,
    Desert,
}

impl Biome {
    /// Human-readable name for this biome (used in the debug overlay).
    pub fn name(&self) -> &'static str {
        match self {
            Biome::Ocean => "Ocean",
            Biome::Beach => "Beach",
            Biome::Plains => "Plains",
            Biome::Forest => "Forest",
            Biome::Mountains => "Mountains",
            Biome::Desert => "Desert",
        }
    }

    /// Get the representative color for this biome (used for distant ring rendering)
    pub fn color(&self) -> [f32; 4] {
        match self {
            Biome::Ocean => [0.15, 0.3, 0.7, 1.0],
            Biome::Beach => [0.85, 0.8, 0.55, 1.0],
            Biome::Plains => [0.3, 0.7, 0.2, 1.0],
            Biome::Forest => [0.15, 0.5, 0.1, 1.0],
            Biome::Mountains => [0.5, 0.45, 0.4, 1.0],
            Biome::Desert => [0.9, 0.78, 0.45, 1.0],
        }
    }

    /// Get the color at a specific height within this biome
    pub fn color_at_height(&self, height: f64, max_height: f64) -> [f32; 4] {
        let normalized = height / max_height;
        match self {
            Biome::Ocean => {
                if normalized < 0.35 {
                    [0.1, 0.2, 0.6, 1.0]
                } else {
                    [0.15, 0.35, 0.75, 1.0]
                }
            }
            Biome::Beach => [0.85, 0.8, 0.55, 1.0],
            Biome::Plains => [0.3, 0.7, 0.2, 1.0],
            Biome::Forest => {
                if normalized > 0.6 {
                    [0.1, 0.45, 0.08, 1.0]
                } else {
                    [0.2, 0.55, 0.15, 1.0]
                }
            }
            Biome::Mountains => {
                if normalized > 0.75 {
                    [0.9, 0.9, 0.95, 1.0]
                } else if normalized > 0.5 {
                    [0.45, 0.42, 0.4, 1.0]
                } else {
                    [0.35, 0.5, 0.25, 1.0]
                }
            }
            Biome::Desert => [0.9, 0.78, 0.45, 1.0],
        }
    }
}

/// Terrain generator using layered noise
pub struct TerrainGenerator {
    terrain_noise: Perlin,
    detail_noise: Perlin,
    biome_noise: Perlin,
    biome_detail_noise: Perlin,
    cave_noise: Perlin,
    cave_noise_2: Perlin,
    ore_iron_noise: Perlin,
    ore_gold_noise: Perlin,
    ore_diamond_noise: Perlin,
    ravine_noise: Perlin,
    river_noise: Perlin,
    overhang_noise: Perlin,
    underwater_noise: Perlin,
    tree_noise: Perlin,
    vegetation_noise: Perlin,
    continent_noise: Perlin,
    structure_generator: StructureGenerator,
    seed: u32,
    /// Radius (in noise units) of the circle the linear noise_x coordinate is
    /// wrapped onto, making all noise periodic in theta. Equals ring radius *
    /// 0.01 (the noise coordinate scale), so the period matches theta's 2*PI.
    noise_circle_radius: f64,
}

impl TerrainGenerator {
    pub fn new(seed: u32) -> Self {
        Self {
            terrain_noise: Perlin::new(seed),
            detail_noise: Perlin::new(seed.wrapping_add(1)),
            biome_noise: Perlin::new(seed.wrapping_add(100)),
            biome_detail_noise: Perlin::new(seed.wrapping_add(200)),
            cave_noise: Perlin::new(seed.wrapping_add(300)),
            cave_noise_2: Perlin::new(seed.wrapping_add(301)),
            ore_iron_noise: Perlin::new(seed.wrapping_add(400)),
            ore_gold_noise: Perlin::new(seed.wrapping_add(401)),
            ore_diamond_noise: Perlin::new(seed.wrapping_add(402)),
            ravine_noise: Perlin::new(seed.wrapping_add(500)),
            river_noise: Perlin::new(seed.wrapping_add(600)),
            overhang_noise: Perlin::new(seed.wrapping_add(700)),
            underwater_noise: Perlin::new(seed.wrapping_add(800)),
            tree_noise: Perlin::new(seed.wrapping_add(900)),
            vegetation_noise: Perlin::new(seed.wrapping_add(1000)),
            continent_noise: Perlin::new(seed.wrapping_add(1100)),
            structure_generator: StructureGenerator::new(seed),
            seed,
            noise_circle_radius: RingWorldConfig::default().radius * 0.01,
        }
    }

    /// Map the linear circumferential noise coordinate onto a circle so every
    /// noise sample is periodic in theta. The ring seam (theta wrapping from
    /// 2*PI back to 0) previously produced a hard discontinuity in biomes and
    /// terrain because noise_x jumped from its maximum back to zero. Sampling
    /// on a circle whose arc length equals the linear coordinate keeps feature
    /// sizes identical everywhere while making the seam invisible.
    fn circle_coords(&self, noise_x: f64) -> (f64, f64) {
        let ang = noise_x / self.noise_circle_radius;
        (
            self.noise_circle_radius * ang.cos(),
            self.noise_circle_radius * ang.sin(),
        )
    }

    /// Periodic 2D noise sample at the given frequency.
    fn get2(&self, noise: &Perlin, noise_x: f64, noise_z: f64, freq: f64) -> f64 {
        let (cx, cy) = self.circle_coords(noise_x);
        noise.get([cx * freq, cy * freq, noise_z * freq])
    }

    /// Periodic 3D (adds height) noise sample at the given frequency.
    fn get3(&self, noise: &Perlin, noise_x: f64, noise_z: f64, h: f64, freq: f64) -> f64 {
        let (cx, cy) = self.circle_coords(noise_x);
        noise.get([cx * freq, cy * freq, noise_z * freq, h * freq])
    }

    /// The combined biome-selection scalar. Biome thresholds and the height
    /// blend both read this same value so they can never disagree.
    fn biome_scalar(&self, noise_x: f64, noise_z: f64) -> f64 {
        let biome_val = self.get2(&self.biome_noise, noise_x, noise_z, 0.03);
        let biome_detail = self.get2(&self.biome_detail_noise, noise_x, noise_z, 0.05);
        biome_val * 0.7 + biome_detail * 0.3
    }

    /// Determine the biome at a given noise coordinate
    pub fn sample_biome(&self, noise_x: f64, noise_z: f64) -> Biome {
        let combined = self.biome_scalar(noise_x, noise_z);

        if combined < -0.5 {
            Biome::Ocean
        } else if combined < -0.3 {
            Biome::Beach
        } else if combined < 0.0 {
            Biome::Plains
        } else if combined < 0.3 {
            Biome::Forest
        } else if combined < 0.6 {
            Biome::Mountains
        } else {
            Biome::Desert
        }
    }

    /// Get terrain height based on biome
    fn biome_terrain_height(&self, noise_x: f64, noise_z: f64, biome: Biome) -> f64 {
        let hills = self.get2(&self.terrain_noise, noise_x, noise_z, 0.1) * 0.5 + 0.5;
        let bumps = self.get2(&self.detail_noise, noise_x, noise_z, 0.4) * 0.5 + 0.5;
        let detail = self.get2(&self.terrain_noise, noise_x, noise_z, 1.5) * 0.5 + 0.5;

        match biome {
            Biome::Ocean => {
                let base = hills * 0.3 + bumps * 0.1;
                10.0 + base * 12.0
            }
            Biome::Beach => {
                let base = hills * 0.4 + bumps * 0.2 + detail * 0.1;
                22.0 + base * 6.0
            }
            Biome::Plains => {
                let base = hills * 0.3 + bumps * 0.15 + detail * 0.05;
                27.0 + base * 8.0
            }
            Biome::Forest => {
                let base = hills * 0.6 + bumps * 0.25 + detail * 0.1;
                28.0 + base * 14.0
            }
            Biome::Mountains => {
                let base = hills * 0.7 + bumps * 0.2 + detail * 0.1;
                30.0 + base * 28.0
            }
            Biome::Desert => {
                let base = hills * 0.4 + bumps * 0.3 + detail * 0.05;
                26.0 + base * 8.0
            }
        }
    }

    /// Band centers for the biome-selection scalar. The height blend
    /// interpolates between the two bracketing biomes' height profiles so
    /// biome borders are slopes instead of vertical cliff walls (each biome's
    /// raw height range is disjoint, e.g. Ocean tops out near 22 where Beach
    /// starts, and Mountains reach 58 next to Desert's 34).
    const BIOME_BANDS: [(f64, Biome); 6] = [
        (-0.65, Biome::Ocean),
        (-0.40, Biome::Beach),
        (-0.15, Biome::Plains),
        (0.15, Biome::Forest),
        (0.45, Biome::Mountains),
        (0.75, Biome::Desert),
    ];

    /// Terrain height with cross-biome blending plus a very low frequency
    /// continental swell (about +/- 4 blocks) for large-scale elevation
    /// variety (rolling coastlines, occasional small ocean islands).
    fn blended_terrain_height(&self, noise_x: f64, noise_z: f64) -> f64 {
        let t = self.biome_scalar(noise_x, noise_z);
        let bands = Self::BIOME_BANDS;

        let mut height = if t <= bands[0].0 {
            self.biome_terrain_height(noise_x, noise_z, bands[0].1)
        } else if t >= bands[bands.len() - 1].0 {
            self.biome_terrain_height(noise_x, noise_z, bands[bands.len() - 1].1)
        } else {
            let mut h = 0.0;
            for i in 0..bands.len() - 1 {
                let (c0, b0) = bands[i];
                let (c1, b1) = bands[i + 1];
                if t >= c0 && t <= c1 {
                    let u = (t - c0) / (c1 - c0);
                    let u = u * u * (3.0 - 2.0 * u); // smoothstep
                    let h0 = self.biome_terrain_height(noise_x, noise_z, b0);
                    let h1 = self.biome_terrain_height(noise_x, noise_z, b1);
                    h = h0 * (1.0 - u) + h1 * u;
                    break;
                }
            }
            h
        };

        height += self.get2(&self.continent_noise, noise_x, noise_z, 0.008) * 4.0;
        height.clamp(3.0, 62.0)
    }

    /// Check if a position is a river location
    fn is_river(&self, noise_x: f64, noise_z: f64, biome: Biome) -> bool {
        if biome != Biome::Plains && biome != Biome::Forest {
            return false;
        }
        let river_val = self.get2(&self.river_noise, noise_x, noise_z, 0.01);
        river_val.abs() < 0.015
    }

    /// Get river width factor (0.0 = not river, 1.0 = center of river)
    fn river_factor(&self, noise_x: f64, noise_z: f64, biome: Biome) -> f64 {
        if biome != Biome::Plains && biome != Biome::Forest {
            return 0.0;
        }
        let river_val = self.get2(&self.river_noise, noise_x, noise_z, 0.01);
        let abs_val = river_val.abs();
        if abs_val < 0.015 {
            1.0 - (abs_val / 0.015)
        } else {
            0.0
        }
    }

    /// Get the surface voxel type for a biome at a given height
    fn surface_voxel(&self, biome: Biome, height: i32) -> VoxelType {
        match biome {
            Biome::Ocean => {
                if height < SEA_LEVEL as i32 - 5 {
                    VoxelType::Dirt
                } else {
                    VoxelType::Sand
                }
            }
            Biome::Beach => VoxelType::Sand,
            Biome::Plains => VoxelType::Grass,
            Biome::Forest => VoxelType::Grass,
            Biome::Mountains => {
                if height > 50 {
                    VoxelType::Snow
                } else if height > 42 {
                    VoxelType::Stone
                } else {
                    VoxelType::Grass
                }
            }
            Biome::Desert => VoxelType::Sand,
        }
    }

    /// Get the sub-surface voxel type for a biome
    fn subsurface_voxel(&self, biome: Biome, _height: i32) -> VoxelType {
        match biome {
            Biome::Ocean => VoxelType::Sand,
            Biome::Beach => VoxelType::Sand,
            Biome::Desert => VoxelType::Sand,
            _ => VoxelType::Dirt,
        }
    }

    /// Check if a position should be carved as a cave
    fn is_cave(&self, noise_x: f64, noise_z: f64, height: i32, terrain_height: i32) -> bool {
        if height > terrain_height - 3 {
            return false;
        }
        if height <= 1 {
            return false;
        }

        let height_factor = 1.0 - (height as f64 / 64.0) * 0.4;

        let cave_val = self.get3(&self.cave_noise, noise_x, noise_z, height as f64, 0.05);

        let cave_val_2 = self.get3(&self.cave_noise_2, noise_x, noise_z, height as f64, 0.1);

        let threshold_1 = 0.6 - (1.0 - height_factor) * 0.15;
        let threshold_2 = 0.5 - (1.0 - height_factor) * 0.1;

        cave_val > threshold_1 && cave_val_2 > threshold_2
    }

    /// Check if a position should be a ravine
    fn is_ravine(&self, noise_x: f64, noise_z: f64, height: i32, terrain_height: i32) -> bool {
        if height < 5 || height > terrain_height {
            return false;
        }

        let ravine_val = self.get2(&self.ravine_noise, noise_x, noise_z, 0.08);

        if ravine_val.abs() < 0.02 {
            let ravine_bottom = (terrain_height - 15).max(5);
            height >= ravine_bottom
        } else {
            false
        }
    }

    /// Determine ore type at a position (returns None if no ore)
    fn sample_ore(&self, noise_x: f64, noise_z: f64, height: i32) -> Option<VoxelType> {
        if height >= 5 && height <= 15 {
            let diamond_val = self.get3(&self.ore_diamond_noise, noise_x, noise_z, height as f64, 0.15);
            if diamond_val > 0.85 {
                return Some(VoxelType::DiamondOre);
            }
        }

        if height >= 5 && height <= 30 {
            let gold_val = self.get3(&self.ore_gold_noise, noise_x, noise_z, height as f64, 0.12);
            if gold_val > 0.78 {
                return Some(VoxelType::GoldOre);
            }
        }

        if height >= 5 && height <= 45 {
            let iron_val = self.get3(&self.ore_iron_noise, noise_x, noise_z, height as f64, 0.1);
            if iron_val > 0.65 {
                return Some(VoxelType::IronOre);
            }
        }

        None
    }

    /// Check if a position should have an overhang (Mountains biome)
    fn is_overhang(&self, noise_x: f64, noise_z: f64, height: i32, terrain_height: i32) -> bool {
        if height <= terrain_height || height > terrain_height + 5 {
            return false;
        }

        let overhang_val = self.get3(&self.overhang_noise, noise_x, noise_z, height as f64, 0.15);

        overhang_val > 0.7
    }

    /// Get underwater terrain detail for Ocean biome
    fn underwater_voxel(&self, noise_x: f64, noise_z: f64, height: i32, terrain_height: i32) -> VoxelType {
        if height != terrain_height {
            return VoxelType::Sand;
        }

        let detail_val = self.get2(&self.underwater_noise, noise_x, noise_z, 0.2);
        let detail_val_2 = self.get2(&self.underwater_noise, noise_x, noise_z, 0.5);

        if detail_val > 0.4 {
            VoxelType::Gravel
        } else if detail_val < -0.5 && detail_val_2 > 0.3 {
            VoxelType::Stone
        } else {
            VoxelType::Sand
        }
    }

    /// Deterministic hash for tree/vegetation placement decisions
    fn position_hash(x: i32, z: i32, seed: u32) -> u32 {
        let mut h = (x as u32).wrapping_mul(374761393)
            .wrapping_add((z as u32).wrapping_mul(668265263))
            .wrapping_add(seed.wrapping_mul(1274126177));
        h = (h ^ (h >> 13)).wrapping_mul(1103515245);
        h = h ^ (h >> 16);
        h & 0xFFFF
    }

    /// Check if a tree should be placed at this world position
    /// Returns tree type: 0 = no tree, 1 = oak, 2 = birch, 3 = pine
    fn tree_at_position(&self, noise_x: f64, noise_z: f64, biome: Biome, world_x: i32, world_z: i32) -> u8 {
        let threshold = match biome {
            Biome::Forest => 0.55,
            Biome::Plains => 0.75,
            Biome::Mountains => 0.70,
            _ => return 0,
        };

        let tree_val = self.get2(&self.tree_noise, noise_x, noise_z, 0.5);
        if tree_val < threshold {
            return 0;
        }

        let ph = Self::position_hash(world_x, world_z, self.seed);
        let spacing_check = match biome {
            Biome::Forest => ph % 7 == 0,
            Biome::Plains => ph % 12 == 0,
            Biome::Mountains => ph % 9 == 0,
            _ => false,
        };

        if !spacing_check {
            return 0;
        }

        match biome {
            Biome::Forest => {
                if ph % 10 < 3 { 2 } else { 1 }
            }
            Biome::Plains => 1,
            Biome::Mountains => 3,
            _ => 0,
        }
    }

    fn oak_tree_height(world_x: i32, world_z: i32, seed: u32) -> u32 {
        4 + (Self::position_hash(world_x, world_z, seed.wrapping_add(111)) % 3)
    }

    fn birch_tree_height(world_x: i32, world_z: i32, seed: u32) -> u32 {
        5 + (Self::position_hash(world_x, world_z, seed.wrapping_add(222)) % 3)
    }

    fn pine_tree_height(world_x: i32, world_z: i32, seed: u32) -> u32 {
        7 + (Self::position_hash(world_x, world_z, seed.wrapping_add(333)) % 4)
    }

    fn cactus_height(world_x: i32, world_z: i32, seed: u32) -> u32 {
        2 + (Self::position_hash(world_x, world_z, seed.wrapping_add(444)) % 3)
    }

    /// Generate terrain for a chunk
    pub fn generate_chunk(&self, chunk: &mut Chunk, config: &RingWorldConfig) {
        let chunk_size = chunk.size();
        let coord = chunk.coord;

        for lz in 0..chunk_size {
            for lx in 0..chunk_size {
                let (world_theta, world_y) = self.local_to_world(lx, lz, &coord, config);
                let noise_x = world_theta * config.radius * 0.01;
                let noise_z = world_y * 0.01;
                let biome = self.sample_biome(noise_x, noise_z);

                let mut terrain_height = self.blended_terrain_height(noise_x, noise_z);

                let river_f = self.river_factor(noise_x, noise_z, biome);
                if river_f > 0.0 {
                    let river_depth = 3.0 + river_f * 2.0;
                    terrain_height -= river_depth;
                    if terrain_height < 20.0 {
                        terrain_height = 20.0;
                    }
                }

                let terrain_height_i = terrain_height as i32;

                let ocean_floor_offset = if biome == Biome::Ocean {
                    let extra_depth = self.get2(&self.underwater_noise, noise_x, noise_z, 0.08);
                    (extra_depth * 4.0) as i32
                } else {
                    0
                };
                let effective_terrain_height = terrain_height_i + ocean_floor_offset;

                for ly in 0..chunk_size {
                    let world_height = (coord.height_index * chunk_size + ly) as i32;

                    let mut voxel_type = if world_height == 0 {
                        VoxelType::Bedrock
                    } else if world_height > effective_terrain_height {
                        if world_height <= SEA_LEVEL as i32 && (biome == Biome::Ocean || biome == Biome::Beach) {
                            VoxelType::Water
                        } else if river_f > 0.0 && world_height <= SEA_LEVEL as i32 {
                            VoxelType::Water
                        } else if biome == Biome::Ocean && world_height <= SEA_LEVEL as i32
                            && world_height == effective_terrain_height + 1
                        {
                            let seagrass_val = self.get2(&self.underwater_noise, noise_x, noise_z, 0.8);
                            if seagrass_val > 0.5 {
                                VoxelType::Leaves
                            } else {
                                VoxelType::Water
                            }
                        } else if biome == Biome::Ocean && world_height <= SEA_LEVEL as i32 {
                            VoxelType::Water
                        } else {
                            VoxelType::Air
                        }
                    } else if world_height == effective_terrain_height {
                        if biome == Biome::Ocean {
                            self.underwater_voxel(noise_x, noise_z, world_height, effective_terrain_height)
                        } else {
                            self.surface_voxel(biome, world_height)
                        }
                    } else if world_height > effective_terrain_height - 3 {
                        self.subsurface_voxel(biome, world_height)
                    } else {
                        VoxelType::Stone
                    };

                    // Ore generation
                    if voxel_type == VoxelType::Stone {
                        if let Some(ore) = self.sample_ore(noise_x, noise_z, world_height) {
                            voxel_type = ore;
                        }
                    }

                    // Mountain overhangs
                    if biome == Biome::Mountains && voxel_type == VoxelType::Air
                        && world_height > terrain_height_i
                    {
                        if self.is_overhang(noise_x, noise_z, world_height, terrain_height_i) {
                            voxel_type = VoxelType::Stone;
                        }
                    }

                    // Cave carving
                    if voxel_type != VoxelType::Air && voxel_type != VoxelType::Water
                        && voxel_type != VoxelType::Bedrock
                    {
                        if self.is_cave(noise_x, noise_z, world_height, effective_terrain_height) {
                            voxel_type = VoxelType::Air;
                        }
                    }

                    // Ravine carving
                    if voxel_type != VoxelType::Air && voxel_type != VoxelType::Water
                        && voxel_type != VoxelType::Bedrock
                    {
                        if self.is_ravine(noise_x, noise_z, world_height, effective_terrain_height) {
                            voxel_type = VoxelType::Air;
                        }
                    }

                    chunk.set_voxel(lx, ly, lz, Voxel::new(voxel_type));
                }
            }
        }

        // Second pass: vegetation
        self.generate_vegetation(chunk, config);

        // Third pass: structures (villages, ruins, dungeons, walls, tower)
        self.structure_generator.generate_structures(chunk, config, self);

        chunk.generated = true;
        chunk.dirty = true;
    }

    /// Generate vegetation for a chunk (second pass after terrain)
    fn generate_vegetation(&self, chunk: &mut Chunk, config: &RingWorldConfig) {
        let chunk_size = chunk.size();
        let coord = chunk.coord;

        // Handle trees from this chunk and neighboring chunks (trees can extend across boundaries)
        for dz in -1i32..=1 {
            for dx in -1i32..=1 {
                // Wrap the ring index so trees generate (and overhang) across
                // the ring seam; width is a true world edge, so bound it.
                let circ = config.chunks_circumference as i32;
                let neighbor_ring = (coord.ring_index as i32 + dx).rem_euclid(circ);
                let neighbor_width = coord.width_index as i32 + dz;

                if neighbor_width < 0 || neighbor_width >= config.chunks_width as i32 {
                    continue;
                }

                for nlz in 0..chunk_size {
                    for nlx in 0..chunk_size {
                        let world_x = neighbor_ring * chunk_size as i32 + nlx as i32;
                        let world_z = neighbor_width * chunk_size as i32 + nlz as i32;

                        let neighbor_coord = ChunkCoord {
                            ring_index: neighbor_ring as u32,
                            width_index: neighbor_width as u32,
                            height_index: coord.height_index,
                        };
                        let (world_theta, world_y) = self.local_to_world(nlx, nlz, &neighbor_coord, config);
                        let noise_x = world_theta * config.radius * 0.01;
                        let noise_z = world_y * 0.01;
                        let biome = self.sample_biome(noise_x, noise_z);

                        let mut terrain_height = self.blended_terrain_height(noise_x, noise_z);
                        let river_f = self.river_factor(noise_x, noise_z, biome);
                        if river_f > 0.0 {
                            terrain_height -= 3.0 + river_f * 2.0;
                            if terrain_height < 20.0 {
                                terrain_height = 20.0;
                            }
                        }
                        let surface_height = terrain_height as i32;

                        // Skip trees on rivers
                        if river_f > 0.0 {
                            continue;
                        }

                        let tree_type = self.tree_at_position(noise_x, noise_z, biome, world_x, world_z);
                        if tree_type > 0 {
                            if biome == Biome::Mountains && surface_height < 35 {
                                continue;
                            }
                            self.place_tree(chunk, config, world_x, world_z, surface_height, tree_type);
                        }

                        // Cactus (only from this chunk's columns)
                        if dx == 0 && dz == 0 && biome == Biome::Desert {
                            let cactus_hash = Self::position_hash(world_x, world_z, self.seed.wrapping_add(555));
                            if cactus_hash % 33 == 0 {
                                self.place_cactus(chunk, config, world_x, world_z, surface_height);
                            }
                        }
                    }
                }
            }
        }

        // Ground-level vegetation (only for columns in this chunk)
        for lz in 0..chunk_size {
            for lx in 0..chunk_size {
                let (world_theta, world_y) = self.local_to_world(lx, lz, &coord, config);
                let noise_x = world_theta * config.radius * 0.01;
                let noise_z = world_y * 0.01;
                let biome = self.sample_biome(noise_x, noise_z);

                let mut terrain_height = self.blended_terrain_height(noise_x, noise_z);
                let river_f = self.river_factor(noise_x, noise_z, biome);
                if river_f > 0.0 {
                    terrain_height -= 3.0 + river_f * 2.0;
                    if terrain_height < 20.0 {
                        terrain_height = 20.0;
                    }
                }
                let surface_height = terrain_height as i32;

                let world_x = coord.ring_index as i32 * chunk_size as i32 + lx as i32;
                let world_z = coord.width_index as i32 * chunk_size as i32 + lz as i32;

                // Tall Grass
                if biome == Biome::Plains || biome == Biome::Forest {
                    let veg_val = self.get2(&self.vegetation_noise, noise_x, noise_z, 2.0);
                    let threshold = match biome {
                        Biome::Plains => 0.35,
                        Biome::Forest => 0.55,
                        _ => 1.0,
                    };
                    if veg_val > threshold {
                        let above_height = surface_height + 1;
                        self.place_ground_decoration(chunk, config, lx, lz, above_height, VoxelType::TallGrass, VoxelType::Grass);
                    }
                }

                // Flowers (Plains only, ~3%)
                if biome == Biome::Plains {
                    let flower_hash = Self::position_hash(world_x, world_z, self.seed.wrapping_add(666));
                    if flower_hash % 33 == 0 {
                        let above_height = surface_height + 1;
                        self.place_ground_decoration(chunk, config, lx, lz, above_height, VoxelType::Flower, VoxelType::Grass);
                    }
                }

                // Mushrooms in caves (~1% of eligible positions)
                let mushroom_hash = Self::position_hash(world_x, world_z, self.seed.wrapping_add(777));
                if mushroom_hash % 100 == 0 {
                    let chunk_base_y = (coord.height_index * chunk_size) as i32;
                    for ly in 1..chunk_size {
                        let world_height = chunk_base_y + ly as i32;
                        if world_height >= surface_height {
                            break;
                        }
                        let below = chunk.get_voxel(lx, ly - 1, lz).voxel_type;
                        let at = chunk.get_voxel(lx, ly, lz).voxel_type;
                        if (below == VoxelType::Stone || below == VoxelType::Dirt) && at == VoxelType::Air {
                            chunk.set_voxel(lx, ly, lz, Voxel::new(VoxelType::Mushroom));
                            break;
                        }
                    }
                }

                // Vines in Forest biome (~20% of leaf blocks)
                if biome == Biome::Forest {
                    let vine_hash = Self::position_hash(world_x, world_z, self.seed.wrapping_add(888));
                    if vine_hash % 5 == 0 {
                        // Find leaves blocks in this column and hang vines below
                        for ly in (1..chunk_size).rev() {
                            let voxel = chunk.get_voxel(lx, ly, lz).voxel_type;
                            if voxel == VoxelType::Leaves {
                                let vine_len = 1 + (vine_hash % 4);
                                for v in 1..=vine_len {
                                    if ly >= v && chunk.get_voxel(lx, ly - v, lz).voxel_type == VoxelType::Air {
                                        chunk.set_voxel(lx, ly - v, lz, Voxel::new(VoxelType::Vine));
                                    } else {
                                        break;
                                    }
                                }
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Place a tree at the given world position
    fn place_tree(&self, chunk: &mut Chunk, config: &RingWorldConfig, world_x: i32, world_z: i32, surface_height: i32, tree_type: u8) {
        match tree_type {
            1 => self.place_oak_tree(chunk, config, world_x, world_z, surface_height),
            2 => self.place_birch_tree(chunk, config, world_x, world_z, surface_height),
            3 => self.place_pine_tree(chunk, config, world_x, world_z, surface_height),
            _ => {}
        }
    }

    /// Place an oak tree: 4-6 block trunk + 3x3x3 leaf canopy with corners removed
    fn place_oak_tree(&self, chunk: &mut Chunk, config: &RingWorldConfig, world_x: i32, world_z: i32, surface_height: i32) {
        let trunk_height = Self::oak_tree_height(world_x, world_z, self.seed) as i32;
        let trunk_top = surface_height + trunk_height;

        // Place trunk
        for y in (surface_height + 1)..=trunk_top {
            self.try_place_block(chunk, config, world_x, world_z, y, VoxelType::Wood);
        }

        // Place leaf canopy (3x3x3 centered above trunk, corners removed)
        for dy in 0..3i32 {
            for ddz in -1..=1i32 {
                for ddx in -1..=1i32 {
                    // Remove corners on top and bottom layers
                    if (dy == 0 || dy == 2) && ddx.abs() == 1 && ddz.abs() == 1 {
                        continue;
                    }
                    // Don't overwrite trunk in lower canopy
                    if ddx == 0 && ddz == 0 && dy == 0 {
                        continue;
                    }
                    let leaf_y = trunk_top + dy;
                    let leaf_x = world_x + ddx;
                    let leaf_z = world_z + ddz;
                    self.try_place_leaf(chunk, config, leaf_x, leaf_z, leaf_y);
                }
            }
        }
        // Top leaf on trunk
        self.try_place_leaf(chunk, config, world_x, world_z, trunk_top + 1);
        self.try_place_leaf(chunk, config, world_x, world_z, trunk_top + 2);
    }

    /// Place a birch tree: 5-7 block trunk + narrow canopy
    fn place_birch_tree(&self, chunk: &mut Chunk, config: &RingWorldConfig, world_x: i32, world_z: i32, surface_height: i32) {
        let trunk_height = Self::birch_tree_height(world_x, world_z, self.seed) as i32;
        let trunk_top = surface_height + trunk_height;

        // Place trunk
        for y in (surface_height + 1)..=trunk_top {
            self.try_place_block(chunk, config, world_x, world_z, y, VoxelType::Wood);
        }

        // Narrow canopy: cross pattern, 4 blocks tall
        for dy in -1..=3i32 {
            let leaf_y = trunk_top + dy;
            // Always place center
            self.try_place_leaf(chunk, config, world_x, world_z, leaf_y);
            // Wider in middle layers
            if dy >= 0 && dy <= 1 {
                for d in [-1i32, 1] {
                    self.try_place_leaf(chunk, config, world_x + d, world_z, leaf_y);
                    self.try_place_leaf(chunk, config, world_x, world_z + d, leaf_y);
                }
            }
        }
    }

    /// Place a pine tree: 7-10 block trunk + triangular canopy
    fn place_pine_tree(&self, chunk: &mut Chunk, config: &RingWorldConfig, world_x: i32, world_z: i32, surface_height: i32) {
        let trunk_height = Self::pine_tree_height(world_x, world_z, self.seed) as i32;
        let trunk_top = surface_height + trunk_height;

        // Place trunk
        for y in (surface_height + 1)..=trunk_top {
            self.try_place_block(chunk, config, world_x, world_z, y, VoxelType::Wood);
        }

        // Triangular canopy: wider at bottom, narrow at top
        let canopy_start = trunk_top - 4;
        let canopy_end = trunk_top + 2;

        for y in canopy_start..=canopy_end {
            let progress = (y - canopy_start) as f32 / (canopy_end - canopy_start) as f32;
            let radius = ((1.0 - progress) * 3.0) as i32;

            for ddz in -radius..=radius {
                for ddx in -radius..=radius {
                    // Skip corners for more circular shape
                    if ddx.abs() == radius && ddz.abs() == radius {
                        continue;
                    }
                    let bx = world_x + ddx;
                    let bz = world_z + ddz;
                    // Don't overwrite trunk
                    if ddx == 0 && ddz == 0 && y <= trunk_top {
                        continue;
                    }
                    self.try_place_leaf(chunk, config, bx, bz, y);
                }
            }
        }
        // Top point
        self.try_place_leaf(chunk, config, world_x, world_z, canopy_end + 1);
    }

    /// Place a cactus column
    fn place_cactus(&self, chunk: &mut Chunk, config: &RingWorldConfig, world_x: i32, world_z: i32, surface_height: i32) {
        let height = Self::cactus_height(world_x, world_z, self.seed) as i32;
        for y in (surface_height + 1)..=(surface_height + height) {
            self.try_place_block(chunk, config, world_x, world_z, y, VoxelType::Cactus);
        }
    }

    /// Place a ground decoration block if the surface below is the expected type
    fn place_ground_decoration(&self, chunk: &mut Chunk, _config: &RingWorldConfig, lx: u32, lz: u32, world_height: i32, voxel_type: VoxelType, required_surface: VoxelType) {
        let chunk_size = chunk.size();
        let coord = chunk.coord;
        let chunk_base_y = (coord.height_index * chunk_size) as i32;
        let ly = world_height - chunk_base_y;

        if ly < 0 || ly >= chunk_size as i32 {
            return;
        }

        // Check that the block below is the required surface type
        if ly > 0 {
            let below = chunk.get_voxel(lx, (ly - 1) as u32, lz).voxel_type;
            if below != required_surface {
                return;
            }
        } else {
            return;
        }

        // Only place if current position is air
        let current = chunk.get_voxel(lx, ly as u32, lz).voxel_type;
        if current == VoxelType::Air {
            chunk.set_voxel(lx, ly as u32, lz, Voxel::new(voxel_type));
        }
    }

    /// Try to place a block at a world position if it falls within this chunk
    /// Only replaces Air blocks
    fn try_place_block(&self, chunk: &mut Chunk, config: &RingWorldConfig, world_x: i32, world_z: i32, world_y: i32, voxel_type: VoxelType) {
        let chunk_size = chunk.size() as i32;
        let coord = chunk.coord;

        let chunk_world_x = coord.ring_index as i32 * chunk_size;
        let chunk_world_z = coord.width_index as i32 * chunk_size;
        let chunk_world_y = coord.height_index as i32 * chunk_size;

        // Wrap the circumferential delta so a tree rooted just past the ring
        // seam can still place its overhanging blocks into this chunk.
        let circ_blocks = config.chunks_circumference as i32 * chunk_size;
        let lx = (world_x - chunk_world_x).rem_euclid(circ_blocks);
        let lz = world_z - chunk_world_z;
        let ly = world_y - chunk_world_y;

        if lx >= chunk_size || lz < 0 || lz >= chunk_size || ly < 0 || ly >= chunk_size {
            return;
        }

        let current = chunk.get_voxel(lx as u32, ly as u32, lz as u32).voxel_type;
        if current == VoxelType::Air {
            chunk.set_voxel(lx as u32, ly as u32, lz as u32, Voxel::new(voxel_type));
        }
    }

    /// Try to place a leaf block (only replaces Air, not Wood or other solid blocks)
    fn try_place_leaf(&self, chunk: &mut Chunk, config: &RingWorldConfig, world_x: i32, world_z: i32, world_y: i32) {
        self.try_place_block(chunk, config, world_x, world_z, world_y, VoxelType::Leaves);
    }

    /// Sample terrain height at a noise coordinate (public for distant ring use)
    pub fn sample_terrain_height(&self, noise_x: f64, noise_z: f64, _config: &RingWorldConfig) -> f64 {
        self.blended_terrain_height(noise_x, noise_z)
    }

    /// Sample terrain color at a given ring position (theta, y) for distant ring rendering
    pub fn sample_terrain_color(&self, theta: f64, y: f64, config: &RingWorldConfig) -> [f32; 4] {
        let noise_x = theta * config.radius * 0.01;
        let noise_z = y * 0.01;

        let biome = self.sample_biome(noise_x, noise_z);
        let height = self.blended_terrain_height(noise_x, noise_z);

        // Water anywhere the land dips below sea level, regardless of biome
        // label, shaded darker with depth so the arch shows real coastlines
        // and shallow shelves instead of flat blue discs.
        if height < SEA_LEVEL as f64 {
            let depth = ((SEA_LEVEL as f64 - height) / 15.0).clamp(0.0, 1.0);
            let shallow = [0.20, 0.45, 0.75];
            let deep = [0.05, 0.15, 0.45];
            return [
                (shallow[0] + (deep[0] - shallow[0]) * depth) as f32,
                (shallow[1] + (deep[1] - shallow[1]) * depth) as f32,
                (shallow[2] + (deep[2] - shallow[2]) * depth) as f32,
                1.0,
            ];
        }

        if self.is_river(noise_x, noise_z, biome) {
            return [0.2, 0.35, 0.65, 1.0];
        }

        biome.color_at_height(height, config.max_height)
    }

    /// Small-scale periodic color-variation scalar in [-1, 1] used by the
    /// distant ring to mottle biome colors (breaks up flat banding).
    pub fn sample_mottle(&self, noise_x: f64, noise_z: f64) -> f64 {
        self.get2(&self.vegetation_noise, noise_x, noise_z, 0.8)
    }

    /// Convert local chunk voxel position to world ring coordinates (theta, y)
    fn local_to_world(
        &self,
        local_x: u32,
        local_z: u32,
        coord: &ChunkCoord,
        config: &RingWorldConfig,
    ) -> (f64, f64) {
        let chunk_size = config.chunk_size as f64;

        let theta = (coord.ring_index as f64 + local_x as f64 / chunk_size) * config.chunk_angular_size();
        let y = -config.width / 2.0
            + (coord.width_index as f64 + local_z as f64 / chunk_size) * config.chunk_width_size();

        (theta, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_produces_same_terrain_height() {
        let config = RingWorldConfig::default();
        let g1 = TerrainGenerator::new(42);
        let g2 = TerrainGenerator::new(42);
        let h1 = g1.sample_terrain_height(1.23, 4.56, &config);
        let h2 = g2.sample_terrain_height(1.23, 4.56, &config);
        assert_eq!(h1, h2);
    }

    #[test]
    fn terrain_height_deterministic_same_input() {
        let config = RingWorldConfig::default();
        let g = TerrainGenerator::new(7);
        let a = g.sample_terrain_height(2.0, 3.0, &config);
        let b = g.sample_terrain_height(2.0, 3.0, &config);
        assert_eq!(a, b);
    }

    #[test]
    fn biome_selection_deterministic() {
        let g = TerrainGenerator::new(99);
        let b1 = g.sample_biome(5.0, 10.0);
        let b2 = g.sample_biome(5.0, 10.0);
        assert_eq!(b1, b2);
    }

    #[test]
    fn biome_same_seed_same_result() {
        let g1 = TerrainGenerator::new(1234);
        let g2 = TerrainGenerator::new(1234);
        for &(x, z) in &[(0.0, 0.0), (10.0, -5.0), (-3.3, 8.8), (100.0, 100.0)] {
            assert_eq!(g1.sample_biome(x, z), g2.sample_biome(x, z));
        }
    }

    #[test]
    fn generated_chunk_has_bedrock_at_bottom() {
        let config = RingWorldConfig::default();
        let g = TerrainGenerator::new(2024);
        // Bottom-most height chunk (height_index 0) gets bedrock at world_height 0
        let coord = ChunkCoord::new(0, 0, 0);
        let mut chunk = Chunk::new(coord, config.chunk_size);
        g.generate_chunk(&mut chunk, &config);

        // world_height 0 corresponds to local y=0 in height chunk 0
        for x in 0..config.chunk_size {
            for z in 0..config.chunk_size {
                assert_eq!(
                    chunk.get_voxel(x, 0, z).voxel_type,
                    VoxelType::Bedrock,
                    "expected bedrock at bottom ({},0,{})", x, z
                );
            }
        }
    }

    #[test]
    fn generated_chunk_is_marked_generated() {
        let config = RingWorldConfig::default();
        let g = TerrainGenerator::new(5);
        let coord = ChunkCoord::new(0, 0, 0);
        let mut chunk = Chunk::new(coord, config.chunk_size);
        g.generate_chunk(&mut chunk, &config);
        assert!(chunk.generated);
    }

    #[test]
    fn generate_chunk_deterministic_with_seed() {
        let config = RingWorldConfig::default();
        let coord = ChunkCoord::new(1, 1, 0);

        let g1 = TerrainGenerator::new(777);
        let mut c1 = Chunk::new(coord, config.chunk_size);
        g1.generate_chunk(&mut c1, &config);

        let g2 = TerrainGenerator::new(777);
        let mut c2 = Chunk::new(coord, config.chunk_size);
        g2.generate_chunk(&mut c2, &config);

        for x in 0..config.chunk_size {
            for y in 0..config.chunk_size {
                for z in 0..config.chunk_size {
                    assert_eq!(
                        c1.get_voxel(x, y, z).voxel_type,
                        c2.get_voxel(x, y, z).voxel_type,
                        "mismatch at ({},{},{})", x, y, z
                    );
                }
            }
        }
    }

    #[test]
    fn terrain_and_biome_continuous_across_ring_seam() {
        // theta = 0 and theta = 2*PI are the same physical place on the ring;
        // heights and biomes sampled through the noise-coordinate convention
        // (noise_x = theta * radius * 0.01) must agree exactly.
        let config = RingWorldConfig::default();
        let g = TerrainGenerator::new(42);
        let seam_x = std::f64::consts::TAU * config.radius * 0.01;
        for &y in &[-30.0f64, -5.0, 0.0, 12.5, 30.0] {
            let nz = y * 0.01;
            let a = g.sample_terrain_height(0.0, nz, &config);
            let b = g.sample_terrain_height(seam_x, nz, &config);
            assert!(
                (a - b).abs() < 1e-6,
                "height seam mismatch at y={}: {} vs {}", y, a, b
            );
            assert_eq!(
                g.sample_biome(0.0, nz),
                g.sample_biome(seam_x, nz),
                "biome seam mismatch at y={}", y
            );
        }
    }

    #[test]
    fn terrain_height_has_no_biome_cliff_walls() {
        // Walking along the ring, adjacent columns (2 blocks apart = 0.02
        // noise units) must never differ by a cliff-wall jump. Before the
        // height blend, crossing a biome threshold could jump 10+ blocks in
        // one step (e.g. Mountains base 30 next to Desert base 26 with very
        // different amplitudes).
        let config = RingWorldConfig::default();
        let g = TerrainGenerator::new(42);
        let step = 0.02;
        let mut prev = g.sample_terrain_height(0.0, 0.0, &config);
        let mut x = step;
        while x < 41.0 {
            let h = g.sample_terrain_height(x, 0.0, &config);
            assert!(
                (h - prev).abs() < 3.0,
                "cliff wall at noise_x {}: {} -> {}", x, prev, h
            );
            prev = h;
            x += step;
        }
    }

    #[test]
    fn biome_color_components_in_range() {
        for biome in [Biome::Ocean, Biome::Beach, Biome::Plains, Biome::Forest, Biome::Mountains, Biome::Desert] {
            let c = biome.color();
            for comp in c.iter() {
                assert!(*comp >= 0.0 && *comp <= 1.0);
            }
        }
    }
}
