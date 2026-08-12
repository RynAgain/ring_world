/// Entity & Mob system for the ring world (v0.3)
/// Implements a simple ECS with mob types, AI, spawning, health, and combat.

use rand::Rng;
use crate::chunk::ChunkManager;
use crate::inventory::Inventory;
use crate::lighting;
use crate::ring_world::{ChunkCoord, RingPosition, RingWorldConfig};
use crate::voxel::VoxelType;

/// Types of mobs that can exist on the ring. Three families:
/// - Native FAUNA (passive): Grazer, Skitterling, Floater
/// - Ring NATIVES (passive humanoids): Ringkin, who build the villages
/// - Threats (hostile): Sentinel machines guarding ring facilities, and
///   Stalkers, nocturnal native predators
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MobType {
    /// Six-legged tan herbivore; placid, common on grass in daylight.
    Grazer,
    /// Small skittish scuttler; fast, harmless.
    Skitterling,
    /// Bioluminescent gasbag that hovers ~2 blocks up, trailing tendrils.
    Floater,
    /// The ring's natives: robed teal-skinned humanoids living in villages.
    Ringkin,
    /// Ancient security mech: tall gunmetal frame, glowing red eye-bar.
    /// Guards facilities (ruins, the sun tower) day AND night; rare night
    /// patrols elsewhere.
    Sentinel,
    /// Low six-legged nocturnal predator.
    Stalker,
}

impl MobType {
    /// Hovering mobs skip normal gravity and float above the ground.
    pub fn hovers(&self) -> bool {
        matches!(self, MobType::Floater)
    }
}

/// Behavior category for a mob
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MobBehavior {
    Passive,
    Hostile,
}

/// AI state for controlling mob behavior transitions
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiState {
    Idle,
    Wandering,
    Fleeing,
    Chasing,
    Attacking,
}

/// A single entity (mob) in the world
pub struct Entity {
    pub id: u64,
    pub mob_type: MobType,
    pub position: RingPosition,
    pub velocity: [f64; 3],
    pub health: f32,
    pub max_health: f32,
    pub damage: f32,
    pub attack_cooldown: f32,
    pub attack_timer: f32,
    pub behavior: MobBehavior,
    pub ai_state: AiState,
    pub target_position: Option<RingPosition>,
    pub wander_timer: f32,
    pub flee_timer: f32,
    pub aggro_range: f32,
    pub speed: f64,
    pub is_grounded: bool,
    pub alive: bool,
    pub vertical_velocity: f64,
    /// Yaw the mob is facing, in the ring-surface plane: 0 = +tangent
    /// (increasing theta), pi/2 = +axial (increasing y). Set from movement.
    pub facing: f64,
    /// Walk-cycle phase in radians, advanced by distance walked; drives the
    /// limb swing in build_entity_mesh.
    pub walk_phase: f64,
    /// Seconds remaining of the red damage flash (set on hit, decays).
    pub hurt_timer: f32,
}

impl Entity {
    pub fn new(id: u64, mob_type: MobType, position: RingPosition) -> Self {
        let (health, damage, speed, aggro_range, behavior) = mob_properties(mob_type);
        Self {
            id,
            mob_type,
            position,
            velocity: [0.0, 0.0, 0.0],
            health,
            max_health: health,
            damage,
            attack_cooldown: 1.0,
            attack_timer: 0.0,
            behavior,
            ai_state: AiState::Idle,
            target_position: None,
            wander_timer: 0.0,
            flee_timer: 0.0,
            aggro_range,
            speed,
            is_grounded: false,
            alive: true,
            vertical_velocity: 0.0,
            facing: 0.0,
            walk_phase: 0.0,
            hurt_timer: 0.0,
        }
    }

    pub fn drop_item(&self) -> VoxelType {
        match self.mob_type {
            MobType::Grazer => VoxelType::Leaves,
            MobType::Skitterling => VoxelType::Flower,
            MobType::Floater => VoxelType::Torch, // bioluminescent core
            MobType::Ringkin => VoxelType::Plank,
            MobType::Sentinel => VoxelType::IronIngot, // salvage
            MobType::Stalker => VoxelType::Vine, // sinew
        }
    }

    pub fn render_color(&self) -> [f32; 4] {
        match self.mob_type {
            MobType::Grazer => [0.62, 0.55, 0.40, 1.0],      // dusty tan
            MobType::Skitterling => [0.55, 0.72, 0.55, 1.0], // pale green
            MobType::Floater => [0.55, 0.82, 0.92, 1.0],     // glowing cyan
            MobType::Ringkin => [0.45, 0.62, 0.58, 1.0],     // teal skin
            MobType::Sentinel => [0.60, 0.63, 0.68, 1.0],    // gunmetal
            MobType::Stalker => [0.28, 0.22, 0.34, 1.0],     // dark violet
        }
    }

    pub fn mob_width(&self) -> f64 {
        match self.mob_type {
            MobType::Grazer => 1.1,
            MobType::Skitterling => 0.4,
            MobType::Floater => 0.8,
            MobType::Ringkin => 0.6,
            MobType::Sentinel => 0.9,
            MobType::Stalker => 1.1,
        }
    }

    pub fn mob_height(&self) -> f64 {
        match self.mob_type {
            MobType::Grazer => 1.2,
            MobType::Skitterling => 0.5,
            MobType::Floater => 0.9,
            MobType::Ringkin => 2.1,
            MobType::Sentinel => 2.6,
            MobType::Stalker => 0.9,
        }
    }
}

fn mob_properties(mob_type: MobType) -> (f32, f32, f64, f32, MobBehavior) {
    match mob_type {
        MobType::Grazer => (12.0, 0.0, 1.2, 0.0, MobBehavior::Passive),
        MobType::Skitterling => (4.0, 0.0, 2.5, 0.0, MobBehavior::Passive),
        MobType::Floater => (6.0, 0.0, 0.8, 0.0, MobBehavior::Passive),
        MobType::Ringkin => (16.0, 0.0, 1.6, 0.0, MobBehavior::Passive),
        MobType::Sentinel => (30.0, 5.0, 1.6, 20.0, MobBehavior::Hostile),
        MobType::Stalker => (16.0, 3.0, 3.0, 14.0, MobBehavior::Hostile),
    }
}

/// Manages all entities in the world
pub struct EntityManager {
    pub entities: Vec<Entity>,
    next_id: u64,
    spawn_timer: f32,
    pub max_entities: usize,
}

impl EntityManager {
    pub fn new() -> Self {
        Self {
            entities: Vec::new(),
            next_id: 1,
            spawn_timer: 0.0,
            max_entities: 50,
        }
    }

    pub fn spawn_entity(&mut self, mob_type: MobType, position: RingPosition) -> u64 {
        if self.entities.len() >= self.max_entities {
            return 0;
        }
        let id = self.next_id;
        self.next_id += 1;
        let entity = Entity::new(id, mob_type, position);
        self.entities.push(entity);
        id
    }

    pub fn update(
        &mut self,
        dt: f32,
        player_position: &RingPosition,
        player_health: &mut f32,
        chunk_manager: &ChunkManager,
        config: &RingWorldConfig,
        inventory: &mut Inventory,
        daylight: f32,
        terrain: &crate::terrain::TerrainGenerator,
    ) {
        self.spawn_timer += dt;
        if self.spawn_timer >= 5.0 {
            self.spawn_timer = 0.0;
            self.try_spawn(player_position, chunk_manager, config, daylight, terrain);
        }

        let player_pos = *player_position;
        for entity in self.entities.iter_mut() {
            if !entity.alive {
                continue;
            }

            if entity.attack_timer > 0.0 {
                entity.attack_timer -= dt;
            }

            if entity.hurt_timer > 0.0 {
                entity.hurt_timer -= dt;
            }

            if entity.flee_timer > 0.0 {
                entity.flee_timer -= dt;
                if entity.flee_timer <= 0.0 && entity.ai_state == AiState::Fleeing {
                    entity.ai_state = AiState::Idle;
                }
            }

            let dist = ring_distance(&entity.position, &player_pos, config);

            match entity.behavior {
                MobBehavior::Passive => {
                    update_passive_ai(entity, dt, config);
                }
                MobBehavior::Hostile => {
                    update_hostile_ai(entity, dt, dist, &player_pos, player_health, config);
                }
            }

            apply_movement(entity, dt, config, chunk_manager);
            apply_gravity(entity, dt, config, chunk_manager);
        }

        // Collect drops from dead entities
        for entity in self.entities.iter() {
            if !entity.alive && entity.health <= 0.0 {
                let drop = entity.drop_item();
                inventory.add_item(drop, 1);
            }
        }

        self.remove_dead();
        self.despawn_far_entities(player_position, 60.0, config);
    }

    fn try_spawn(
        &mut self,
        player_position: &RingPosition,
        chunk_manager: &ChunkManager,
        config: &RingWorldConfig,
        daylight: f32,
        terrain: &crate::terrain::TerrainGenerator,
    ) {
        if self.entities.len() >= self.max_entities {
            return;
        }

        let mut rng = rand::thread_rng();
        let spawn_attempts = rng.gen_range(1..=3u32);

        for _ in 0..spawn_attempts {
            if self.entities.len() >= self.max_entities {
                break;
            }

            let distance: f64 = rng.gen_range(20.0..40.0);
            let angle_offset: f64 = rng.gen_range(-std::f64::consts::PI..std::f64::consts::PI);

            let spawn_theta = player_position.theta + distance * angle_offset.cos() / config.radius;
            let spawn_y = player_position.y + distance * angle_offset.sin();

            let spawn_height = find_ground_height(spawn_theta, spawn_y, config, chunk_manager);

            if let Some(ground_height) = spawn_height {
                let spawn_pos = RingPosition::new(spawn_theta, spawn_y, ground_height + 1.0);

                if !spawn_pos.is_valid(config) {
                    continue;
                }
                if is_position_solid_entity(&spawn_pos, config, chunk_manager) {
                    continue;
                }
                if is_position_water_entity(&spawn_pos, config, chunk_manager) {
                    continue;
                }

                // Check light level at spawn position to determine mob type
                let light_level = lighting::get_light_level_at(&spawn_pos, chunk_manager, config);

                // Spawn ecology (deterministic structure queries):
                // - Sentinels guard FACILITIES (ruins, sun tower) day and
                //   night, and run rare night patrols elsewhere.
                // - Ringkin natives live around the villages they build,
                //   in daylight.
                // - Stalkers hunt in the dark (unlit caves or shadow-square
                //   night); native fauna needs a lit daytime surface.
                let cs = config.chunk_size as f64;
                let world_x = (spawn_theta.rem_euclid(std::f64::consts::TAU)
                    / config.chunk_angular_size()
                    * cs) as i32;
                let world_z = ((spawn_y + config.width / 2.0)
                    / config.chunk_width_size()
                    * cs) as i32;
                let near_facility = terrain
                    .facility_center_near(world_x, world_z, config, 28)
                    .is_some();
                let near_village = terrain
                    .village_center_near(world_x, world_z, config, 48)
                    .is_some();
                let dark = light_level < 7 || daylight < 0.3;

                let mob_type = if near_facility {
                    MobType::Sentinel
                } else if dark {
                    if rng.gen_range(0..4u32) == 0 {
                        MobType::Sentinel // night patrol far from its post
                    } else {
                        MobType::Stalker
                    }
                } else if near_village && rng.gen_range(0..5u32) < 3 {
                    MobType::Ringkin
                } else {
                    match rng.gen_range(0..3u32) {
                        0 => MobType::Grazer,
                        1 => MobType::Skitterling,
                        _ => MobType::Floater,
                    }
                };

                self.spawn_entity(mob_type, spawn_pos);
            }
        }
    }

    fn despawn_far_entities(
        &mut self,
        player_position: &RingPosition,
        max_distance: f32,
        config: &RingWorldConfig,
    ) {
        self.entities.retain(|entity| {
            let dist = ring_distance(&entity.position, player_position, config);
            dist < max_distance
        });
    }

    pub fn get_entities_near(&self, position: &RingPosition, radius: f32, config: &RingWorldConfig) -> Vec<&Entity> {
        self.entities
            .iter()
            .filter(|e| e.alive && ring_distance(&e.position, position, config) < radius)
            .collect()
    }

    pub fn damage_entity(&mut self, id: u64, amount: f32, knockback_from: &RingPosition, config: &RingWorldConfig) -> bool {
        if let Some(entity) = self.entities.iter_mut().find(|e| e.id == id && e.alive) {
            entity.health -= amount;
            entity.hurt_timer = HURT_FLASH_SECS;
            if entity.health <= 0.0 {
                entity.alive = false;
            } else {
                let d_theta = entity.position.theta - knockback_from.theta;
                let d_y = entity.position.y - knockback_from.y;
                let dist = (d_theta * config.radius * d_theta * config.radius + d_y * d_y).sqrt();
                if dist > 0.01 {
                    let knockback_strength = 5.0;
                    entity.velocity[0] = (d_theta * config.radius / dist) * knockback_strength / config.radius;
                    entity.velocity[1] = (d_y / dist) * knockback_strength;
                    entity.vertical_velocity = 3.0;
                    entity.is_grounded = false;
                }

                if entity.behavior == MobBehavior::Passive {
                    entity.ai_state = AiState::Fleeing;
                    entity.flee_timer = 3.0;
                    let flee_theta = entity.position.theta + d_theta * 5.0;
                    let flee_y = entity.position.y + d_y * 5.0;
                    entity.target_position = Some(RingPosition::new(
                        flee_theta,
                        flee_y,
                        entity.position.height,
                    ));
                }
            }
            true
        } else {
            false
        }
    }

    fn remove_dead(&mut self) {
        self.entities.retain(|e| e.alive);
    }

    pub fn raycast_hit_entity(
        &self,
        camera_pos: &cgmath::Point3<f32>,
        look_dir: &cgmath::Vector3<f32>,
        max_distance: f32,
        config: &RingWorldConfig,
    ) -> Option<(u64, f32)> {
        let mut closest_hit: Option<(u64, f32)> = None;

        for entity in &self.entities {
            if !entity.alive {
                continue;
            }

            let entity_cart = entity.position.to_cartesian(config);
            let ex = entity_cart.x as f32;
            let ey = entity_cart.y as f32;
            let ez = entity_cart.z as f32;

            let to_entity_x = ex - camera_pos.x;
            let to_entity_y = ey - camera_pos.y;
            let to_entity_z = ez - camera_pos.z;

            let dot = to_entity_x * look_dir.x + to_entity_y * look_dir.y + to_entity_z * look_dir.z;

            if dot < 0.0 || dot > max_distance {
                continue;
            }

            let proj_x = look_dir.x * dot;
            let proj_y = look_dir.y * dot;
            let proj_z = look_dir.z * dot;

            let perp_x = to_entity_x - proj_x;
            let perp_y = to_entity_y - proj_y;
            let perp_z = to_entity_z - proj_z;
            let perp_dist = (perp_x * perp_x + perp_y * perp_y + perp_z * perp_z).sqrt();

            let hit_radius = 1.5f32;

            if perp_dist < hit_radius {
                match closest_hit {
                    None => closest_hit = Some((entity.id, dot)),
                    Some((_, prev_dist)) if dot < prev_dist => {
                        closest_hit = Some((entity.id, dot));
                    }
                    _ => {}
                }
            }
        }

        closest_hit
    }

    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }
}

fn update_passive_ai(entity: &mut Entity, dt: f32, config: &RingWorldConfig) {
    match entity.ai_state {
        AiState::Idle => {
            entity.wander_timer -= dt;
            if entity.wander_timer <= 0.0 {
                let mut rng = rand::thread_rng();
                let wander_dist: f64 = rng.gen_range(2.0..5.0);
                let wander_angle: f64 = rng.gen_range(0.0..std::f64::consts::TAU);

                let target_theta = entity.position.theta + wander_dist * wander_angle.cos() / config.radius;
                let target_y = entity.position.y + wander_dist * wander_angle.sin();

                entity.target_position = Some(RingPosition::new(
                    target_theta,
                    target_y,
                    entity.position.height,
                ));
                entity.ai_state = AiState::Wandering;
                entity.wander_timer = rng.gen_range(3.0..8.0);
            }
        }
        AiState::Wandering => {
            if let Some(target) = entity.target_position {
                let d_theta = (target.theta - entity.position.theta) * config.radius;
                let d_y = target.y - entity.position.y;
                let dist = (d_theta * d_theta + d_y * d_y).sqrt();

                if dist < 0.5 {
                    entity.ai_state = AiState::Idle;
                    entity.target_position = None;
                    let mut rng = rand::thread_rng();
                    entity.wander_timer = rng.gen_range(3.0..8.0);
                }
            } else {
                entity.ai_state = AiState::Idle;
            }
        }
        AiState::Fleeing => {
            if entity.target_position.is_none() {
                entity.ai_state = AiState::Idle;
            }
        }
        _ => {
            entity.ai_state = AiState::Idle;
        }
    }
}

fn update_hostile_ai(
    entity: &mut Entity,
    dt: f32,
    dist_to_player: f32,
    player_pos: &RingPosition,
    player_health: &mut f32,
    config: &RingWorldConfig,
) {
    if dist_to_player <= entity.aggro_range {
        if dist_to_player <= 2.0 {
            entity.ai_state = AiState::Attacking;
            entity.target_position = Some(*player_pos);

            if entity.attack_timer <= 0.0 && entity.damage > 0.0 {
                *player_health -= entity.damage;
                entity.attack_timer = entity.attack_cooldown;
                if *player_health < 0.0 {
                    *player_health = 0.0;
                }
            }
        } else {
            entity.ai_state = AiState::Chasing;
            entity.target_position = Some(*player_pos);
        }
    } else {
        match entity.ai_state {
            AiState::Chasing | AiState::Attacking => {
                entity.ai_state = AiState::Idle;
                entity.target_position = None;
                let mut rng = rand::thread_rng();
                entity.wander_timer = rng.gen_range(3.0..8.0);
            }
            AiState::Idle => {
                entity.wander_timer -= dt;
                if entity.wander_timer <= 0.0 {
                    let mut rng = rand::thread_rng();
                    let wander_dist: f64 = rng.gen_range(2.0..5.0);
                    let wander_angle: f64 = rng.gen_range(0.0..std::f64::consts::TAU);

                    let target_theta = entity.position.theta + wander_dist * wander_angle.cos() / config.radius;
                    let target_y = entity.position.y + wander_dist * wander_angle.sin();

                    entity.target_position = Some(RingPosition::new(
                        target_theta,
                        target_y,
                        entity.position.height,
                    ));
                    entity.ai_state = AiState::Wandering;
                    entity.wander_timer = rng.gen_range(3.0..8.0);
                }
            }
            AiState::Wandering => {
                if let Some(target) = entity.target_position {
                    let d_theta = (target.theta - entity.position.theta) * config.radius;
                    let d_y = target.y - entity.position.y;
                    let dist = (d_theta * d_theta + d_y * d_y).sqrt();

                    if dist < 0.5 {
                        entity.ai_state = AiState::Idle;
                        entity.target_position = None;
                        let mut rng = rand::thread_rng();
                        entity.wander_timer = rng.gen_range(3.0..8.0);
                    }
                } else {
                    entity.ai_state = AiState::Idle;
                }
            }
            _ => {}
        }
    }
}

/// Duration of the red damage flash on a hit mob, in seconds.
const HURT_FLASH_SECS: f32 = 0.35;
/// How far above the ground a Floater's feet hover, in blocks.
const FLOATER_HOVER_HEIGHT: f64 = 2.0;

/// Speed of the hop a mob uses to climb a 1-block step
/// (apex = v^2 / (2 g) = 6.8^2 / 40 = ~1.16 blocks).
const STEP_JUMP_SPEED: f64 = 6.8;
/// Walk-cycle phase advance per block walked (one stride = ~0.9 blocks).
const WALK_PHASE_PER_BLOCK: f64 = 7.0;

/// Whether the mob's body fits standing at (theta, y) with its CENTER at
/// `height`: samples near the feet and near the head, so collision happens at
/// leg level (mobs stop clipping into 1-block rises) while still refusing to
/// walk under overhangs they don't fit beneath.
fn can_stand_at(
    entity: &Entity,
    theta: f64,
    y: f64,
    height: f64,
    config: &RingWorldConfig,
    chunk_manager: &ChunkManager,
) -> bool {
    let feet = height - entity.mob_height() * 0.5;
    let low = RingPosition::new(theta, y, feet + 0.1);
    let high = RingPosition::new(theta, y, feet + entity.mob_height() - 0.1);
    !is_position_solid_entity(&low, config, chunk_manager)
        && !is_position_solid_entity(&high, config, chunk_manager)
}

fn apply_movement(
    entity: &mut Entity,
    dt: f32,
    config: &RingWorldConfig,
    chunk_manager: &ChunkManager,
) {
    let dt_f64 = dt as f64;

    if entity.velocity[0].abs() > 0.01 || entity.velocity[1].abs() > 0.01 {
        entity.position.theta += entity.velocity[0] * dt_f64;
        entity.position.y += entity.velocity[1] * dt_f64;

        let damping = 0.9_f64.powf(dt_f64 * 60.0);
        entity.velocity[0] *= damping;
        entity.velocity[1] *= damping;
    }

    if let Some(target) = entity.target_position {
        let d_theta = (target.theta - entity.position.theta) * config.radius;
        let d_y = target.y - entity.position.y;
        let dist = (d_theta * d_theta + d_y * d_y).sqrt();

        if dist > 0.1 {
            // Face the walk direction (yaw 0 = +tangent, pi/2 = +axial).
            entity.facing = d_y.atan2(d_theta);

            let move_speed = entity.speed * dt_f64;
            let move_frac = (move_speed / dist).min(1.0);

            let new_theta = entity.position.theta + (d_theta / config.radius) * move_frac;
            let new_y = entity.position.y + d_y * move_frac;
            let h = entity.position.height;

            if can_stand_at(entity, new_theta, new_y, h, config, chunk_manager) {
                entity.position.theta = new_theta;
                entity.position.y = new_y;
                entity.walk_phase += move_frac * dist * WALK_PHASE_PER_BLOCK;
            } else if entity.is_grounded
                && can_stand_at(entity, new_theta, new_y, h + 1.0, config, chunk_manager)
            {
                // A 1-block step ahead: hop it with a real jump impulse. The
                // old code moved at constant height, clipped its feet into the
                // step, and let the ground snap teleport the mob up a block,
                // which read as constant twitchy "jumping" on any slope.
                entity.vertical_velocity = STEP_JUMP_SPEED;
                entity.is_grounded = false;
            } else {
                // Blocked straight ahead: try sliding along one axis.
                if can_stand_at(entity, new_theta, entity.position.y, h, config, chunk_manager) {
                    entity.position.theta = new_theta;
                    entity.walk_phase += move_frac * d_theta.abs() * WALK_PHASE_PER_BLOCK;
                } else if can_stand_at(entity, entity.position.theta, new_y, h, config, chunk_manager) {
                    entity.position.y = new_y;
                    entity.walk_phase += move_frac * d_y.abs() * WALK_PHASE_PER_BLOCK;
                }
            }
        }
    }

    entity.position.normalize_theta();
}
fn apply_gravity(
    entity: &mut Entity,
    dt: f32,
    config: &RingWorldConfig,
    chunk_manager: &ChunkManager,
) {
    let dt_f64 = dt as f64;
    let gravity = 20.0;

    // Hovering mobs (Floaters) ignore gravity: ease toward a point ~2
    // blocks above the ground with a gentle per-individual bob. They count
    // as grounded so the movement code treats them as supported.
    if entity.mob_type.hovers() {
        if let Some(ground) = find_ground_height(
            entity.position.theta,
            entity.position.y,
            config,
            chunk_manager,
        ) {
            let bob = ((entity.walk_phase * 0.5) + entity.id as f64 * 1.7).sin() * 0.25;
            let target = ground + FLOATER_HOVER_HEIGHT + bob + entity.mob_height() * 0.5;
            let dh = target - entity.position.height;
            entity.position.height += dh * (dt_f64 * 2.0).min(1.0);
            entity.vertical_velocity = 0.0;
            entity.is_grounded = true;
            entity.position.height = entity.position.height.clamp(1.0, config.max_height - 1.0);
            return;
        }
    }

    if !entity.is_grounded {
        entity.vertical_velocity -= gravity * dt_f64;
        if entity.vertical_velocity < -50.0 {
            entity.vertical_velocity = -50.0;
        }
    }

    let new_height = entity.position.height + entity.vertical_velocity * dt_f64;

    let feet_height = new_height - entity.mob_height() * 0.5;
    let ground_check = RingPosition::new(
        entity.position.theta,
        entity.position.y,
        feet_height - 0.05,
    );

    if entity.vertical_velocity <= 0.0 && is_position_solid_entity(&ground_check, config, chunk_manager) {
        // Snap to the top of the block the ground probe actually HIT (at
        // feet - 0.05), not floor(feet)+1: when standing, feet sit exactly
        // on an integer (12.0) and floor(12.0)+1 = 13 teleported the mob up
        // a block every other frame — the "mobs oscillate up and down" bug.
        let block_top = (feet_height - 0.05).floor() + 1.0;
        entity.position.height = block_top + entity.mob_height() * 0.5;
        entity.vertical_velocity = 0.0;
        entity.is_grounded = true;
    } else {
        entity.position.height = new_height;
        entity.is_grounded = false;
    }

    entity.position.height = entity.position.height.clamp(1.0, config.max_height - 1.0);

    if entity.position.height <= 1.0 {
        entity.alive = false;
    }
}

/// One box of a composite mob model. Offsets and half-extents are in blocks
/// in the mob's local frame (forward, side, up), measured from the ground
/// point under the mob's center (the "feet origin").
struct MobPart {
    offset: [f64; 3],
    half: [f64; 3],
    color_mul: [f32; 3],
    /// Fore/aft sway amplitude sign for the walk animation; diagonal legs
    /// (and humanoid arms vs legs) carry opposite signs so they swing in
    /// opposition. 0.0 = rigid part.
    swing: f64,
}

fn part(offset: [f64; 3], half: [f64; 3], color_mul: [f32; 3], swing: f64) -> MobPart {
    MobPart { offset, half, color_mul, swing }
}

/// Composite box model per mob type (body + head + limbs in the mob's
/// local fwd/side/up frame, offsets from the feet origin).
fn mob_parts(mob: MobType) -> Vec<MobPart> {
    let limb = [0.55, 0.55, 0.55];
    let head = [0.85, 0.85, 0.85];
    let body = [1.0, 1.0, 1.0];
    match mob {
        // Six-legged herbivore: long low body, drooped grazing head.
        MobType::Grazer => vec![
            part([0.0, 0.0, 0.66], [0.55, 0.30, 0.26], body, 0.0),
            part([0.64, 0.0, 0.48], [0.14, 0.12, 0.14], head, 0.0),
            part([0.38, 0.26, 0.20], [0.07, 0.07, 0.20], limb, 1.0),
            part([0.38, -0.26, 0.20], [0.07, 0.07, 0.20], limb, -1.0),
            part([0.0, 0.26, 0.20], [0.07, 0.07, 0.20], limb, -1.0),
            part([0.0, -0.26, 0.20], [0.07, 0.07, 0.20], limb, 1.0),
            part([-0.38, 0.26, 0.20], [0.07, 0.07, 0.20], limb, 1.0),
            part([-0.38, -0.26, 0.20], [0.07, 0.07, 0.20], limb, -1.0),
        ],
        // Tiny scuttler: low body, oversized sensor head, 4 pin legs.
        MobType::Skitterling => vec![
            part([0.0, 0.0, 0.24], [0.14, 0.10, 0.09], body, 0.0),
            part([0.16, 0.0, 0.36], [0.08, 0.07, 0.08], head, 0.0),
            part([0.08, 0.09, 0.08], [0.02, 0.02, 0.08], limb, 1.0),
            part([0.08, -0.09, 0.08], [0.02, 0.02, 0.08], limb, -1.0),
            part([-0.08, 0.09, 0.08], [0.02, 0.02, 0.08], limb, -1.0),
            part([-0.08, -0.09, 0.08], [0.02, 0.02, 0.08], limb, 1.0),
        ],
        // Hovering gasbag: glowing bell + three trailing tendrils. The
        // hover physics keeps its feet ~2 blocks off the ground.
        MobType::Floater => vec![
            part([0.0, 0.0, 0.62], [0.28, 0.28, 0.24], body, 0.0),
            part([0.0, 0.0, 0.86], [0.14, 0.14, 0.06], [1.2, 1.2, 1.2], 0.0),
            part([0.0, 0.18, 0.18], [0.03, 0.03, 0.20], [0.7, 0.9, 1.0], 1.0),
            part([0.0, -0.18, 0.18], [0.03, 0.03, 0.20], [0.7, 0.9, 1.0], -1.0),
            part([0.12, 0.0, 0.18], [0.03, 0.03, 0.20], [0.7, 0.9, 1.0], 1.0),
        ],
        // Robed native: ochre robe (rigid), teal skin, swinging arms.
        MobType::Ringkin => vec![
            part([0.0, 0.0, 0.50], [0.17, 0.22, 0.50], [0.85, 0.62, 0.30], 0.0),
            part([0.0, 0.0, 1.32], [0.14, 0.22, 0.32], [0.75, 0.55, 0.28], 0.0),
            part([0.0, 0.0, 1.86], [0.14, 0.14, 0.18], body, 0.0),
            part([0.0, 0.32, 1.34], [0.06, 0.06, 0.30], body, -1.0),
            part([0.0, -0.32, 1.34], [0.06, 0.06, 0.30], body, 1.0),
        ],
        // Security mech: thick strider legs, armored chest, red eye-bar.
        MobType::Sentinel => vec![
            part([0.0, 0.18, 0.55], [0.10, 0.10, 0.55], [0.45, 0.45, 0.50], 1.0),
            part([0.0, -0.18, 0.55], [0.10, 0.10, 0.55], [0.45, 0.45, 0.50], -1.0),
            part([0.0, 0.0, 1.22], [0.16, 0.24, 0.12], [0.50, 0.52, 0.56], 0.0),
            part([0.0, 0.0, 1.72], [0.22, 0.34, 0.36], body, 0.0),
            part([0.0, 0.42, 2.00], [0.08, 0.10, 0.10], [0.50, 0.52, 0.56], 0.0),
            part([0.0, -0.42, 2.00], [0.08, 0.10, 0.10], [0.50, 0.52, 0.56], 0.0),
            part([0.0, 0.0, 2.32], [0.10, 0.26, 0.10], [1.5, 0.15, 0.15], 0.0),
        ],
        // Nocturnal predator: long low body, jawed head, six splayed legs.
        MobType::Stalker => vec![
            part([-0.05, 0.0, 0.50], [0.45, 0.24, 0.18], body, 0.0),
            part([0.52, 0.0, 0.44], [0.16, 0.13, 0.12], [0.7, 0.6, 0.75], 0.0),
            part([0.30, 0.38, 0.30], [0.05, 0.20, 0.04], limb, 1.0),
            part([0.30, -0.38, 0.30], [0.05, 0.20, 0.04], limb, -1.0),
            part([0.0, 0.40, 0.30], [0.05, 0.22, 0.04], limb, -1.0),
            part([0.0, -0.40, 0.30], [0.05, 0.22, 0.04], limb, 1.0),
            part([-0.30, 0.38, 0.30], [0.05, 0.20, 0.04], limb, 1.0),
            part([-0.30, -0.38, 0.30], [0.05, 0.20, 0.04], limb, -1.0),
        ],
    }
}

/// The player's own composite model (Steve-style humanoid, 2 blocks tall):
/// skin head, cyan shirt torso + arms, indigo legs. Same part conventions as
/// mob_parts (offsets from the feet origin in fwd/side/up blocks).
fn player_parts() -> Vec<MobPart> {
    let skin = [0.87, 0.68, 0.53];
    let shirt = [0.20, 0.65, 0.65];
    let pants = [0.25, 0.28, 0.55];
    vec![
        part([0.0, 0.0, 1.16], [0.14, 0.24, 0.36], shirt, 0.0),
        part([0.0, 0.0, 1.76], [0.16, 0.16, 0.20], skin, 0.0),
        part([0.0, 0.33, 1.22], [0.08, 0.08, 0.32], shirt, -1.0),
        part([0.0, -0.33, 1.22], [0.08, 0.08, 0.32], shirt, 1.0),
        part([0.0, 0.12, 0.40], [0.09, 0.10, 0.40], pants, 1.0),
        part([0.0, -0.12, 0.40], [0.09, 0.10, 0.40], pants, -1.0),
    ]
}

/// Append one composite model's boxes to a vertex/index list. `feet` is the
/// world-space ground point under the model's center; (fwd, side, up) is the
/// model's orthonormal frame (a yaw rotation of the ring frame, so the face
/// table's CCW-outward winding invariant holds); walk_phase drives limb sway.
fn emit_parts(
    parts: &[MobPart],
    base_color: [f32; 4],
    feet: [f32; 3],
    fwd: [f32; 3],
    side: [f32; 3],
    up: [f32; 3],
    walk_phase: f64,
    vertices: &mut Vec<crate::chunk::ChunkVertex>,
    indices: &mut Vec<u32>,
) {
    use crate::texture::TEX_SNOW;

    let scale3 = |v: [f32; 3], s: f32| [v[0] * s, v[1] * s, v[2] * s];
    let add3 = |a: [f32; 3], b: [f32; 3]| [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
    let neg3 = |v: [f32; 3]| [-v[0], -v[1], -v[2]];

    let swing_off = walk_phase.sin() * 0.12;

    for p in parts {
        let off_f = (p.offset[0] + p.swing * swing_off) as f32;
        let off_s = p.offset[1] as f32;
        let off_u = p.offset[2] as f32;
        let c = add3(
            add3(feet, scale3(fwd, off_f)),
            add3(scale3(side, off_s), scale3(up, off_u)),
        );
        let hf = p.half[0] as f32;
        let hs = p.half[1] as f32;
        let hu = p.half[2] as f32;
        let color = [
            base_color[0] * p.color_mul[0],
            base_color[1] * p.color_mul[1],
            base_color[2] * p.color_mul[2],
            base_color[3],
        ];

        // Each face: (outward normal, u basis, v basis, half-extents)
        // chosen so u x v points along the normal (CCW-outward winding
        // for the back-face-culling opaque pipeline).
        let faces: [([f32; 3], [f32; 3], [f32; 3], f32, f32, f32); 6] = [
            (fwd, side, up, hf, hs, hu),
            (neg3(fwd), up, side, hf, hu, hs),
            (up, fwd, side, hu, hf, hs),
            (neg3(up), side, fwd, hu, hs, hf),
            (side, up, fwd, hs, hu, hf),
            (neg3(side), fwd, up, hs, hf, hu),
        ];

        for (normal, u, v, n_half, u_half, v_half) in faces.iter() {
            let fc = add3(c, scale3(*normal, *n_half));
            let uu = scale3(*u, *u_half);
            let vv = scale3(*v, *v_half);
            let corners = [
                add3(add3(fc, neg3(uu)), neg3(vv)),
                add3(add3(fc, uu), neg3(vv)),
                add3(add3(fc, uu), vv),
                add3(add3(fc, neg3(uu)), vv),
            ];
            let uvs = [[0.0f32, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
            let base = vertices.len() as u32;
            for (i, corner) in corners.iter().enumerate() {
                vertices.push(crate::chunk::ChunkVertex {
                    position: *corner,
                    normal: *normal,
                    color,
                    tex_coords: uvs[i],
                    tex_index: TEX_SNOW,
                    light_level: 1.0,
                    alpha_tested: 0,
                });
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
}

/// Compute the ring-local frame (fwd, side, up) at a theta for a facing yaw,
/// plus the world-space feet point for a body whose CENTER is at `position`
/// with total height `body_height`.
fn model_frame(
    position: &RingPosition,
    body_height: f64,
    facing: f64,
    config: &RingWorldConfig,
) -> ([f32; 3], [f32; 3], [f32; 3], [f32; 3]) {
    let scale3 = |v: [f32; 3], s: f32| [v[0] * s, v[1] * s, v[2] * s];
    let add3 = |a: [f32; 3], b: [f32; 3]| [a[0] + b[0], a[1] + b[1], a[2] + b[2]];

    let theta = position.theta;
    let (sin_t, cos_t) = (theta.sin() as f32, theta.cos() as f32);
    let tangent = [-sin_t, 0.0, cos_t];
    let up = [-cos_t, 0.0, -sin_t]; // radial-in, toward the sun
    let axial = [0.0f32, 1.0, 0.0];

    let (sin_f, cos_f) = (facing.sin() as f32, facing.cos() as f32);
    let fwd = add3(scale3(tangent, cos_f), scale3(axial, sin_f));
    let side = add3(scale3(tangent, -sin_f), scale3(axial, cos_f));

    let feet_pos = RingPosition::new(
        position.theta,
        position.y,
        position.height - body_height * 0.5,
    );
    let feet_cart = feet_pos.to_cartesian(config);
    let feet = [feet_cart.x as f32, feet_cart.y as f32, feet_cart.z as f32];

    (fwd, side, up, feet)
}

/// Build the player's own body mesh (third-person view). `position.height`
/// is the body CENTER (same convention as entities); facing is the camera
/// yaw in the ring-surface plane (0 = +tangent). World-space vertices: draw
/// with the identity transform bind group, same pass as entities.
pub fn build_player_mesh(
    position: &RingPosition,
    body_height: f64,
    facing: f64,
    walk_phase: f64,
    config: &RingWorldConfig,
) -> (Vec<crate::chunk::ChunkVertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let (fwd, side, up, feet) = model_frame(position, body_height, facing, config);
    emit_parts(
        &player_parts(),
        [1.0, 1.0, 1.0, 1.0],
        feet,
        fwd,
        side,
        up,
        walk_phase,
        &mut vertices,
        &mut indices,
    );
    (vertices, indices)
}

/// Build a renderable world-space mesh for every living entity: a composite
/// Minecraft-style box model (body, head, limbs) per mob, tinted by
/// render_color, oriented in the ring's local frame at the entity's theta,
/// rotated to its facing yaw, limbs swinging with walk_phase. Vertices are in
/// WORLD space: draw with the identity transform bind group. The boxes go
/// through the normal chunk shader, so they receive sun diffuse, the
/// shadow-square eclipse, and fog for free.
pub fn build_entity_mesh(
    entities: &[Entity],
    config: &RingWorldConfig,
) -> (Vec<crate::chunk::ChunkVertex>, Vec<u32>) {
    let mut vertices: Vec<crate::chunk::ChunkVertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for entity in entities.iter().filter(|e| e.alive) {
        let (fwd, side, up, feet) = model_frame(
            &entity.position,
            entity.mob_height(),
            entity.facing,
            config,
        );
        // Red damage flash: blend the body color toward red while the hurt
        // timer runs, strongest at the moment of the hit.
        let mut color = entity.render_color();
        if entity.hurt_timer > 0.0 {
            let f = (entity.hurt_timer / HURT_FLASH_SECS).clamp(0.0, 1.0) * 0.8;
            color[0] = color[0] + (1.0 - color[0]) * f;
            color[1] *= 1.0 - f;
            color[2] *= 1.0 - f;
        }
        emit_parts(
            &mob_parts(entity.mob_type),
            color,
            feet,
            fwd,
            side,
            up,
            entity.walk_phase,
            &mut vertices,
            &mut indices,
        );
    }

    (vertices, indices)
}

/// Calculate approximate distance between two ring positions in blocks
pub fn ring_distance(a: &RingPosition, b: &RingPosition, config: &RingWorldConfig) -> f32 {
    let d_theta = (a.theta - b.theta) * config.radius;
    let d_y = a.y - b.y;
    let d_h = a.height - b.height;
    ((d_theta * d_theta + d_y * d_y + d_h * d_h).sqrt()) as f32
}

fn find_ground_height(
    theta: f64,
    y: f64,
    config: &RingWorldConfig,
    chunk_manager: &ChunkManager,
) -> Option<f64> {
    let max_h = config.max_height as i32;
    for h in (0..max_h).rev() {
        let check_pos = RingPosition::new(theta, y, h as f64 + 0.5);
        if is_position_solid_entity(&check_pos, config, chunk_manager) {
            return Some(h as f64 + 1.0);
        }
    }
    None
}

fn is_position_solid_entity(pos: &RingPosition, config: &RingWorldConfig, chunk_manager: &ChunkManager) -> bool {
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

fn is_position_water_entity(pos: &RingPosition, config: &RingWorldConfig, chunk_manager: &ChunkManager) -> bool {
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
    #[test]
    fn entity_mesh_boxes_wind_ccw_outward() {
        // Regression guard: entity boxes draw through the back-face-culling
        // opaque pipeline, so every triangle must wind CCW seen from outside
        // (geometric normal agrees with the declared outward normal) at
        // several ring angles.
        let config = crate::ring_world::RingWorldConfig::default();
        for theta in [0.0f64, 0.7, 1.6, 3.14, 4.5, 6.0] {
            let mut e = super::Entity::new(
                1,
                super::MobType::Grazer,
                crate::ring_world::RingPosition::new(theta, 3.0, 10.0),
            );
            e.facing = 0.9;
            e.walk_phase = 2.3;
            let n_parts = super::mob_parts(super::MobType::Grazer).len();
            let (verts, idx) = super::build_entity_mesh(&[e], &config);
            assert_eq!(verts.len(), 24 * n_parts);
            assert_eq!(idx.len(), 36 * n_parts);
            for tri in idx.chunks(3) {
                let a = &verts[tri[0] as usize];
                let b = &verts[tri[1] as usize];
                let c = &verts[tri[2] as usize];
                let e1 = [
                    b.position[0] - a.position[0],
                    b.position[1] - a.position[1],
                    b.position[2] - a.position[2],
                ];
                let e2 = [
                    c.position[0] - a.position[0],
                    c.position[1] - a.position[1],
                    c.position[2] - a.position[2],
                ];
                let geo = [
                    e1[1] * e2[2] - e1[2] * e2[1],
                    e1[2] * e2[0] - e1[0] * e2[2],
                    e1[0] * e2[1] - e1[1] * e2[0],
                ];
                let dot = geo[0] * a.normal[0] + geo[1] * a.normal[1] + geo[2] * a.normal[2];
                assert!(dot > 0.0, "entity face winds against normal at theta {}", theta);
            }
        }
    }

    #[test]
    fn player_mesh_winds_ccw_outward_and_fits_height() {
        // The player body goes through the same back-face-culled pipeline as
        // entities: every triangle must wind CCW seen from outside, at
        // several thetas and facings, and no part may poke below the feet or
        // above the 2-block body height.
        let config = crate::ring_world::RingWorldConfig::default();
        for (theta, facing) in [(0.0f64, 0.0f64), (1.3, 0.9), (3.9, -2.2), (5.7, 3.0)] {
            let pos = crate::ring_world::RingPosition::new(theta, 2.0, 12.0);
            let (verts, idx) = super::build_player_mesh(&pos, 2.0, facing, 1.7, &config);
            assert_eq!(verts.len(), 24 * 6, "6 player parts expected");
            for tri in idx.chunks(3) {
                let a = &verts[tri[0] as usize];
                let b = &verts[tri[1] as usize];
                let c = &verts[tri[2] as usize];
                let e1 = [
                    b.position[0] - a.position[0],
                    b.position[1] - a.position[1],
                    b.position[2] - a.position[2],
                ];
                let e2 = [
                    c.position[0] - a.position[0],
                    c.position[1] - a.position[1],
                    c.position[2] - a.position[2],
                ];
                let geo = [
                    e1[1] * e2[2] - e1[2] * e2[1],
                    e1[2] * e2[0] - e1[0] * e2[2],
                    e1[0] * e2[1] - e1[1] * e2[0],
                ];
                let dot = geo[0] * a.normal[0] + geo[1] * a.normal[1] + geo[2] * a.normal[2];
                assert!(dot > 0.0, "player face winds against normal at theta {}", theta);
            }
        }
        // Static extents (walk_phase 0): parts stay within [feet, feet+2.0]
        // along up. Check in part space directly.
        for p in super::player_parts() {
            assert!(p.offset[2] - p.half[2] >= -1e-6);
            assert!(p.offset[2] + p.half[2] <= 2.0 + 1e-6);
        }
    }

    #[test]
    fn mobs_hop_up_one_block_steps_instead_of_clipping() {
        // A floor at height 10 with a 1-block step at height 11 halfway along
        // the walk path. The mob must end standing ON the step top (feet ~12)
        // having crossed it via the step-up jump, never clipping inside it.
        use crate::chunk::{Chunk, ChunkManager};
        use crate::voxel::{Voxel, VoxelType};

        let config = crate::ring_world::RingWorldConfig::default();
        let size = config.chunk_size;
        let coord = crate::ring_world::ChunkCoord::new(0, 0, 0);
        let mut chunk = Chunk::new(coord, size);
        for lx in 0..size {
            for lz in 0..size {
                chunk.set_voxel(lx, 10, lz, Voxel::new(VoxelType::Stone));
                if lx >= 8 {
                    chunk.set_voxel(lx, 11, lz, Voxel::new(VoxelType::Stone));
                }
            }
        }
        let mut mgr = ChunkManager::new(config.clone(), 2);
        mgr.chunks.insert(coord, chunk);

        let block_theta = config.chunk_angular_size() / size as f64;
        let y0 = -config.width / 2.0 + 8.5;
        let mut e = super::Entity::new(
            1,
            super::MobType::Stalker,
            crate::ring_world::RingPosition::new(4.5 * block_theta, y0, 11.5),
        );

        for _ in 0..400 {
            // Keep the walk target alive (wandering AI normally does this).
            e.target_position = Some(crate::ring_world::RingPosition::new(
                12.5 * block_theta,
                y0,
                e.position.height,
            ));
            super::apply_movement(&mut e, 0.05, &config, &mgr);
            super::apply_gravity(&mut e, 0.05, &config, &mgr);
        }

        let feet = e.position.height - e.mob_height() * 0.5;
        assert!(
            feet > 11.9 && feet < 12.2,
            "mob should stand on the step top, feet at {}",
            feet
        );
        assert!(
            e.position.theta > 9.0 * block_theta,
            "mob should have crossed the step, theta {}",
            e.position.theta
        );
    }

    #[test]
    fn damaged_mobs_flash_red_in_the_mesh() {
        let config = crate::ring_world::RingWorldConfig::default();
        let mut mgr = super::EntityManager::new();
        let pos = crate::ring_world::RingPosition::new(1.0, 0.0, 30.0);
        let id = mgr.spawn_entity(super::MobType::Grazer, pos);

        let (verts_before, _) = super::build_entity_mesh(&mgr.entities, &config);
        let knockback = crate::ring_world::RingPosition::new(0.9, 0.0, 30.0);
        assert!(mgr.damage_entity(id, 2.0, &knockback, &config));
        assert!(mgr.entities[0].hurt_timer > 0.0);
        let (verts_after, _) = super::build_entity_mesh(&mgr.entities, &config);

        // Same geometry, redder color: red channel up, green down.
        assert_eq!(verts_before.len(), verts_after.len());
        assert!(verts_after[0].color[0] > verts_before[0].color[0]);
        assert!(verts_after[0].color[1] < verts_before[0].color[1]);
    }

    #[test]
    fn standing_mob_height_is_rock_stable() {
        // Regression: the landing snap used floor(feet)+1, which re-read feet
        // sitting exactly on an integer as "inside the NEXT block up" and
        // teleported the mob up a block every other frame (visible as mobs
        // endlessly oscillating up and down). A mob standing still on flat
        // ground must keep a bit-stable height across hundreds of frames.
        use crate::chunk::{Chunk, ChunkManager};
        use crate::voxel::{Voxel, VoxelType};

        let config = crate::ring_world::RingWorldConfig::default();
        let size = config.chunk_size;
        let coord = crate::ring_world::ChunkCoord::new(0, 0, 0);
        let mut chunk = Chunk::new(coord, size);
        for lx in 0..size {
            for lz in 0..size {
                chunk.set_voxel(lx, 10, lz, Voxel::new(VoxelType::Stone));
            }
        }
        let mut mgr = ChunkManager::new(config.clone(), 2);
        mgr.chunks.insert(coord, chunk);

        let block_theta = config.chunk_angular_size() / size as f64;
        let y0 = -config.width / 2.0 + 8.5;
        let mut e = super::Entity::new(
            1,
            super::MobType::Grazer,
            crate::ring_world::RingPosition::new(8.0 * block_theta, y0, 13.0),
        );

        // Let it land first.
        for _ in 0..60 {
            super::apply_gravity(&mut e, 0.05, &config, &mgr);
        }
        let settled = e.position.height;
        let expected = 11.0 + e.mob_height() * 0.5; // stone top at 11
        assert!(
            (settled - expected).abs() < 1e-9,
            "settled at {} expected {}",
            settled,
            expected
        );
        for i in 0..400 {
            super::apply_gravity(&mut e, 0.05, &config, &mgr);
            assert!(
                (e.position.height - settled).abs() < 1e-9,
                "height drifted to {} at frame {}",
                e.position.height,
                i
            );
        }
    }

    #[test]
    fn dead_entities_are_not_meshed() {
        let config = crate::ring_world::RingWorldConfig::default();
        let mut e = super::Entity::new(
            1,
            super::MobType::Skitterling,
            crate::ring_world::RingPosition::new(1.0, 0.0, 10.0),
        );
        e.alive = false;
        let (verts, idx) = super::build_entity_mesh(&[e], &config);
        assert!(verts.is_empty() && idx.is_empty());
    }


    use super::*;

    fn test_pos() -> RingPosition {
        RingPosition::new(1.0, 0.0, 30.0)
    }

    #[test]
    fn new_entity_manager_is_empty() {
        let mgr = EntityManager::new();
        assert_eq!(mgr.entity_count(), 0);
    }

    #[test]
    fn spawn_entity_adds_with_correct_properties() {
        let mut mgr = EntityManager::new();
        let id = mgr.spawn_entity(MobType::Sentinel, test_pos());
        assert_ne!(id, 0);
        assert_eq!(mgr.entity_count(), 1);
        let e = &mgr.entities[0];
        assert_eq!(e.mob_type, MobType::Sentinel);
        assert_eq!(e.health, 30.0);
        assert_eq!(e.max_health, 30.0);
        assert_eq!(e.behavior, MobBehavior::Hostile);
        assert!(e.alive);
    }

    #[test]
    fn mob_property_table_correct() {
        // (health, damage, speed, aggro_range, behavior)
        assert_eq!(mob_properties(MobType::Sentinel).0, 30.0);
        assert_eq!(mob_properties(MobType::Stalker).0, 16.0);
        assert_eq!(mob_properties(MobType::Grazer).0, 12.0);
        assert_eq!(mob_properties(MobType::Skitterling).0, 4.0);
        assert_eq!(mob_properties(MobType::Ringkin).0, 16.0);

        // Passive mobs (fauna + natives) deal no damage
        assert_eq!(mob_properties(MobType::Grazer).1, 0.0);
        assert_eq!(mob_properties(MobType::Ringkin).1, 0.0);
        // Hostile mobs deal damage
        assert!(mob_properties(MobType::Sentinel).1 > 0.0);

        // Behavior categories
        assert_eq!(mob_properties(MobType::Floater).4, MobBehavior::Passive);
        assert_eq!(mob_properties(MobType::Stalker).4, MobBehavior::Hostile);

        // Only the Floater hovers
        assert!(MobType::Floater.hovers());
        assert!(!MobType::Sentinel.hovers());
    }

    #[test]
    fn spawn_respects_max_entities() {
        let mut mgr = EntityManager::new();
        mgr.max_entities = 2;
        assert_ne!(mgr.spawn_entity(MobType::Grazer, test_pos()), 0);
        assert_ne!(mgr.spawn_entity(MobType::Grazer, test_pos()), 0);
        // Third spawn should fail (returns 0)
        assert_eq!(mgr.spawn_entity(MobType::Grazer, test_pos()), 0);
        assert_eq!(mgr.entity_count(), 2);
    }

    #[test]
    fn damage_entity_reduces_health() {
        let config = RingWorldConfig::default();
        let mut mgr = EntityManager::new();
        let id = mgr.spawn_entity(MobType::Sentinel, test_pos());
        let knockback = RingPosition::new(0.9, 0.0, 30.0);
        assert!(mgr.damage_entity(id, 5.0, &knockback, &config));
        let e = mgr.entities.iter().find(|e| e.id == id).unwrap();
        assert_eq!(e.health, 25.0);
        assert!(e.alive);
    }

    #[test]
    fn damage_entity_kills_at_zero() {
        let config = RingWorldConfig::default();
        let mut mgr = EntityManager::new();
        let id = mgr.spawn_entity(MobType::Skitterling, test_pos());
        let knockback = RingPosition::new(0.9, 0.0, 30.0);
        // Skitterling has 4 health
        assert!(mgr.damage_entity(id, 100.0, &knockback, &config));
        let e = mgr.entities.iter().find(|e| e.id == id).unwrap();
        assert!(!e.alive);
        assert!(e.health <= 0.0);
    }

    #[test]
    fn damage_unknown_entity_returns_false() {
        let config = RingWorldConfig::default();
        let mut mgr = EntityManager::new();
        let knockback = test_pos();
        assert!(!mgr.damage_entity(9999, 5.0, &knockback, &config));
    }

    #[test]
    fn ring_distance_zero_for_same_position() {
        let config = RingWorldConfig::default();
        let p = test_pos();
        assert!(ring_distance(&p, &p, &config) < 1e-6);
    }

    #[test]
    fn ring_distance_axial_matches_difference() {
        let config = RingWorldConfig::default();
        let a = RingPosition::new(1.0, 0.0, 30.0);
        let b = RingPosition::new(1.0, 5.0, 30.0);
        let d = ring_distance(&a, &b, &config);
        assert!((d - 5.0).abs() < 1e-4, "expected ~5.0 got {}", d);
    }

    #[test]
    fn ring_distance_theta_scaled_by_radius() {
        let config = RingWorldConfig::default();
        let d_theta = 0.01;
        let a = RingPosition::new(1.0, 0.0, 30.0);
        let b = RingPosition::new(1.0 + d_theta, 0.0, 30.0);
        let d = ring_distance(&a, &b, &config);
        let expected = (d_theta * config.radius) as f32;
        assert!((d - expected).abs() < 1e-2, "expected ~{} got {}", expected, d);
    }

    #[test]
    fn entity_drop_items_correct() {
        let e = Entity::new(1, MobType::Stalker, test_pos());
        assert_eq!(e.drop_item(), VoxelType::Vine);
        let z = Entity::new(2, MobType::Sentinel, test_pos());
        assert_eq!(z.drop_item(), VoxelType::IronIngot);
    }
}
