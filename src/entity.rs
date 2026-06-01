/// Entity & Mob system for the ring world (v0.3)
/// Implements a simple ECS with mob types, AI, spawning, health, and combat.

use rand::Rng;
use crate::chunk::ChunkManager;
use crate::inventory::Inventory;
use crate::lighting;
use crate::ring_world::{ChunkCoord, RingPosition, RingWorldConfig};
use crate::voxel::VoxelType;

/// Types of mobs that can exist in the world
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MobType {
    Sheep,
    Cow,
    Pig,
    Chicken,
    Zombie,
    Skeleton,
    Spider,
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
        }
    }

    pub fn drop_item(&self) -> VoxelType {
        match self.mob_type {
            MobType::Sheep => VoxelType::Dirt,
            MobType::Cow => VoxelType::Dirt,
            MobType::Pig => VoxelType::Dirt,
            MobType::Chicken => VoxelType::Dirt,
            MobType::Zombie => VoxelType::IronOre,
            MobType::Skeleton => VoxelType::GoldOre,
            MobType::Spider => VoxelType::Vine,
        }
    }

    pub fn render_color(&self) -> [f32; 4] {
        match self.mob_type {
            MobType::Sheep => [0.9, 0.9, 0.9, 1.0],
            MobType::Cow => [0.4, 0.2, 0.1, 1.0],
            MobType::Pig => [0.9, 0.6, 0.6, 1.0],
            MobType::Chicken => [1.0, 1.0, 0.8, 1.0],
            MobType::Zombie => [0.3, 0.5, 0.3, 1.0],
            MobType::Skeleton => [0.8, 0.8, 0.75, 1.0],
            MobType::Spider => [0.2, 0.2, 0.2, 1.0],
        }
    }

    pub fn mob_height(&self) -> f64 {
        match self.mob_type {
            MobType::Sheep => 1.3,
            MobType::Cow => 1.5,
            MobType::Pig => 1.0,
            MobType::Chicken => 0.7,
            MobType::Zombie => 2.0,
            MobType::Skeleton => 2.0,
            MobType::Spider => 0.8,
        }
    }
}

fn mob_properties(mob_type: MobType) -> (f32, f32, f64, f32, MobBehavior) {
    match mob_type {
        MobType::Sheep => (8.0, 0.0, 1.5, 0.0, MobBehavior::Passive),
        MobType::Cow => (10.0, 0.0, 1.5, 0.0, MobBehavior::Passive),
        MobType::Pig => (10.0, 0.0, 1.5, 0.0, MobBehavior::Passive),
        MobType::Chicken => (4.0, 0.0, 2.0, 0.0, MobBehavior::Passive),
        MobType::Zombie => (20.0, 3.0, 2.0, 16.0, MobBehavior::Hostile),
        MobType::Skeleton => (20.0, 4.0, 2.0, 24.0, MobBehavior::Hostile),
        MobType::Spider => (16.0, 2.0, 3.0, 12.0, MobBehavior::Hostile),
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
    ) {
        self.spawn_timer += dt;
        if self.spawn_timer >= 5.0 {
            self.spawn_timer = 0.0;
            self.try_spawn(player_position, chunk_manager, config);
        }

        let player_pos = *player_position;
        for entity in self.entities.iter_mut() {
            if !entity.alive {
                continue;
            }

            if entity.attack_timer > 0.0 {
                entity.attack_timer -= dt;
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

                let mob_type = if light_level < 7 {
                    // Dark areas: hostile mobs spawn (caves, underground, unlit areas)
                    match rng.gen_range(0..3u32) {
                        0 => MobType::Zombie,
                        1 => MobType::Skeleton,
                        _ => MobType::Spider,
                    }
                } else {
                    // Well-lit areas: passive mobs only
                    match rng.gen_range(0..4u32) {
                        0 => MobType::Sheep,
                        1 => MobType::Cow,
                        2 => MobType::Pig,
                        _ => MobType::Chicken,
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
            let move_speed = entity.speed * dt_f64;
            let move_frac = (move_speed / dist).min(1.0);

            let new_theta = entity.position.theta + (d_theta / config.radius) * move_frac;
            let new_y = entity.position.y + d_y * move_frac;

            let check_pos = RingPosition::new(new_theta, new_y, entity.position.height);
            if !is_position_solid_entity(&check_pos, config, chunk_manager) {
                entity.position.theta = new_theta;
                entity.position.y = new_y;
            } else {
                let check_theta = RingPosition::new(new_theta, entity.position.y, entity.position.height);
                if !is_position_solid_entity(&check_theta, config, chunk_manager) {
                    entity.position.theta = new_theta;
                } else {
                    let check_y = RingPosition::new(entity.position.theta, new_y, entity.position.height);
                    if !is_position_solid_entity(&check_y, config, chunk_manager) {
                        entity.position.y = new_y;
                    }
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
        let block_top = feet_height.floor() + 1.0;
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
        let id = mgr.spawn_entity(MobType::Zombie, test_pos());
        assert_ne!(id, 0);
        assert_eq!(mgr.entity_count(), 1);
        let e = &mgr.entities[0];
        assert_eq!(e.mob_type, MobType::Zombie);
        assert_eq!(e.health, 20.0);
        assert_eq!(e.max_health, 20.0);
        assert_eq!(e.behavior, MobBehavior::Hostile);
        assert!(e.alive);
    }

    #[test]
    fn mob_property_table_correct() {
        // (health, damage, speed, aggro_range, behavior)
        assert_eq!(mob_properties(MobType::Zombie).0, 20.0);
        assert_eq!(mob_properties(MobType::Skeleton).0, 20.0);
        assert_eq!(mob_properties(MobType::Spider).0, 16.0);
        assert_eq!(mob_properties(MobType::Sheep).0, 8.0);
        assert_eq!(mob_properties(MobType::Chicken).0, 4.0);

        // Passive mobs deal no damage
        assert_eq!(mob_properties(MobType::Sheep).1, 0.0);
        // Hostile mobs deal damage
        assert!(mob_properties(MobType::Zombie).1 > 0.0);

        // Behavior categories
        assert_eq!(mob_properties(MobType::Cow).4, MobBehavior::Passive);
        assert_eq!(mob_properties(MobType::Skeleton).4, MobBehavior::Hostile);
    }

    #[test]
    fn spawn_respects_max_entities() {
        let mut mgr = EntityManager::new();
        mgr.max_entities = 2;
        assert_ne!(mgr.spawn_entity(MobType::Sheep, test_pos()), 0);
        assert_ne!(mgr.spawn_entity(MobType::Sheep, test_pos()), 0);
        // Third spawn should fail (returns 0)
        assert_eq!(mgr.spawn_entity(MobType::Sheep, test_pos()), 0);
        assert_eq!(mgr.entity_count(), 2);
    }

    #[test]
    fn damage_entity_reduces_health() {
        let config = RingWorldConfig::default();
        let mut mgr = EntityManager::new();
        let id = mgr.spawn_entity(MobType::Zombie, test_pos());
        let knockback = RingPosition::new(0.9, 0.0, 30.0);
        assert!(mgr.damage_entity(id, 5.0, &knockback, &config));
        let e = mgr.entities.iter().find(|e| e.id == id).unwrap();
        assert_eq!(e.health, 15.0);
        assert!(e.alive);
    }

    #[test]
    fn damage_entity_kills_at_zero() {
        let config = RingWorldConfig::default();
        let mut mgr = EntityManager::new();
        let id = mgr.spawn_entity(MobType::Chicken, test_pos());
        let knockback = RingPosition::new(0.9, 0.0, 30.0);
        // Chicken has 4 health
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
        let e = Entity::new(1, MobType::Spider, test_pos());
        assert_eq!(e.drop_item(), VoxelType::Vine);
        let z = Entity::new(2, MobType::Zombie, test_pos());
        assert_eq!(z.drop_item(), VoxelType::IronOre);
    }
}
