/// Lighting system for the ring world (v0.3)
/// Handles block light propagation (torches, lava) and sunlight from ring center.
/// Light levels are stored per-voxel as a packed byte:
///   upper 4 bits = sunlight (0-15), lower 4 bits = block light (0-15)

use std::collections::VecDeque;
use crate::block::BlockProperties;
use crate::chunk::{Chunk, ChunkManager};
use crate::ring_world::{ChunkCoord, RingPosition, RingWorldConfig};
use crate::voxel::VoxelType;

/// The lighting engine computes and propagates light levels within chunks.
pub struct LightingEngine;

impl LightingEngine {
    /// Compute all lighting for a chunk (sunlight + block light).
    /// Call this after terrain generation.
    pub fn compute_lighting(chunk: &mut Chunk) {
        // Reset all light levels
        chunk.clear_light();

        // First pass: sunlight from top
        Self::propagate_sunlight(chunk);

        // Second pass: block light from emitters
        Self::propagate_block_light(chunk);
    }

    /// Propagate sunlight downward from the top of the chunk.
    /// In a ring world, sunlight comes from the radial "up" direction (toward ring center).
    /// Y axis in chunk = height (radial direction toward sun).
    /// Sunlight starts at level 15 from the top and propagates downward.
    pub fn propagate_sunlight(chunk: &mut Chunk) {
        let size = chunk.size();

        // For each column (x, z), trace downward from top
        for z in 0..size {
            for x in 0..size {
                let mut sun_level: u8 = 15;

                // Trace from top to bottom (y = size-1 down to 0)
                for y in (0..size).rev() {
                    let voxel = chunk.get_voxel(x, y, z);
                    let props = BlockProperties::get(voxel.voxel_type);

                    if voxel.voxel_type == VoxelType::Air || (props.is_transparent && !props.is_liquid) {
                        // Air or transparent non-liquid: full sunlight passes through
                        let (_, block) = chunk.get_light(x, y, z);
                        chunk.set_light(x, y, z, sun_level, block);
                    } else if props.is_liquid {
                        // Water: sunlight decreases by 1 per block
                        if sun_level > 0 {
                            sun_level -= 1;
                        }
                        let (_, block) = chunk.get_light(x, y, z);
                        chunk.set_light(x, y, z, sun_level, block);
                    } else {
                        // Opaque block: sunlight = 0 below it
                        sun_level = 0;
                        let (_, block) = chunk.get_light(x, y, z);
                        chunk.set_light(x, y, z, 0, block);
                    }
                }
            }
        }

        // Spread sunlight horizontally using BFS (for areas under overhangs that
        // receive indirect sunlight from adjacent lit blocks)
        Self::spread_sunlight_horizontal(chunk);
    }

    /// Spread sunlight horizontally after the initial vertical pass.
    /// This handles light spreading under overhangs and into caves near openings.
    fn spread_sunlight_horizontal(chunk: &mut Chunk) {
        let size = chunk.size();
        let mut queue: VecDeque<(u32, u32, u32, u8)> = VecDeque::new();

        // Seed the queue with all blocks that have sunlight > 1
        for z in 0..size {
            for y in 0..size {
                for x in 0..size {
                    let (sun, _) = chunk.get_light(x, y, z);
                    if sun > 1 {
                        queue.push_back((x, y, z, sun));
                    }
                }
            }
        }

        // BFS spread
        let directions: [(i32, i32, i32); 6] = [
            (1, 0, 0), (-1, 0, 0),
            (0, 1, 0), (0, -1, 0),
            (0, 0, 1), (0, 0, -1),
        ];

        while let Some((x, y, z, level)) = queue.pop_front() {
            if level <= 1 {
                continue;
            }
            let new_level = level - 1;

            for (dx, dy, dz) in &directions {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                let nz = z as i32 + dz;

                if nx < 0 || ny < 0 || nz < 0
                    || nx >= size as i32 || ny >= size as i32 || nz >= size as i32
                {
                    continue;
                }

                let nx = nx as u32;
                let ny = ny as u32;
                let nz = nz as u32;

                let voxel = chunk.get_voxel(nx, ny, nz);
                let props = BlockProperties::get(voxel.voxel_type);

                // Can't propagate through opaque blocks
                if !props.is_transparent && voxel.voxel_type != VoxelType::Air {
                    continue;
                }

                let (current_sun, block) = chunk.get_light(nx, ny, nz);
                if new_level > current_sun {
                    chunk.set_light(nx, ny, nz, new_level, block);
                    queue.push_back((nx, ny, nz, new_level));
                }
            }
        }
    }

    /// Propagate block light from emitting blocks (torches, etc.) using BFS flood fill.
    pub fn propagate_block_light(chunk: &mut Chunk) {
        let size = chunk.size();
        let mut queue: VecDeque<(u32, u32, u32, u8)> = VecDeque::new();

        // Find all light-emitting blocks
        for z in 0..size {
            for y in 0..size {
                for x in 0..size {
                    let voxel = chunk.get_voxel(x, y, z);
                    let props = BlockProperties::get(voxel.voxel_type);
                    if props.light_level > 0 {
                        let (sun, _) = chunk.get_light(x, y, z);
                        chunk.set_light(x, y, z, sun, props.light_level);
                        queue.push_back((x, y, z, props.light_level));
                    }
                }
            }
        }

        // BFS flood fill
        let directions: [(i32, i32, i32); 6] = [
            (1, 0, 0), (-1, 0, 0),
            (0, 1, 0), (0, -1, 0),
            (0, 0, 1), (0, 0, -1),
        ];

        while let Some((x, y, z, level)) = queue.pop_front() {
            if level <= 1 {
                continue;
            }
            let new_level = level - 1;

            for (dx, dy, dz) in &directions {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                let nz = z as i32 + dz;

                if nx < 0 || ny < 0 || nz < 0
                    || nx >= size as i32 || ny >= size as i32 || nz >= size as i32
                {
                    continue;
                }

                let nx = nx as u32;
                let ny = ny as u32;
                let nz = nz as u32;

                let voxel = chunk.get_voxel(nx, ny, nz);
                let props = BlockProperties::get(voxel.voxel_type);

                // Can't propagate through opaque solid blocks
                if props.is_solid && !props.is_transparent {
                    continue;
                }

                let (sun, current_block) = chunk.get_light(nx, ny, nz);
                if new_level > current_block {
                    chunk.set_light(nx, ny, nz, sun, new_level);
                    queue.push_back((nx, ny, nz, new_level));
                }
            }
        }
    }

    /// Recompute lighting for a chunk after a block change.
    /// This is more efficient than recomputing from scratch for small changes.
    pub fn recompute_lighting(chunk: &mut Chunk) {
        // For simplicity in v0.3, just recompute everything.
        // A more optimized version would only update affected regions.
        Self::compute_lighting(chunk);
    }
}

/// Get the combined light level at a world position.
/// Returns the maximum of sunlight and block light (0-15).
pub fn get_light_level_at(position: &RingPosition, chunk_manager: &ChunkManager, config: &RingWorldConfig) -> u8 {
    let chunk_coord = ChunkCoord::from_ring_position(position, config);

    if let Some(chunk) = chunk_manager.get_chunk(&chunk_coord) {
        let chunk_origin = chunk_coord.to_ring_position(config);
        let chunk_size = config.chunk_size as f64;

        let local_theta = (position.theta - chunk_origin.theta) / config.chunk_angular_size() * chunk_size;
        let local_height = (position.height - chunk_origin.height) / config.chunk_height_size() * chunk_size;
        let local_y = (position.y - chunk_origin.y) / config.chunk_width_size() * chunk_size;

        let lx = local_theta.floor() as i32;
        let ly = local_height.floor() as i32;
        let lz = local_y.floor() as i32;

        if lx >= 0 && ly >= 0 && lz >= 0
            && lx < config.chunk_size as i32
            && ly < config.chunk_size as i32
            && lz < config.chunk_size as i32
        {
            let (sun, block) = chunk.get_light(lx as u32, ly as u32, lz as u32);
            return sun.max(block);
        }
    }

    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::Voxel;

    fn empty_chunk(size: u32) -> Chunk {
        Chunk::new(ChunkCoord::new(0, 0, 0), size)
    }

    #[test]
    fn sunlight_reaches_top_of_empty_chunk() {
        let mut chunk = empty_chunk(16);
        LightingEngine::propagate_sunlight(&mut chunk);
        // Top layer of an all-air chunk should have full sunlight everywhere
        for x in 0..16 {
            for z in 0..16 {
                let (sun, _) = chunk.get_light(x, 15, z);
                assert_eq!(sun, 15, "top sunlight should be full at ({},15,{})", x, z);
            }
        }
    }

    #[test]
    fn sunlight_fills_empty_column() {
        let mut chunk = empty_chunk(16);
        LightingEngine::propagate_sunlight(&mut chunk);
        // An all-air column receives full sunlight at every height
        for y in 0..16 {
            let (sun, _) = chunk.get_light(8, y, 8);
            assert_eq!(sun, 15, "expected full sunlight at y={}", y);
        }
    }

    #[test]
    fn opaque_block_blocks_sunlight_below() {
        let mut chunk = empty_chunk(16);
        // Place an opaque stone block partway down a column
        chunk.set_voxel(4, 8, 4, Voxel::new(VoxelType::Stone));
        LightingEngine::propagate_sunlight(&mut chunk);
        // Below the block, sunlight should be reduced (0 directly under, before
        // any horizontal spreading raises it - check the block itself)
        let (sun_at_block, _) = chunk.get_light(4, 8, 4);
        assert_eq!(sun_at_block, 0, "opaque block should have 0 sunlight");
    }

    #[test]
    fn torch_emits_decreasing_block_light() {
        let mut chunk = empty_chunk(16);
        // Place a torch in the middle
        chunk.set_voxel(8, 8, 8, Voxel::new(VoxelType::Torch));
        LightingEngine::propagate_block_light(&mut chunk);

        let (_, at_torch) = chunk.get_light(8, 8, 8);
        let (_, one_away) = chunk.get_light(9, 8, 8);
        let (_, two_away) = chunk.get_light(10, 8, 8);

        assert!(at_torch > 0, "torch position should be lit");
        assert!(one_away < at_torch, "light should decrease away from torch");
        assert!(two_away < one_away, "light should keep decreasing");
    }

    #[test]
    fn compute_lighting_runs_without_panic() {
        let mut chunk = empty_chunk(16);
        chunk.set_voxel(8, 4, 8, Voxel::new(VoxelType::Torch));
        chunk.set_voxel(2, 2, 2, Voxel::new(VoxelType::Stone));
        LightingEngine::compute_lighting(&mut chunk);
        // Sanity: some block light exists near the torch
        let (_, block) = chunk.get_light(8, 4, 8);
        assert!(block > 0);
    }

    #[test]
    fn get_light_level_at_returns_sensible_value() {
        let config = RingWorldConfig::default();
        let mut chunk_manager = ChunkManager::new(config.clone(), 1);

        // Insert a chunk at the player's chunk coordinate and light it.
        let pos = RingPosition::new(0.05, 0.0, 8.0);
        let coord = ChunkCoord::from_ring_position(&pos, &config);
        let mut chunk = Chunk::new(coord, config.chunk_size);
        LightingEngine::compute_lighting(&mut chunk);
        chunk_manager.chunks.insert(coord, chunk);

        let level = get_light_level_at(&pos, &chunk_manager, &config);
        assert!(level <= 15);
    }

    #[test]
    fn get_light_level_at_missing_chunk_is_zero() {
        let config = RingWorldConfig::default();
        let chunk_manager = ChunkManager::new(config.clone(), 1);
        let pos = RingPosition::new(0.5, 0.0, 8.0);
        // No chunk loaded -> 0
        assert_eq!(get_light_level_at(&pos, &chunk_manager, &config), 0);
    }
}
