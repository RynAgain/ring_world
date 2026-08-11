/// Shadow squares - the Niven Ringworld day/night system.
///
/// The sun sits at the ring's center and never moves: local noon is eternal.
/// Night is produced by a chain of rectangular SHADOW SQUARES orbiting between
/// the sun and the ring. When a square passes between a point on the ring and
/// the sun, that point falls into eclipse; the terminator sweeps along the
/// ring as the squares orbit, and the far side of the arch overhead stays lit
/// during local night (it is shadowed by a DIFFERENT square at a different
/// time).
///
/// The lighting side is computed per fragment in shader.wgsl from the
/// `eclipse` vec4 in SunUniform: (count, phase, half_arc, softness).
/// `daylight_at` here is the exact CPU mirror of that shader formula, used by
/// tests and the F3 overlay.

use crate::chunk::ChunkVertex;
use crate::ring_world::RingWorldConfig;
use crate::texture::TEX_STONE;

/// Seconds for one full day+night cycle at a fixed point on the ring
/// (the time for the square pattern to advance by one period).
pub const DAY_CYCLE_SECS: f64 = 600.0;

/// Fraction of the cycle spent in (umbral) night.
pub const NIGHT_FRACTION: f64 = 0.35;

/// Number of shadow squares in the orbital chain.
pub const SQUARE_COUNT: u32 = 6;

/// Penumbra softness (radians of ring angle): the terminator band width.
/// 0.03 rad at R~652 is a ~20-block-wide dawn/dusk gradient.
pub const PENUMBRA_SOFTNESS: f32 = 0.03;

/// Arc segments per square panel (so the panel hugs its orbit circle).
const PANEL_ARC_SEGMENTS: u32 = 8;

const TWO_PI: f64 = std::f64::consts::PI * 2.0;

pub struct ShadowSquares {
    /// Number of squares, evenly spaced around the orbit.
    pub count: u32,
    /// Orbit radius (world units from the ring axis).
    pub orbit_radius: f32,
    /// Axial half-height of each panel (world units).
    pub axial_half_height: f32,
    /// Angular half-width of each square as seen from the ring axis (radians).
    pub half_arc: f32,
    /// Penumbra softness (radians).
    pub softness: f32,
    /// Current orbital phase: the angle of square 0's center.
    pub phase: f64,
    /// Orbital angular velocity (radians/second).
    pub omega: f64,
}

impl ShadowSquares {
    pub fn new(config: &RingWorldConfig) -> Self {
        let count = SQUARE_COUNT;
        // Umbral night fraction = count * 2*half_arc / 2*PI.
        let half_arc = (NIGHT_FRACTION * std::f64::consts::PI / count as f64) as f32;
        // One period (2*PI / count) sweeps past a fixed point per DAY_CYCLE.
        let omega = (TWO_PI / count as f64) / DAY_CYCLE_SECS;
        Self {
            count,
            orbit_radius: (config.radius * 0.4) as f32,
            axial_half_height: (config.width * 0.35) as f32,
            half_arc,
            softness: PENUMBRA_SOFTNESS,
            phase: 0.0,
            omega,
        }
    }

    /// Advance the orbit by `dt` seconds (already time-scaled by the caller).
    pub fn update(&mut self, dt: f64) {
        self.phase = (self.phase + self.omega * dt).rem_euclid(TWO_PI);
    }

    /// Daylight factor at ring angle `theta`: 1.0 = full noon, 0.0 = umbral
    /// night, smooth penumbra in between. EXACT CPU mirror of the shader
    /// formula in shader.wgsl (keep the two in sync).
    pub fn daylight_at(&self, theta: f64) -> f32 {
        let period = TWO_PI / self.count as f64;
        let rel = (theta - self.phase).rem_euclid(period);
        let d = rel.min(period - rel) as f32;
        smoothstep(self.half_arc, self.half_arc + self.softness, d)
    }

    /// Pack the shader uniform payload: (count, phase, half_arc, softness).
    pub fn eclipse_uniform(&self) -> [f32; 4] {
        [
            self.count as f32,
            self.phase as f32,
            self.half_arc,
            self.softness,
        ]
    }

    /// Build the panels' world-space vertices for the current phase. The
    /// panels are dark, sun-blocking silhouettes: their ring-facing side gets
    /// no diffuse (normal points away from the sun) and a near-black tint.
    /// Rebuilt per frame (the panel count is constant, so the index buffer
    /// from `build_indices` stays valid).
    pub fn build_vertices(&self) -> Vec<ChunkVertex> {
        let mut verts = Vec::with_capacity((self.count * (PANEL_ARC_SEGMENTS + 1) * 2) as usize);
        let period = TWO_PI / self.count as f64;
        let r = self.orbit_radius;
        let h = self.axial_half_height;
        // Near-black with a hint of blue: an unlit megastructure panel.
        let color = [0.02f32, 0.02, 0.035, 1.0];

        for k in 0..self.count {
            let center = self.phase + k as f64 * period;
            for s in 0..=PANEL_ARC_SEGMENTS {
                let t = s as f64 / PANEL_ARC_SEGMENTS as f64;
                let ang = center - self.half_arc as f64 + t * (2.0 * self.half_arc as f64);
                let (sin_a, cos_a) = (ang.sin() as f32, ang.cos() as f32);
                // Normal faces the ring surface (radially OUT from the axis),
                // i.e. away from the sun: the panel's dark back side.
                let normal = [cos_a, 0.0, sin_a];
                for &y in &[-h, h] {
                    verts.push(ChunkVertex {
                        position: [r * cos_a, y, r * sin_a],
                        normal,
                        color,
                        tex_coords: [t as f32, (y > 0.0) as u32 as f32],
                        tex_index: TEX_STONE,
                        light_level: 0.0,
                        alpha_tested: 0,
                    });
                }
            }
        }
        verts
    }

    /// Static triangle-list indices matching `build_vertices`' layout.
    pub fn build_indices(&self) -> Vec<u32> {
        let mut idx = Vec::new();
        let verts_per_panel = (PANEL_ARC_SEGMENTS + 1) * 2;
        for k in 0..self.count {
            let base = k * verts_per_panel;
            for s in 0..PANEL_ARC_SEGMENTS {
                let a = base + s * 2; // (segment s, -h)
                let b = a + 1; //        (segment s, +h)
                let c = a + 2; //        (segment s+1, -h)
                let d = a + 3; //        (segment s+1, +h)
                // Two triangles per strip segment. The pipeline that draws the
                // panels has cull_mode: None, so winding order is cosmetic.
                idx.extend_from_slice(&[a, b, c, c, b, d]);
            }
        }
        idx
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn squares() -> ShadowSquares {
        ShadowSquares::new(&RingWorldConfig::default())
    }

    #[test]
    fn square_center_is_full_night() {
        let s = squares();
        // phase = 0 puts square 0's center at theta = 0.
        assert_eq!(s.daylight_at(0.0), 0.0);
        // Every square center is night.
        let period = TWO_PI / s.count as f64;
        for k in 0..s.count {
            assert_eq!(s.daylight_at(k as f64 * period), 0.0, "square {}", k);
        }
    }

    #[test]
    fn midpoint_between_squares_is_full_day() {
        let s = squares();
        let period = TWO_PI / s.count as f64;
        assert_eq!(s.daylight_at(period / 2.0), 1.0);
    }

    #[test]
    fn terminator_is_smooth_and_monotonic() {
        let s = squares();
        let mut prev = -1.0f32;
        // Walk outward from the square center: daylight must rise 0 -> 1.
        let steps = 200;
        let period = TWO_PI / s.count as f64;
        for i in 0..=steps {
            let theta = (i as f64 / steps as f64) * (period / 2.0);
            let d = s.daylight_at(theta);
            assert!(d >= prev - 1e-6, "daylight not monotonic at {}", theta);
            prev = d;
        }
        assert_eq!(prev, 1.0);
    }

    #[test]
    fn night_fraction_matches_config() {
        let s = squares();
        let samples = 100_000;
        let mut night = 0u32;
        for i in 0..samples {
            let theta = (i as f64 / samples as f64) * TWO_PI;
            if s.daylight_at(theta) < 0.5 {
                night += 1;
            }
        }
        let frac = night as f64 / samples as f64;
        // Umbra fraction plus half the penumbra band on each side.
        assert!(
            (frac - NIGHT_FRACTION).abs() < 0.03,
            "night fraction {} vs configured {}",
            frac,
            NIGHT_FRACTION
        );
    }

    #[test]
    fn phase_advances_and_wraps() {
        let mut s = squares();
        // A full day cycle advances the phase by exactly one period.
        s.update(DAY_CYCLE_SECS);
        let period = TWO_PI / s.count as f64;
        assert!((s.phase - period).abs() < 1e-9);
        // The pattern is period-symmetric: daylight is unchanged.
        assert_eq!(s.daylight_at(0.0), 0.0);
        // Wraps past 2*PI.
        s.update(DAY_CYCLE_SECS * s.count as f64 * 10.0);
        assert!(s.phase >= 0.0 && s.phase < TWO_PI);
    }

    #[test]
    fn geometry_buffers_are_consistent() {
        let s = squares();
        let verts = s.build_vertices();
        let idx = s.build_indices();
        assert_eq!(verts.len() as u32, s.count * (PANEL_ARC_SEGMENTS + 1) * 2);
        assert!(idx.iter().all(|&i| (i as usize) < verts.len()));
        // Panels sit exactly on the orbit radius.
        for v in &verts {
            let r = (v.position[0].powi(2) + v.position[2].powi(2)).sqrt();
            assert!((r - s.orbit_radius).abs() < 1e-3);
        }
    }
}
