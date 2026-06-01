/// Visual effects module - particle system, weather state, and effect helpers

/// A single particle in the particle system
#[derive(Clone, Debug)]
pub struct Particle {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub color: [f32; 4],
    pub size: f32,
}

impl Particle {
    pub fn new(position: [f32; 3], velocity: [f32; 3], lifetime: f32, color: [f32; 4], size: f32) -> Self {
        Self {
            position,
            velocity,
            lifetime,
            max_lifetime: lifetime,
            color,
            size,
        }
    }

    /// Returns true if the particle has expired
    pub fn is_dead(&self) -> bool {
        self.lifetime <= 0.0
    }

    /// Get the normalized age (0.0 = just born, 1.0 = about to die)
    pub fn age_fraction(&self) -> f32 {
        1.0 - (self.lifetime / self.max_lifetime).clamp(0.0, 1.0)
    }
}

/// Maximum number of particles allowed in the system
const MAX_PARTICLES: usize = 500;

/// Gravity applied to particles (blocks/s^2)
const PARTICLE_GRAVITY: f32 = 9.8;

/// CPU-side particle system for visual effects (data only, no GPU rendering yet)
pub struct ParticleSystem {
    pub particles: Vec<Particle>,
}

impl ParticleSystem {
    pub fn new() -> Self {
        Self {
            particles: Vec::with_capacity(MAX_PARTICLES),
        }
    }

    /// Spawn particles when a block is broken
    /// `position` is the world-space center of the broken block
    /// `color` is the block's face color
    /// `count` is the number of particles to spawn (clamped to 5-10)
    pub fn spawn_block_break_particles(&mut self, position: [f32; 3], color: [f32; 4], count: u32) {
        let count = count.clamp(5, 10) as usize;

        // Don't exceed max particles
        let available = MAX_PARTICLES.saturating_sub(self.particles.len());
        let spawn_count = count.min(available);

        for i in 0..spawn_count {
            // Spread particles in a small random-ish pattern using index-based pseudo-randomness
            let angle = (i as f32 / spawn_count as f32) * std::f32::consts::TAU;
            let spread = 0.3 + (i as f32 * 0.1) % 0.4;
            let up_speed = 2.0 + (i as f32 * 0.7) % 2.0;

            let velocity = [
                angle.cos() * spread,
                up_speed,
                angle.sin() * spread,
            ];

            let lifetime = 0.5 + (i as f32 * 0.13) % 0.5;
            let size = 0.05 + (i as f32 * 0.02) % 0.05;

            self.particles.push(Particle::new(
                position,
                velocity,
                lifetime,
                color,
                size,
            ));
        }
    }

    /// Update all particles: move them, apply gravity, remove expired ones
    pub fn update(&mut self, dt: f32) {
        for particle in self.particles.iter_mut() {
            // Apply gravity
            particle.velocity[1] -= PARTICLE_GRAVITY * dt;

            // Move particle
            particle.position[0] += particle.velocity[0] * dt;
            particle.position[1] += particle.velocity[1] * dt;
            particle.position[2] += particle.velocity[2] * dt;

            // Decrease lifetime
            particle.lifetime -= dt;

            // Fade out as particle ages
            let fade = 1.0 - particle.age_fraction();
            particle.color[3] = fade * particle.color[3].min(1.0);
        }

        // Remove dead particles
        self.particles.retain(|p| !p.is_dead());
    }

    /// Get the current particle count
    pub fn count(&self) -> usize {
        self.particles.len()
    }

    /// Clear all particles
    pub fn clear(&mut self) {
        self.particles.clear();
    }
}

/// Weather states for the world
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeatherState {
    Clear,
    Rain,
    Snow,
}

/// Weather system that manages weather state transitions
pub struct Weather {
    pub current_state: WeatherState,
    pub transition_timer: f32,
    pub state_duration: f32,
}

impl Weather {
    pub fn new() -> Self {
        Self {
            current_state: WeatherState::Clear,
            transition_timer: 0.0,
            state_duration: 300.0, // 5 minutes default duration
        }
    }

    /// Update the weather system (state machine, no visual rendering)
    pub fn update(&mut self, dt: f32) {
        self.transition_timer += dt;

        // Auto-transition after duration (simple cycling for now)
        if self.transition_timer >= self.state_duration {
            self.transition_timer = 0.0;
            self.current_state = match self.current_state {
                WeatherState::Clear => WeatherState::Rain,
                WeatherState::Rain => WeatherState::Snow,
                WeatherState::Snow => WeatherState::Clear,
            };
        }
    }

    /// Force a specific weather state
    pub fn set_weather(&mut self, state: WeatherState) {
        self.current_state = state;
        self.transition_timer = 0.0;
    }

    /// Get the transition progress (0.0 to 1.0) within current state
    pub fn transition_progress(&self) -> f32 {
        (self.transition_timer / self.state_duration).clamp(0.0, 1.0)
    }
}

/// Visual effects manager that combines all effect systems
pub struct EffectsManager {
    pub particles: ParticleSystem,
    pub weather: Weather,
}

impl EffectsManager {
    pub fn new() -> Self {
        Self {
            particles: ParticleSystem::new(),
            weather: Weather::new(),
        }
    }

    /// Update all effect systems
    pub fn update(&mut self, dt: f32) {
        self.particles.update(dt);
        self.weather.update(dt);
    }
}
