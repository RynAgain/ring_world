/// Sun module - the light source at the center of the ring

use cgmath::Vector3;

/// The sun sits at the center of the ring (origin)
/// It provides directional lighting to the inner surface
pub struct Sun {
    /// Position is always at origin (center of ring)
    pub position: Vector3<f32>,
    /// Color of the sunlight
    pub color: [f32; 3],
    /// Intensity multiplier
    pub intensity: f32,
    /// Ambient light level (simulates light bouncing around the ring)
    pub ambient: f32,
}

impl Sun {
    pub fn new() -> Self {
        Self {
            position: Vector3::new(0.0, 0.0, 0.0),
            color: [1.0, 0.95, 0.8], // Warm white
            intensity: 1.5,
            ambient: 0.3, // Ring worlds have high ambient due to reflected light from opposite side
        }
    }

    /// Get the light direction for a point on the ring surface
    /// Light always points from center outward (toward the surface)
    /// But since player is on inner surface, light comes from "above" (toward center)
    pub fn light_direction_at(&self, surface_position: Vector3<f32>) -> Vector3<f32> {
        use cgmath::InnerSpace;
        // Direction from surface point toward center (sun)
        let to_sun = self.position - surface_position;
        to_sun.normalize()
    }
}

/// Sun uniform data for the shader
///
/// Layout note: `ambient` is a `vec4` where `ambient.xyz` is the ambient color
/// and `ambient.w` carries the render-debug flag (`debug_mode`). A non-zero
/// `ambient.w` switches the fragment shader into the F6 render-diagnostic mode
/// (full-bright per-face normal tint, no fog, no alpha discard). Stashing the
/// flag in the existing padding slot keeps the struct size, `#[repr(C)]` layout
/// and the bind-group layout unchanged.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SunUniform {
    /// Sun position (center of ring)
    pub position: [f32; 4],
    /// Sun color and intensity packed
    pub color: [f32; 4],
    /// Ambient light (xyz) + debug_mode flag (w): 0 = normal, 1 = render-debug.
    pub ambient: [f32; 4],
}

impl SunUniform {
    pub fn new() -> Self {
        Self {
            position: [0.0, 0.0, 0.0, 1.0],
            color: [1.0, 0.95, 0.8, 1.5],
            ambient: [0.3, 0.3, 0.3, 0.0],
        }
    }

    pub fn from_sun(sun: &Sun) -> Self {
        Self {
            position: [sun.position.x, sun.position.y, sun.position.z, 1.0],
            color: [sun.color[0], sun.color[1], sun.color[2], sun.intensity],
            ambient: [sun.ambient, sun.ambient, sun.ambient, 0.0],
        }
    }

    /// Set the render-debug flag (F6). When enabled the fragment shader outputs
    /// a per-face normal tint at full brightness and skips fog/discard.
    pub fn set_debug_mode(&mut self, enabled: bool) {
        self.ambient[3] = if enabled { 1.0 } else { 0.0 };
    }
}
