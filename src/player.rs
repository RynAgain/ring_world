/// Player module - manages player state on the ring world

use cgmath::{Deg, InnerSpace, Vector3};
#[allow(unused_imports)]
use crate::audio::{AudioEngine, SoundEvent};
use crate::block::{BlockProperties, ToolType, get_base_break_time};
use crate::camera::{Camera, CameraController, CameraUniform, Projection};
use crate::chunk::ChunkManager;
use crate::crafting::CraftingManager;
use crate::inventory::Inventory;
use crate::ring_world::{ChunkCoord, RingPosition, RingWorldConfig};
use crate::voxel::{Voxel, VoxelType};

/// Player bounding box dimensions (in voxel units)
const PLAYER_WIDTH: f64 = 0.6;
const PLAYER_HEIGHT: f64 = 2.0; // 2 blocks tall like Minecraft
const PLAYER_DEPTH: f64 = 0.6;
/// Eye/camera height from feet (near top of head)
const EYE_HEIGHT: f64 = 1.8;
/// Eye height when crouching
const CROUCH_EYE_HEIGHT: f64 = 1.5;

/// Physics constants
const GRAVITY: f64 = 20.0; // voxels/s^2 (downward = decreasing height)
const JUMP_VELOCITY: f64 = 6.93; // sqrt(2 * 20 * 1.2) for max jump height of 1.2 blocks
const TERMINAL_VELOCITY: f64 = 50.0; // max fall speed

/// Movement speeds (blocks/s)
const NORMAL_SPEED: f64 = 4.3;
const SPRINT_SPEED: f64 = 5.6;
const CROUCH_SPEED: f64 = 1.3;
const SWIM_SPEED: f64 = 2.0;

/// Water physics
const WATER_GRAVITY: f64 = 2.0;
const WATER_DRAG: f64 = 0.8;
const WATER_SWIM_UP_VELOCITY: f64 = 3.5;

/// Fall damage
const FALL_DAMAGE_THRESHOLD: f64 = 3.0; // blocks before damage starts
const FALL_DAMAGE_MULTIPLIER: f64 = 1.0; // damage per block above threshold
const MAX_HEALTH: f32 = 20.0;

/// Raycast max distance for block interaction
const RAYCAST_MAX_DISTANCE: f32 = 6.0;

/// Creative mode flying speed
const FLY_SPEED: f64 = 10.0;
/// Vertical fly speed (up/down)
const FLY_VERTICAL_SPEED: f64 = 8.0;

/// Result of a raycast hit
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RaycastHit {
    /// The chunk coordinate containing the hit voxel
    pub chunk_coord: ChunkCoord,
    /// Local voxel position within the chunk
    pub local_x: u32,
    pub local_y: u32,
    pub local_z: u32,
    /// The face normal of the hit (which face was hit)
    pub normal: [i32; 3],
}

/// Block breaking target - tracks which block is currently being broken
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BreakingTarget {
    pub chunk_coord: ChunkCoord,
    pub local_x: u32,
    pub local_y: u32,
    pub local_z: u32,
}

impl BreakingTarget {
    pub fn from_raycast_hit(hit: &RaycastHit) -> Self {
        Self {
            chunk_coord: hit.chunk_coord,
            local_x: hit.local_x,
            local_y: hit.local_y,
            local_z: hit.local_z,
        }
    }
}

/// The player exists on the inner surface of the ring
pub struct Player {
    /// Player's position in ring coordinates
    pub ring_position: RingPosition,
    /// Camera for rendering
    pub camera: Camera,
    /// Camera projection
    pub projection: Projection,
    /// Camera controller for input
    pub camera_controller: CameraController,
    /// Camera uniform for GPU
    pub camera_uniform: CameraUniform,
    /// Movement speed (legacy, kept for compat)
    #[allow(dead_code)]
    pub speed: f32,
    /// Whether the player is on the ground
    pub grounded: bool,
    /// Vertical velocity (for jumping/falling) in height units/sec
    pub vertical_velocity: f64,
    /// Whether jump was requested this frame
    pub jump_requested: bool,

    // --- v0.2 fields ---

    /// Selected block type for placement
    pub selected_block: VoxelType,
    /// Hotbar of available block types
    pub hotbar: [VoxelType; 9],
    /// Currently selected hotbar index (0-8)
    pub hotbar_index: usize,

    /// Whether the player is sprinting
    pub is_sprinting: bool,
    /// Whether the player is crouching
    pub is_crouching: bool,
    /// Whether the player is in water
    pub in_water: bool,

    /// Player health (max 20.0)
    pub health: f32,
    /// Height when the player started falling (for fall damage calculation)
    pub fall_start_height: Option<f64>,
    /// Whether the player was grounded last frame (for detecting transitions)
    was_grounded: bool,

    /// Spawn position for respawning
    pub spawn_position: RingPosition,

    // --- Inventory & Items system ---

    /// Player inventory (slots 0-8 are the hotbar)
    pub inventory: Inventory,
    /// Whether the player is in creative mode
    pub creative_mode: bool,
    /// Whether the inventory UI is open
    pub inventory_open: bool,
    /// Whether the player is flying (creative mode only)
    pub is_flying: bool,
    /// Whether the fly-down key (Shift) is held while flying
    pub fly_down: bool,
    /// Whether the fly-up key (Space) is held while flying
    pub fly_up: bool,

    // --- v0.3 Crafting fields ---

    /// Whether the crafting UI is open
    pub crafting_open: bool,
    /// Whether a crafting table is nearby (within 3 blocks)
    pub near_crafting_table: bool,

    // --- v0.2 World Interaction fields ---

    /// Current block breaking progress (0.0 to 1.0)
    pub breaking_progress: f32,
    /// The block currently being broken
    pub breaking_target: Option<BreakingTarget>,
    /// Whether the left mouse button is currently held
    pub left_mouse_held: bool,
    /// Whether a block is currently in reach (raycast hit within max distance)
    pub target_in_reach: bool,
    /// Current raycast hit result (updated each frame)
    pub current_raycast_hit: Option<RaycastHit>,
    /// Block placement preview position (chunk + local coords of where a block would be placed)
    pub placement_preview: Option<PlacementPreview>,

    // --- v0.4 Quality of Life ---

    /// Whether auto-jump is enabled (auto-climb 1-block ledges while walking).
    pub auto_jump: bool,
}

/// Placement preview data for ghost block rendering
#[derive(Clone, Copy, Debug)]
pub struct PlacementPreview {
    pub chunk_coord: ChunkCoord,
    pub local_x: u32,
    pub local_y: u32,
    pub local_z: u32,
}

impl Player {
    pub fn new(config: &RingWorldConfig, window_width: u32, window_height: u32) -> Self {
        // Provisional spawn point: theta=0, center of the ring width, near the
        // world ceiling. This is only a placeholder — the real, terrain-aware
        // placement happens in `Renderer::try_settle_spawn` once the spawn
        // column's chunks have generated (it calls `find_safe_spawn_height` to
        // drop the player's feet onto the actual surface). Physics is gated off
        // until that settle happens, so the player never free-falls from here or
        // starts embedded in terrain.
        let ring_position = RingPosition::new(0.0, 0.0, config.max_height - PLAYER_HEIGHT - 0.5);
        let cart = ring_position.to_cartesian(config);

        let pos = (cart.x as f32, cart.y as f32, cart.z as f32);
        // Local "up" on the ring is the radial direction toward the center
        // (the sun). Derive it from the spawn position instead of hardcoding a
        // theta=0 value, so the camera frame stays correct if the spawn theta
        // ever changes. At theta=0 this evaluates to (-1, 0, 0) as before.
        let radial = Vector3::new(cart.x as f32, 0.0, cart.z as f32);
        let up = if radial.magnitude2() > 1e-6 {
            (-radial).normalize()
        } else {
            Vector3::new(-1.0, 0.0, 0.0)
        };
        let camera = Camera::new(
            pos,
            Deg(0.0),
            Deg(0.0),
            up,
        );

        // Near plane raised from 0.1 to 0.3 to drastically improve depth-buffer
        // precision. With near=0.1/far=5000 the far/near ratio was 50,000, which
        // crushes Depth32Float precision and makes distant coplanar voxel faces
        // z-fight (the "flickering see-through holes on solid surfaces" bug).
        // Voxels are 1 unit and the eye sits ~1.6 units above the surface, so a
        // 0.3 near plane never clips geometry the player can actually reach.
        let projection = Projection::new(window_width, window_height, Deg(70.0), 0.3, 5000.0);
        let camera_controller = CameraController::new(30.0, 0.5);
        let camera_uniform = CameraUniform::new();

        let hotbar = [
            VoxelType::Stone,
            VoxelType::Dirt,
            VoxelType::Grass,
            VoxelType::Sand,
            VoxelType::Wood,
            VoxelType::Leaves,
            VoxelType::Snow,
            VoxelType::Bedrock,
            VoxelType::Water,
        ];

        Self {
            ring_position,
            camera,
            projection,
            camera_controller,
            camera_uniform,
            speed: 30.0,
            grounded: false,
            vertical_velocity: 0.0,
            jump_requested: false,

            selected_block: hotbar[0],
            hotbar,
            hotbar_index: 0,

            is_sprinting: false,
            is_crouching: false,
            in_water: false,

            health: MAX_HEALTH,
            fall_start_height: None,
            was_grounded: false,

            spawn_position: ring_position,

            inventory: Inventory::new(),
            creative_mode: false,
            inventory_open: false,
            is_flying: false,
            fly_down: false,
            fly_up: false,

            // v0.3 Crafting
            crafting_open: false,
            near_crafting_table: false,

            // v0.2 World Interaction
            breaking_progress: 0.0,
            breaking_target: None,
            left_mouse_held: false,
            target_in_reach: false,
            current_raycast_hit: None,
            placement_preview: None,

            // v0.4 Quality of Life
            auto_jump: true,
        }
    }

    /// Select a hotbar slot (0-indexed)
    pub fn select_hotbar_slot(&mut self, index: usize) {
        if index < 9 {
            self.hotbar_index = index;
            self.selected_block = self.hotbar[index];
        }
    }

    /// Set sprinting state
    pub fn set_sprinting(&mut self, sprinting: bool) {
        self.is_sprinting = sprinting;
    }

    /// Set crouching state
    pub fn set_crouching(&mut self, crouching: bool) {
        self.is_crouching = crouching;
    }

    /// Toggle creative mode
    pub fn toggle_creative_mode(&mut self) {
        self.creative_mode = !self.creative_mode;
        if !self.creative_mode {
            // Exiting creative mode: disable flying
            self.is_flying = false;
        }
    }

    /// Toggle inventory open/closed
    pub fn toggle_inventory(&mut self) {
        self.inventory_open = !self.inventory_open;
    }

    /// Toggle flying (creative mode only)
    pub fn toggle_flying(&mut self) {
        if self.creative_mode {
            self.is_flying = !self.is_flying;
            if self.is_flying {
                self.vertical_velocity = 0.0;
            }
        }
    }

    // --- v0.3 Health/Combat Methods ---

    /// Apply damage to the player from an external source (mob attack, etc.)
    pub fn damage_player(&mut self, amount: f32, config: &RingWorldConfig, chunk_manager: &ChunkManager) {
        if self.creative_mode {
            return; // No damage in creative mode
        }
        self.health -= amount;
        if self.health <= 0.0 {
            self.health = 0.0;
            self.respawn(config, chunk_manager);
        }
    }

    // --- v0.2 World Interaction Methods ---

    /// Get the break speed multiplier based on whether the player has the right tool.
    /// Checks the currently held item against the block's required tool type.
    pub fn break_speed_multiplier(&self, block_tool_type: ToolType) -> f32 {
        // Get the item in the selected hotbar slot
        let held_item = self.inventory.get_hotbar_slot(self.hotbar_index)
            .map(|stack| stack.item_type);

        if let Some(item) = held_item {
            // Check if this item is a tool
            if let Some(tool_type) = CraftingManager::get_tool_type(item) {
                if tool_type == block_tool_type {
                    // Correct tool - apply multiplier
                    return CraftingManager::get_tool_multiplier(item);
                }
            }
        }

        // No tool or wrong tool - base speed
        1.0
    }

    /// Toggle crafting UI open/closed
    pub fn toggle_crafting(&mut self) {
        self.crafting_open = !self.crafting_open;
    }

    /// Check if the current raycast target is within reach distance
    pub fn is_block_in_reach(&self) -> bool {
        self.target_in_reach
    }

    /// Reset breaking progress (called when target changes or button released)
    pub fn reset_breaking(&mut self) {
        self.breaking_progress = 0.0;
        self.breaking_target = None;
    }

    /// Continue breaking the targeted block. Called each frame while left mouse is held.
    /// Returns true if a block was destroyed this frame.
    pub fn continue_breaking(&mut self, dt: f32, config: &RingWorldConfig, chunk_manager: &mut ChunkManager) -> bool {
        if !self.left_mouse_held {
            self.reset_breaking();
            return false;
        }

        // Perform raycast to find current target
        let hit = self.raycast(config, chunk_manager);

        match hit {
            None => {
                // Not looking at any block - reset
                self.reset_breaking();
                false
            }
            Some(hit) => {
                let current_target = BreakingTarget::from_raycast_hit(&hit);

                // Check if target changed
                if self.breaking_target != Some(current_target) {
                    // Target changed - reset and start on new target
                    self.breaking_progress = 0.0;
                    self.breaking_target = Some(current_target);
                }

                // Get block properties to determine break time
                let voxel = if let Some(chunk) = chunk_manager.get_chunk(&hit.chunk_coord) {
                    chunk.get_voxel(hit.local_x, hit.local_y, hit.local_z)
                } else {
                    return false;
                };

                let props = BlockProperties::get(voxel.voxel_type);

                // Unbreakable blocks
                if props.hardness < 0.0 {
                    self.breaking_progress = 0.0;
                    return false;
                }

                // Instant break (hardness == 0)
                if props.hardness == 0.0 {
                    self.breaking_progress = 1.0;
                } else {
                    // Calculate break time using base multiplier (no tool system yet)
                    let break_time = get_base_break_time(props.hardness);
                    let multiplier = self.break_speed_multiplier(props.tool_type);
                    let effective_break_time = break_time / multiplier;
                    self.breaking_progress += dt / effective_break_time;
                }

                // Check if block is fully broken
                if self.breaking_progress >= 1.0 {
                    // Destroy the block. Read first, then edit through the
                    // boundary-aware setter so a block broken on a chunk seam
                    // also re-meshes the neighbor (whose face toward this block
                    // must now be exposed). Using chunk.set_voxel directly here
                    // was the cause of the "broken face at a seam doesn't update
                    // from the player's viewpoint" bug.
                    let voxel_type = chunk_manager
                        .get_chunk(&hit.chunk_coord)
                        .map(|c| c.get_voxel(hit.local_x, hit.local_y, hit.local_z).voxel_type);

                    if let Some(voxel_type) = voxel_type {
                        let props = BlockProperties::get(voxel_type);

                        chunk_manager.set_voxel(
                            &hit.chunk_coord,
                            hit.local_x,
                            hit.local_y,
                            hit.local_z,
                            Voxel::air(),
                        );

                        // Add block drop to inventory (survival mode only)
                        if !self.creative_mode {
                            let drop_type = props.drop.unwrap_or(voxel_type);
                            if drop_type != VoxelType::Air {
                                self.inventory.add_item(drop_type, 1);
                            }
                        }
                    }

                    // Reset breaking state
                    self.reset_breaking();
                    return true;
                }

                false
            }
        }
    }

    /// Update the interaction state each frame (raycast, reach, placement preview)
    pub fn update_interaction(&mut self, config: &RingWorldConfig, chunk_manager: &ChunkManager) {
        // Perform raycast
        let hit = self.raycast(config, chunk_manager);
        self.current_raycast_hit = hit;
        self.target_in_reach = hit.is_some();

        // Update placement preview
        self.placement_preview = None;
        if let Some(hit) = hit {
            // Compute where a new block would be placed (adjacent to hit face)
            let place_x = hit.local_x as i32 + hit.normal[0];
            let place_y = hit.local_y as i32 + hit.normal[1];
            let place_z = hit.local_z as i32 + hit.normal[2];

            let cs = config.chunk_size as i32;

            if place_x >= 0 && place_x < cs
                && place_y >= 0 && place_y < cs
                && place_z >= 0 && place_z < cs
            {
                // Same chunk
                self.placement_preview = Some(PlacementPreview {
                    chunk_coord: hit.chunk_coord,
                    local_x: place_x as u32,
                    local_y: place_y as u32,
                    local_z: place_z as u32,
                });
            } else {
                // Neighbor chunk
                let d_ring = if place_x < 0 { -1 } else if place_x >= cs { 1 } else { 0 };
                let d_height = if place_y < 0 { -1 } else if place_y >= cs { 1 } else { 0 };
                let d_width = if place_z < 0 { -1 } else if place_z >= cs { 1 } else { 0 };
                if let Some(neighbor) = hit.chunk_coord.neighbor(d_ring, d_width, d_height, config) {
                    let fx = if place_x < 0 { cs - 1 } else if place_x >= cs { 0 } else { place_x };
                    let fy = if place_y < 0 { cs - 1 } else if place_y >= cs { 0 } else { place_y };
                    let fz = if place_z < 0 { cs - 1 } else if place_z >= cs { 0 } else { place_z };
                    self.placement_preview = Some(PlacementPreview {
                        chunk_coord: neighbor,
                        local_x: fx as u32,
                        local_y: fy as u32,
                        local_z: fz as u32,
                    });
                }
            }
        }
    }

    /// Get the current movement speed based on state
    fn get_movement_speed(&self) -> f64 {
        if self.is_flying {
            FLY_SPEED
        } else if self.in_water {
            SWIM_SPEED
        } else if self.is_crouching {
            CROUCH_SPEED
        } else if self.is_sprinting {
            SPRINT_SPEED
        } else {
            NORMAL_SPEED
        }
    }

    /// Respawn the player at spawn point.
    ///
    /// Uses the same safe-placement logic as the initial spawn: it scans the
    /// spawn column for the surface and puts the player's feet on top of it,
    /// so the player never respawns embedded in / below terrain.
    fn respawn(&mut self, config: &RingWorldConfig, chunk_manager: &ChunkManager) {
        let theta = self.spawn_position.theta;
        let y = self.spawn_position.y;
        let feet = find_safe_spawn_height(theta, y, chunk_manager, config);

        self.ring_position = RingPosition::new(theta, y, feet + PLAYER_HEIGHT * 0.5);
        self.spawn_position = self.ring_position;
        self.vertical_velocity = 0.0;
        self.grounded = false;
        self.health = MAX_HEALTH;
        self.fall_start_height = None;
        self.is_sprinting = false;
        self.is_crouching = false;
    }

    /// Set the player's respawn point to the given ring position.
    ///
    /// Used by "set spawn" gameplay logic (e.g. sleeping in a bed or activating
    /// a respawn anchor). The position is clamped into the ring's valid bounds
    /// first so the stored spawn can never be out-of-range; on the next death
    /// `respawn()` still runs `find_safe_spawn_height` on this (theta, y) column,
    /// so the player lands safely on top of the surface there rather than at the
    /// exact stored height.
    pub fn set_spawn(&mut self, mut position: RingPosition, config: &RingWorldConfig) {
        position.clamp(config);
        self.spawn_position = position;
    }

    /// Set the player's respawn point to the current position (convenience).
    pub fn set_spawn_here(&mut self, config: &RingWorldConfig) {
        let here = self.ring_position;
        self.set_spawn(here, config);
    }

    /// Update player state — applies movement input to ring position, then updates camera
    pub fn update(&mut self, dt: std::time::Duration, config: &RingWorldConfig) {
        let dt_secs = dt.as_secs_f64().min(0.1);
        
        // Get movement input from camera controller
        let (move_forward, move_right, _move_up) = self.camera_controller.get_movement();
        
        // Stop sprinting if not moving forward
        if move_forward <= 0.0 {
            self.is_sprinting = false;
        }

        let speed = self.get_movement_speed();
        let forward = self.camera.forward();
        let right = self.camera.right();
        
        // Project forward and right onto the ring surface (remove the radial component)
        let up = self.camera.up;
        
        // Forward projected onto surface (remove up component)
        let fwd_proj_x = forward.x as f64 - (forward.x * up.x + forward.y * up.y + forward.z * up.z) as f64 * up.x as f64;
        let fwd_proj_y = forward.y as f64 - (forward.x * up.x + forward.y * up.y + forward.z * up.z) as f64 * up.y as f64;
        let fwd_proj_z = forward.z as f64 - (forward.x * up.x + forward.y * up.y + forward.z * up.z) as f64 * up.z as f64;
        let fwd_len = (fwd_proj_x * fwd_proj_x + fwd_proj_y * fwd_proj_y + fwd_proj_z * fwd_proj_z).sqrt();
        
        // Right projected onto surface
        let rgt_proj_x = right.x as f64 - (right.x * up.x + right.y * up.y + right.z * up.z) as f64 * up.x as f64;
        let rgt_proj_y = right.y as f64 - (right.x * up.x + right.y * up.y + right.z * up.z) as f64 * up.y as f64;
        let rgt_proj_z = right.z as f64 - (right.x * up.x + right.y * up.y + right.z * up.z) as f64 * up.z as f64;
        let rgt_len = (rgt_proj_x * rgt_proj_x + rgt_proj_y * rgt_proj_y + rgt_proj_z * rgt_proj_z).sqrt();
        
        // Compute world-space displacement
        let mut dx = 0.0f64;
        let mut dy = 0.0f64;
        let mut dz = 0.0f64;
        
        if fwd_len > 0.001 {
            let f = move_forward as f64 * speed * dt_secs / fwd_len;
            dx += fwd_proj_x * f;
            dy += fwd_proj_y * f;
            dz += fwd_proj_z * f;
        }
        if rgt_len > 0.001 {
            let r = move_right as f64 * speed * dt_secs / rgt_len;
            dx += rgt_proj_x * r;
            dy += rgt_proj_y * r;
            dz += rgt_proj_z * r;
        }
        
        // Convert current position to Cartesian, apply displacement, convert back
        let current_cart = self.ring_position.to_cartesian(config);
        let new_cart = Vector3::new(current_cart.x + dx, current_cart.y + dy, current_cart.z + dz);
        let new_ring = RingPosition::from_cartesian(new_cart, config);
        
        // Update theta and y from the new position (keep height from physics)
        self.ring_position.theta = new_ring.theta;
        self.ring_position.y = new_ring.y;
        
        // Clamp to ring bounds (theta wraps, y clamps to edges)
        self.ring_position.clamp(config);

        // Update camera position from ring position (at eye level).
        // Smooth Camera (v0.4 QoL): lerp the camera toward the target eye
        // position each frame instead of snapping, for smoother movement.
        let eye_height = if self.is_crouching { CROUCH_EYE_HEIGHT } else { EYE_HEIGHT };
        let eye_offset = eye_height - PLAYER_HEIGHT * 0.5;
        let mut eye_pos = self.ring_position;
        eye_pos.height += eye_offset;
        let cart = eye_pos.to_cartesian(config);
        let target = cgmath::Point3::new(cart.x as f32, cart.y as f32, cart.z as f32);
        // On the very first frame (camera far from target) snap to avoid a slide.
        let dx = (target.x - self.camera.position.x) as f64;
        let dy = (target.y - self.camera.position.y) as f64;
        let dz = (target.z - self.camera.position.z) as f64;
        let dist_sq = dx * dx + dy * dy + dz * dz;
        if dist_sq > 25.0 {
            self.camera.position = target;
        } else {
            self.camera.lerp_position(target, 0.15, dt_secs as f32);
        }
        self.camera.update_up_from_position();

        // Apply mouse rotation
        self.camera_controller.update_rotation(&mut self.camera, dt);

        // Update camera uniform
        self.camera_uniform.update_view_proj(&self.camera, &self.projection);
    }

    /// Update player physics (gravity, jumping, collision)
    pub fn update_physics(&mut self, dt: std::time::Duration, config: &RingWorldConfig, chunk_manager: &ChunkManager) {
        let dt_secs = dt.as_secs_f64().min(0.1);

        // Creative mode flying: no gravity, vertical movement via Space/Shift
        if self.is_flying && self.creative_mode {
            let mut vertical_move = 0.0;
            if self.fly_up {
                vertical_move += FLY_VERTICAL_SPEED * dt_secs;
            }
            if self.fly_down {
                vertical_move -= FLY_VERTICAL_SPEED * dt_secs;
            }
            self.ring_position.height += vertical_move;
            self.ring_position.height = self.ring_position.height.clamp(1.0, config.max_height - 1.0);
            self.vertical_velocity = 0.0;
            self.grounded = false;
            self.fall_start_height = None;
            self.jump_requested = false;
            self.was_grounded = false;
            return;
        }

        // Check if player is in water (check at body center)
        let body_check = RingPosition::new(
            self.ring_position.theta,
            self.ring_position.y,
            self.ring_position.height,
        );
        self.in_water = is_position_water(&body_check, config, chunk_manager);

        // Handle jump request
        if self.jump_requested {
            if self.in_water {
                self.vertical_velocity = WATER_SWIM_UP_VELOCITY;
            } else if self.grounded {
                self.vertical_velocity = JUMP_VELOCITY;
                self.grounded = false;
            }
        }
        self.jump_requested = false;

        // Track fall start height for fall damage
        if self.was_grounded && !self.grounded && !self.in_water {
            // Just left the ground - record height
            self.fall_start_height = Some(self.ring_position.height);
        }
        
        if self.grounded {
            // When grounded, don't apply gravity - just check if we're still on solid ground
            let feet_height = self.ring_position.height - PLAYER_HEIGHT * 0.5;
            let ground_check = RingPosition::new(
                self.ring_position.theta,
                self.ring_position.y,
                feet_height - 0.05,
            );
            if !is_position_solid(&ground_check, config, chunk_manager) {
                self.grounded = false;
            }
        } else if self.in_water {
            // Swimming physics - reduced gravity
            self.vertical_velocity -= WATER_GRAVITY * dt_secs;
            // Water drag (frame-rate independent)
            let drag_factor = WATER_DRAG.powf(dt_secs * 60.0);
            self.vertical_velocity *= drag_factor;
            // Clamp fall speed in water
            if self.vertical_velocity < -5.0 {
                self.vertical_velocity = -5.0;
            }
            
            // Apply vertical velocity in water
            let new_height = self.ring_position.height + self.vertical_velocity * dt_secs;
            
            // Check for landing on solid ground even in water
            let new_feet = new_height - PLAYER_HEIGHT * 0.5;
            let ground_check = RingPosition::new(
                self.ring_position.theta,
                self.ring_position.y,
                new_feet - 0.01,
            );
            if self.vertical_velocity <= 0.0 && is_position_solid(&ground_check, config, chunk_manager) {
                let block_top = new_feet.floor() + 1.0;
                self.ring_position.height = block_top + PLAYER_HEIGHT * 0.5;
                self.vertical_velocity = 0.0;
                self.grounded = true;
                // Water negates fall damage
                self.fall_start_height = None;
            } else {
                self.ring_position.height = new_height;
            }
        } else {
            // Normal air physics
            self.vertical_velocity -= GRAVITY * dt_secs;
            if self.vertical_velocity < -TERMINAL_VELOCITY {
                self.vertical_velocity = -TERMINAL_VELOCITY;
            }

            // Apply vertical velocity to height
            let new_height = self.ring_position.height + self.vertical_velocity * dt_secs;
            
            // Check for landing (only when falling)
            if self.vertical_velocity <= 0.0 {
                let new_feet = new_height - PLAYER_HEIGHT * 0.5;
                let ground_check = RingPosition::new(
                    self.ring_position.theta,
                    self.ring_position.y,
                    new_feet - 0.01,
                );
                if is_position_solid(&ground_check, config, chunk_manager) {
                    // Land: snap feet to top of the block
                    let block_top = new_feet.floor() + 1.0;
                    self.ring_position.height = block_top + PLAYER_HEIGHT * 0.5;
                    self.vertical_velocity = 0.0;
                    self.grounded = true;

                    // Calculate fall damage (skip in creative mode)
                    if !self.creative_mode {
                        if let Some(start_height) = self.fall_start_height {
                            let fall_distance = start_height - self.ring_position.height;
                            if fall_distance > FALL_DAMAGE_THRESHOLD {
                                let damage = ((fall_distance - FALL_DAMAGE_THRESHOLD) * FALL_DAMAGE_MULTIPLIER) as f32;
                                self.health -= damage;
                                if self.health <= 0.0 {
                                    self.respawn(config, chunk_manager);
                                    self.was_grounded = self.grounded;
                                    return;
                                }
                            }
                        }
                    }
                    self.fall_start_height = None;
                } else {
                    self.ring_position.height = new_height;
                }
            } else {
                // Moving up - update fall start height if going higher
                self.ring_position.height = new_height;
                if let Some(ref mut start) = self.fall_start_height {
                    if self.ring_position.height > *start {
                        *start = self.ring_position.height;
                    }
                }
                
                // Check head bonk
                let head_height = self.ring_position.height + PLAYER_HEIGHT * 0.5;
                let head_check = RingPosition::new(
                    self.ring_position.theta,
                    self.ring_position.y,
                    head_height + 0.01,
                );
                if is_position_solid(&head_check, config, chunk_manager) {
                    self.vertical_velocity = 0.0;
                }
            }
        }

        // Auto-jump: detect 1-block ledges in the movement direction and jump.
        if self.auto_jump && self.grounded && !self.in_water {
            self.try_auto_jump(config, chunk_manager);
        }

        // AABB collision for horizontal movement
        self.check_horizontal_collision(config, chunk_manager);

        // Crouch edge prevention: if crouching and grounded, prevent moving off edges
        if self.is_crouching && self.grounded {
            self.prevent_edge_fall(config, chunk_manager);
        }

        // Clamp height to valid range
        self.ring_position.height = self.ring_position.height.clamp(1.0, config.max_height - 1.0);

        // Update was_grounded for next frame
        self.was_grounded = self.grounded;
    }

    /// Prevent the player from walking off edges while crouching
    fn prevent_edge_fall(&mut self, config: &RingWorldConfig, chunk_manager: &ChunkManager) {
        let half_w = PLAYER_WIDTH * 0.5;
        let half_d = PLAYER_DEPTH * 0.5;
        let feet_height = self.ring_position.height - PLAYER_HEIGHT * 0.5;

        // Check if there's solid ground below each corner of the player's bounding box
        let corners = [
            (self.ring_position.theta - half_w / config.radius, self.ring_position.y - half_d),
            (self.ring_position.theta + half_w / config.radius, self.ring_position.y - half_d),
            (self.ring_position.theta - half_w / config.radius, self.ring_position.y + half_d),
            (self.ring_position.theta + half_w / config.radius, self.ring_position.y + half_d),
        ];

        for (theta, y) in &corners {
            let check = RingPosition::new(*theta, *y, feet_height - 0.1);
            if !is_position_solid(&check, config, chunk_manager) {
                // This corner is unsupported - push player back toward center
                let d_theta = *theta - self.ring_position.theta;
                let d_y = *y - self.ring_position.y;
                
                if d_theta.abs() > 0.0001 {
                    self.ring_position.theta -= d_theta * 0.5;
                }
                if d_y.abs() > 0.0001 {
                    self.ring_position.y -= d_y * 0.5;
                }
            }
        }
    }

    /// Auto-jump: if the player is walking into a 1-block-high ledge (solid at
    /// feet level ahead, but air directly above that block), trigger a jump so
    /// they smoothly step up onto it.
    fn try_auto_jump(&mut self, config: &RingWorldConfig, chunk_manager: &ChunkManager) {
        // Determine horizontal movement direction from input.
        let (move_forward, move_right, _) = self.camera_controller.get_movement();
        if move_forward.abs() < 0.01 && move_right.abs() < 0.01 {
            return; // Not moving
        }

        // Build a surface-tangent movement direction in world space, matching
        // the projection used in `update()`.
        let forward = self.camera.forward();
        let right = self.camera.right();
        let up = self.camera.up;

        let dot_f = forward.x * up.x + forward.y * up.y + forward.z * up.z;
        let fwd = Vector3::new(
            forward.x as f64 - dot_f as f64 * up.x as f64,
            forward.y as f64 - dot_f as f64 * up.y as f64,
            forward.z as f64 - dot_f as f64 * up.z as f64,
        );
        let dot_r = right.x * up.x + right.y * up.y + right.z * up.z;
        let rgt = Vector3::new(
            right.x as f64 - dot_r as f64 * up.x as f64,
            right.y as f64 - dot_r as f64 * up.y as f64,
            right.z as f64 - dot_r as f64 * up.z as f64,
        );

        let mut dir = Vector3::new(
            fwd.x * move_forward as f64 + rgt.x * move_right as f64,
            fwd.y * move_forward as f64 + rgt.y * move_right as f64,
            fwd.z * move_forward as f64 + rgt.z * move_right as f64,
        );
        let dir_len = (dir.x * dir.x + dir.y * dir.y + dir.z * dir.z).sqrt();
        if dir_len < 0.001 {
            return;
        }
        dir.x /= dir_len;
        dir.y /= dir_len;
        dir.z /= dir_len;

        // Sample a point ~0.7 blocks ahead of the player in the move direction.
        let probe_dist = 0.7;
        let current_cart = self.ring_position.to_cartesian(config);
        let ahead_cart = Vector3::new(
            current_cart.x + dir.x * probe_dist,
            current_cart.y + dir.y * probe_dist,
            current_cart.z + dir.z * probe_dist,
        );
        let ahead = RingPosition::from_cartesian(ahead_cart, config);

        let feet_height = self.ring_position.height - PLAYER_HEIGHT * 0.5;

        // Block at the player's feet level, in front (the ledge).
        let ledge = RingPosition::new(ahead.theta, ahead.y, feet_height + 0.1);
        // The space one block above the ledge must be clear to step into.
        let above_ledge = RingPosition::new(ahead.theta, ahead.y, feet_height + 1.1);
        // The space two blocks above must also be clear (player is 2 tall).
        let above_ledge2 = RingPosition::new(ahead.theta, ahead.y, feet_height + 2.0);

        if is_position_solid(&ledge, config, chunk_manager)
            && !is_position_solid(&above_ledge, config, chunk_manager)
            && !is_position_solid(&above_ledge2, config, chunk_manager)
        {
            // 1-block ledge ahead with clearance above: auto-jump.
            self.vertical_velocity = JUMP_VELOCITY;
            self.grounded = false;
        }
    }

    /// Check and resolve horizontal collisions using AABB with iterative resolution
    fn check_horizontal_collision(&mut self, config: &RingWorldConfig, chunk_manager: &ChunkManager) {
        let half_w = PLAYER_WIDTH * 0.5;
        let half_d = PLAYER_DEPTH * 0.5;

        // Run multiple iterations to handle corner cases and ensure full push-out
        for _iter in 0..3 {
            let feet = self.ring_position.height - PLAYER_HEIGHT * 0.5;
            let head = self.ring_position.height + PLAYER_HEIGHT * 0.5;

            let theta_min = self.ring_position.theta - half_w / config.radius;
            let theta_max = self.ring_position.theta + half_w / config.radius;
            let y_min = self.ring_position.y - half_d;
            let y_max = self.ring_position.y + half_d;

            let h_start = feet.floor() as i32;
            let h_end = (head).ceil() as i32;

            let arc_block_min = ((theta_min * config.radius).floor() as i32) - 1;
            let arc_block_max = ((theta_max * config.radius).ceil() as i32) + 1;
            let y_block_min = (y_min.floor() as i32) - 1;
            let y_block_max = (y_max.ceil() as i32) + 1;

            let mut min_penetration = f64::MAX;
            let mut push_axis = 0; // 0=none, 1=theta, 2=y
            let mut push_amount = 0.0f64;

            for bh in h_start..h_end {
                for by in y_block_min..y_block_max {
                    for ba in arc_block_min..arc_block_max {
                        let block_theta = ba as f64 / config.radius;
                        let block_y = by as f64;
                        let block_h = bh as f64;

                        let check_pos = RingPosition::new(
                            block_theta + 0.5 / config.radius,
                            block_y + 0.5,
                            block_h + 0.5,
                        );
                        if !is_position_solid(&check_pos, config, chunk_manager) {
                            continue;
                        }

                        let block_arc_min = ba as f64;
                        let block_arc_max = (ba + 1) as f64;
                        let block_y_min = by as f64;
                        let block_y_max = (by + 1) as f64;
                        let block_h_min = bh as f64;
                        let block_h_max = (bh + 1) as f64;

                        let player_arc_min = self.ring_position.theta * config.radius - half_w;
                        let player_arc_max = self.ring_position.theta * config.radius + half_w;
                        let player_y_min = self.ring_position.y - half_d;
                        let player_y_max = self.ring_position.y + half_d;
                        let player_h_min = feet;
                        let player_h_max = head;

                        let overlap_arc = (player_arc_max.min(block_arc_max) - player_arc_min.max(block_arc_min)).max(0.0);
                        let overlap_y = (player_y_max.min(block_y_max) - player_y_min.max(block_y_min)).max(0.0);
                        let overlap_h = (player_h_max.min(block_h_max) - player_h_min.max(block_h_min)).max(0.0);

                        if overlap_arc > 0.0 && overlap_y > 0.0 && overlap_h > 0.0 {
                            let player_arc_center = self.ring_position.theta * config.radius;
                            let block_arc_center = (block_arc_min + block_arc_max) * 0.5;
                            let pen_arc = if player_arc_center < block_arc_center {
                                player_arc_max - block_arc_min
                            } else {
                                -(block_arc_max - player_arc_min)
                            };

                            let player_y_center = self.ring_position.y;
                            let block_y_center = (block_y_min + block_y_max) * 0.5;
                            let pen_y = if player_y_center < block_y_center {
                                player_y_max - block_y_min
                            } else {
                                -(block_y_max - player_y_min)
                            };

                            if pen_arc.abs() < pen_y.abs() {
                                if pen_arc.abs() < min_penetration {
                                    min_penetration = pen_arc.abs();
                                    push_axis = 1;
                                    push_amount = -pen_arc;
                                }
                            } else {
                                if pen_y.abs() < min_penetration {
                                    min_penetration = pen_y.abs();
                                    push_axis = 2;
                                    push_amount = -pen_y;
                                }
                            }
                        }
                    }
                }
            }

            match push_axis {
                1 => {
                    self.ring_position.theta += push_amount / config.radius;
                }
                2 => {
                    self.ring_position.y += push_amount;
                }
                _ => break,
            }
        }
    }

    /// Request a jump (will be processed next physics update)
    pub fn request_jump(&mut self) {
        self.jump_requested = true;
    }

    /// Perform a raycast from the camera to find the block being looked at
    pub fn raycast(&self, config: &RingWorldConfig, chunk_manager: &ChunkManager) -> Option<RaycastHit> {
        let origin = Vector3::new(
            self.camera.position.x as f64,
            self.camera.position.y as f64,
            self.camera.position.z as f64,
        );
        let forward = self.camera.forward();
        let direction = Vector3::new(
            forward.x as f64,
            forward.y as f64,
            forward.z as f64,
        );

        let step_size = 0.1;
        let max_steps = (RAYCAST_MAX_DISTANCE as f64 / step_size) as usize;

        let mut prev_ring_pos: Option<(u32, u32, u32, ChunkCoord)> = None;

        for i in 0..max_steps {
            let t = i as f64 * step_size;
            let world_pos = origin + direction * t;

            let ring_pos = RingPosition::from_cartesian(world_pos, config);
            
            if !ring_pos.is_valid(config) {
                continue;
            }

            let chunk_coord = ChunkCoord::from_ring_position(&ring_pos, config);
            let chunk_origin = chunk_coord.to_ring_position(config);
            let chunk_size = config.chunk_size as f64;

            let local_theta = (ring_pos.theta - chunk_origin.theta) / config.chunk_angular_size() * chunk_size;
            let local_height = (ring_pos.height - chunk_origin.height) / config.chunk_height_size() * chunk_size;
            let local_y = (ring_pos.y - chunk_origin.y) / config.chunk_width_size() * chunk_size;

            let lx = local_theta.floor() as i32;
            let ly = local_height.floor() as i32;
            let lz = local_y.floor() as i32;

            if lx < 0 || ly < 0 || lz < 0 || lx >= config.chunk_size as i32 || ly >= config.chunk_size as i32 || lz >= config.chunk_size as i32 {
                continue;
            }

            let lx = lx as u32;
            let ly = ly as u32;
            let lz = lz as u32;

            if let Some(chunk) = chunk_manager.get_chunk(&chunk_coord) {
                let voxel = chunk.get_voxel(lx, ly, lz);
                if voxel.voxel_type != VoxelType::Air && voxel.voxel_type != VoxelType::Water {
                    let normal = if let Some((px, py, pz, _prev_coord)) = prev_ring_pos {
                        let dx = lx as i32 - px as i32;
                        let dy = ly as i32 - py as i32;
                        let dz = lz as i32 - pz as i32;
                        [-dx.signum(), -dy.signum(), -dz.signum()]
                    } else {
                        [0, 1, 0]
                    };

                    return Some(RaycastHit {
                        chunk_coord,
                        local_x: lx,
                        local_y: ly,
                        local_z: lz,
                        normal,
                    });
                }

                prev_ring_pos = Some((lx, ly, lz, chunk_coord));
            }
        }

        None
    }

    /// Destroy the block the player is looking at
    pub fn destroy_block(&mut self, config: &RingWorldConfig, chunk_manager: &mut ChunkManager) -> bool {
        if let Some(hit) = self.raycast(config, chunk_manager) {
            // Read the target voxel first (immutable borrow), then edit via the
            // boundary-aware setter so neighbor chunk meshes are also rebuilt.
            let voxel_type = match chunk_manager.get_chunk(&hit.chunk_coord) {
                Some(chunk) => chunk.get_voxel(hit.local_x, hit.local_y, hit.local_z).voxel_type,
                None => return false,
            };
            let props = BlockProperties::get(voxel_type);

            if props.hardness < 0.0 {
                return false;
            }

            // set_voxel here marks the edited chunk AND any boundary-adjacent
            // neighbor dirty, so a face exposed at a chunk seam updates.
            chunk_manager.set_voxel(&hit.chunk_coord, hit.local_x, hit.local_y, hit.local_z, Voxel::air());

            // Add block drop to inventory (survival mode only)
            if !self.creative_mode {
                let drop_type = props.drop.unwrap_or(voxel_type);
                // Don't add Air drops
                if drop_type != VoxelType::Air {
                    self.inventory.add_item(drop_type, 1);
                }
            }

            return true;
        }
        false
    }

    /// Place a block adjacent to the face the player is looking at
    pub fn place_block(&mut self, block_type: VoxelType, config: &RingWorldConfig, chunk_manager: &mut ChunkManager) -> bool {
        // In survival mode, check if we have the item in the selected hotbar slot
        if !self.creative_mode {
            let slot = self.inventory.get_hotbar_slot(self.hotbar_index);
            match slot {
                Some(stack) if stack.item_type == block_type && stack.count > 0 => {}
                _ => return false, // No items to place
            }
        }

        if let Some(hit) = self.raycast(config, chunk_manager) {
            let place_x = hit.local_x as i32 + hit.normal[0];
            let place_y = hit.local_y as i32 + hit.normal[1];
            let place_z = hit.local_z as i32 + hit.normal[2];

            let cs = config.chunk_size as i32;
            let (target_coord, final_x, final_y, final_z) = if place_x >= 0 && place_x < cs
                && place_y >= 0 && place_y < cs
                && place_z >= 0 && place_z < cs
            {
                (hit.chunk_coord, place_x, place_y, place_z)
            } else {
                let d_ring = if place_x < 0 { -1 } else if place_x >= cs { 1 } else { 0 };
                let d_height = if place_y < 0 { -1 } else if place_y >= cs { 1 } else { 0 };
                let d_width = if place_z < 0 { -1 } else if place_z >= cs { 1 } else { 0 };
                if let Some(neighbor) = hit.chunk_coord.neighbor(d_ring, d_width, d_height, config) {
                    let fx = if place_x < 0 { cs - 1 } else if place_x >= cs { 0 } else { place_x };
                    let fy = if place_y < 0 { cs - 1 } else if place_y >= cs { 0 } else { place_y };
                    let fz = if place_z < 0 { cs - 1 } else if place_z >= cs { 0 } else { place_z };
                    (neighbor, fx, fy, fz)
                } else {
                    return false;
                }
            };

            // Calculate the ring position of the block to be placed
            let block_origin = target_coord.to_ring_position(config);
            let block_theta = block_origin.theta + (final_x as f64 + 0.5) / config.chunk_size as f64 * config.chunk_angular_size();
            let block_height = block_origin.height + (final_y as f64) / config.chunk_size as f64 * config.chunk_height_size();
            let block_y = block_origin.y + (final_z as f64) / config.chunk_size as f64 * config.chunk_width_size();

            // Check if the block's AABB would overlap with the player's AABB
            let half_w = PLAYER_WIDTH * 0.5;
            let half_d = PLAYER_DEPTH * 0.5;
            let player_arc_min = self.ring_position.theta * config.radius - half_w;
            let player_arc_max = self.ring_position.theta * config.radius + half_w;
            let player_y_min = self.ring_position.y - half_d;
            let player_y_max = self.ring_position.y + half_d;
            let player_h_min = self.ring_position.height - PLAYER_HEIGHT * 0.5;
            let player_h_max = self.ring_position.height + PLAYER_HEIGHT * 0.5;

            let block_arc_center = block_theta * config.radius;
            let block_arc_min = block_arc_center - 0.5;
            let block_arc_max = block_arc_center + 0.5;
            let block_y_min = block_y;
            let block_y_max = block_y + config.chunk_width_size() / config.chunk_size as f64;
            let block_h_min = block_height;
            let block_h_max = block_height + config.chunk_height_size() / config.chunk_size as f64;

            let overlap_arc = player_arc_max > block_arc_min && player_arc_min < block_arc_max;
            let overlap_y = player_y_max > block_y_min && player_y_min < block_y_max;
            let overlap_h = player_h_max > block_h_min && player_h_min < block_h_max;

            if overlap_arc && overlap_y && overlap_h {
                return false;
            }

            // Place the block. Read first (immutable), then edit via the
            // boundary-aware setter so a block placed against a chunk seam also
            // re-meshes the neighbor (whose face toward the new block must now
            // be culled).
            let is_air = match chunk_manager.get_chunk(&target_coord) {
                Some(chunk) => {
                    chunk.get_voxel(final_x as u32, final_y as u32, final_z as u32).voxel_type
                        == VoxelType::Air
                }
                None => false,
            };
            if is_air {
                chunk_manager.set_voxel(
                    &target_coord,
                    final_x as u32,
                    final_y as u32,
                    final_z as u32,
                    Voxel::new(block_type),
                );
                // Consume item from inventory in survival mode
                if !self.creative_mode {
                    self.inventory.consume_from_hotbar(self.hotbar_index);
                }
                return true;
            }
        }
        false
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.projection.resize(width, height);
    }
}

/// Find a safe feet height for spawning/respawning at the given (theta, y).
///
/// Scans the column from the top of the world downward to find the highest
/// solid voxel, then places the player's feet exactly on top of it
/// (feet = solid_top + 1). It then verifies the player's 2-block-tall AABB does
/// not intersect any solid voxel and raises the player until it is clear. If no
/// solid ground is found in the column, it returns a height near the top of the
/// world so the player can still fall safely onto generated terrain.
///
/// Returns the FEET height (the player's body-center height is
/// `feet + PLAYER_HEIGHT * 0.5`).
pub fn find_safe_spawn_height(
    theta: f64,
    y: f64,
    chunk_manager: &ChunkManager,
    config: &RingWorldConfig,
) -> f64 {
    // Scan from just below the world ceiling down to the floor.
    let mut surface_top: Option<f64> = None;
    let mut h = config.max_height - 0.5;
    while h > 0.0 {
        let sample = RingPosition::new(theta, y, h);
        if is_position_solid(&sample, config, chunk_manager) {
            // Top surface of this solid voxel is the next integer boundary above
            // the voxel's floor.
            surface_top = Some(h.floor() + 1.0);
            break;
        }
        h -= 1.0;
    }

    // Feet land on top of the surface; default to a safe high point if none.
    let mut feet = match surface_top {
        Some(top) => top,
        None => (config.max_height - PLAYER_HEIGHT - 1.0).max(1.0),
    };

    // Verify the player's AABB (PLAYER_HEIGHT tall, feet-origin) is clear of
    // solid voxels; if not, raise the player one block at a time. Sample at
    // several heights spanning feet..feet+PLAYER_HEIGHT.
    let max_clamp = config.max_height - PLAYER_HEIGHT - 0.5;
    let mut guard = 0;
    loop {
        let mut blocked = false;
        // Sample feet, mid, and just-below-head to cover the 2-block body.
        let samples = [feet + 0.1, feet + 1.0, feet + PLAYER_HEIGHT - 0.1];
        for &sh in &samples {
            let p = RingPosition::new(theta, y, sh);
            if is_position_solid(&p, config, chunk_manager) {
                blocked = true;
                break;
            }
        }
        if !blocked || guard > config.chunk_size as i32 * 4 {
            break;
        }
        feet += 1.0;
        if feet > max_clamp {
            feet = max_clamp.max(1.0);
            break;
        }
        guard += 1;
    }

    feet.clamp(1.0, (config.max_height - PLAYER_HEIGHT - 0.5).max(1.0))
}

/// Check if a ring position corresponds to a solid voxel
fn is_position_solid(pos: &RingPosition, config: &RingWorldConfig, chunk_manager: &ChunkManager) -> bool {
    if !pos.is_valid(config) {
        return pos.height <= 0.0;
    }

    let chunk_coord = ChunkCoord::from_ring_position(pos, config);
    
    if let Some(chunk) = chunk_manager.get_chunk(&chunk_coord) {
        let chunk_origin = chunk_coord.to_ring_position(config);
        let chunk_size = config.chunk_size as f64;

        let local_theta = (pos.theta - chunk_origin.theta) / config.chunk_angular_size() * chunk_size;
        let local_height = (pos.height - chunk_origin.height) / config.chunk_height_size() * chunk_size;
        let local_y = (pos.y - chunk_origin.y) / config.chunk_width_size() * chunk_size;

        let lx = local_theta.floor() as i32;
        let ly = local_height.floor() as i32;
        let lz = local_y.floor() as i32;

        if lx >= 0 && ly >= 0 && lz >= 0
            && lx < config.chunk_size as i32
            && ly < config.chunk_size as i32
            && lz < config.chunk_size as i32
        {
            let voxel = chunk.get_voxel(lx as u32, ly as u32, lz as u32);
            return voxel.voxel_type.is_solid();
        }
    }

    false
}

/// Check if a ring position corresponds to a water voxel
fn is_position_water(pos: &RingPosition, config: &RingWorldConfig, chunk_manager: &ChunkManager) -> bool {
    if !pos.is_valid(config) {
        return false;
    }

    let chunk_coord = ChunkCoord::from_ring_position(pos, config);
    
    if let Some(chunk) = chunk_manager.get_chunk(&chunk_coord) {
        let chunk_origin = chunk_coord.to_ring_position(config);
        let chunk_size = config.chunk_size as f64;

        let local_theta = (pos.theta - chunk_origin.theta) / config.chunk_angular_size() * chunk_size;
        let local_height = (pos.height - chunk_origin.height) / config.chunk_height_size() * chunk_size;
        let local_y = (pos.y - chunk_origin.y) / config.chunk_width_size() * chunk_size;

        let lx = local_theta.floor() as i32;
        let ly = local_height.floor() as i32;
        let lz = local_y.floor() as i32;

        if lx >= 0 && ly >= 0 && lz >= 0
            && lx < config.chunk_size as i32
            && ly < config.chunk_size as i32
            && lz < config.chunk_size as i32
        {
            let voxel = chunk.get_voxel(lx as u32, ly as u32, lz as u32);
            return voxel.voxel_type == VoxelType::Water;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::{Chunk, ChunkManager};

    /// Build a chunk manager with a single solid column of `stone` from the
    /// bottom up to (and including) local y = `top_local_y` in the spawn chunk.
    fn manager_with_floor(top_local_y: u32) -> (ChunkManager, RingWorldConfig) {
        let config = RingWorldConfig::default();
        let mut manager = ChunkManager::new(config.clone(), 2);

        // Spawn at theta=0, y=0: figure out which chunk that maps to at height 0.
        let spawn_pos = RingPosition::new(0.0, 0.0, 0.0);
        let coord = ChunkCoord::from_ring_position(&spawn_pos, &config);
        let mut chunk = Chunk::new(coord, config.chunk_size);

        // Find local (x,z) for theta=0, y=0 in this chunk.
        let origin = coord.to_ring_position(&config);
        let cs = config.chunk_size as f64;
        let lx = ((spawn_pos.theta - origin.theta) / config.chunk_angular_size() * cs).floor() as u32;
        let lz = ((spawn_pos.y - origin.y) / config.chunk_width_size() * cs).floor() as u32;
        for y in 0..=top_local_y.min(config.chunk_size - 1) {
            chunk.set_voxel(lx, y, lz, Voxel::new(VoxelType::Stone));
        }
        chunk.generated = true;
        manager.chunks.insert(coord, chunk);
        (manager, config)
    }

    #[test]
    fn safe_spawn_places_feet_above_surface() {
        // Solid up to local y=4 (world height 0..=4); top surface is at h=5.
        let (manager, config) = manager_with_floor(4);
        let feet = find_safe_spawn_height(0.0, 0.0, &manager, &config);
        // Feet should be on top of the highest solid block (height 5).
        assert!(feet >= 5.0, "feet {} should be >= 5 (on top of surface)", feet);
        // And not buried: the voxel just below feet is solid, at feet is air.
        let below = RingPosition::new(0.0, 0.0, feet - 0.5);
        let at = RingPosition::new(0.0, 0.0, feet + 0.1);
        assert!(is_position_solid(&below, &config, &manager));
        assert!(!is_position_solid(&at, &config, &manager));
    }

    #[test]
    fn safe_spawn_aabb_is_clear() {
        let (manager, config) = manager_with_floor(6);
        let feet = find_safe_spawn_height(0.0, 0.0, &manager, &config);
        // Player AABB (feet .. feet + PLAYER_HEIGHT) must be free of solids.
        for &sh in &[feet + 0.1, feet + 1.0, feet + PLAYER_HEIGHT - 0.1] {
            let p = RingPosition::new(0.0, 0.0, sh);
            assert!(
                !is_position_solid(&p, &config, &manager),
                "player body at height {} should be clear",
                sh
            );
        }
    }

    #[test]
    fn safe_spawn_no_ground_returns_high_point() {
        // Empty world: no solid ground in the column.
        let config = RingWorldConfig::default();
        let manager = ChunkManager::new(config.clone(), 2);
        let feet = find_safe_spawn_height(0.0, 0.0, &manager, &config);
        assert!(feet >= 1.0 && feet <= config.max_height);
    }
}
