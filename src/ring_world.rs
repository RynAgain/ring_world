/// Ring World geometry module
/// 
/// The ring world is defined by:
/// - A major radius R (distance from the center of the ring to the center of the habitable surface)
/// - A width W (the extent along the ring's axis - the "edge" boundaries)
/// - The habitable surface is on the INNER edge of the ring
/// - A sun sits at the center of the ring
///
/// Coordinate system:
/// - Ring coordinates: (theta, y, r) where theta is angle around ring, y is axial position, r is radial
/// - theta wraps around [0, 2*PI) - going "forward" brings you back
/// - y is bounded [-W/2, W/2] - there are edges here
/// - r extends from the ring surface outward (toward center/sun)

use cgmath::{Vector3, Matrix4};

/// Configuration for the ring world
#[derive(Clone, Debug)]
pub struct RingWorldConfig {
    /// Major radius of the ring (distance from center to inner surface)
    pub radius: f64,
    /// Width of the ring (axial extent)
    pub width: f64,
    /// Maximum height of terrain/builds above the surface
    pub max_height: f64,
    /// Size of each voxel in world units
    pub voxel_size: f64,
    /// Number of chunks around the circumference
    pub chunks_circumference: u32,
    /// Number of chunks along the width
    pub chunks_width: u32,
    /// Number of chunks in height (radial direction, toward sun)
    pub chunks_height: u32,
    /// Size of each chunk in voxels (cubic chunks)
    pub chunk_size: u32,
}

impl Default for RingWorldConfig {
    fn default() -> Self {
        // Calculate radius and width so voxels are cubic (1.0 × 1.0 × 1.0)
        // radius = chunks_circumference * chunk_size / (2 * PI)
        // width = chunks_width * chunk_size
        let chunks_circumference = 256u32;
        let chunks_width = 16u32;
        let chunks_height = 4u32;
        let chunk_size = 16u32;
        let radius = (chunks_circumference as f64 * chunk_size as f64) / (2.0 * std::f64::consts::PI);
        let width = (chunks_width * chunk_size) as f64;
        let max_height = (chunks_height * chunk_size) as f64;

        Self {
            radius, // ~651.9 for cubic voxels
            width,  // 256.0
            max_height, // 64.0
            voxel_size: 1.0,
            chunks_circumference,
            chunks_width,
            chunks_height,
            chunk_size,
        }
    }
}

impl RingWorldConfig {
    /// Total circumference of the ring
    pub fn circumference(&self) -> f64 {
        2.0 * std::f64::consts::PI * self.radius
    }

    /// Angular extent of one chunk
    pub fn chunk_angular_size(&self) -> f64 {
        2.0 * std::f64::consts::PI / self.chunks_circumference as f64
    }

    /// Width of one chunk in world units
    pub fn chunk_width_size(&self) -> f64 {
        self.width / self.chunks_width as f64
    }

    /// Height of one chunk in world units
    pub fn chunk_height_size(&self) -> f64 {
        self.max_height / self.chunks_height as f64
    }
}

/// Position in ring-local coordinates
#[derive(Clone, Copy, Debug)]
pub struct RingPosition {
    /// Angle around the ring [0, 2*PI)
    pub theta: f64,
    /// Axial position [-width/2, width/2]
    pub y: f64,
    /// Height above surface (radial inward toward sun)
    pub height: f64,
}

impl RingPosition {
    pub fn new(theta: f64, y: f64, height: f64) -> Self {
        Self { theta, y, height }
    }

    /// Normalize theta to [0, 2*PI)
    pub fn normalize_theta(&mut self) {
        let two_pi = 2.0 * std::f64::consts::PI;
        self.theta = ((self.theta % two_pi) + two_pi) % two_pi;
    }

    /// Convert ring position to 3D Cartesian coordinates
    /// The ring lies in the XZ plane, with Y being the axial direction
    /// The inner surface faces toward the center (origin)
    pub fn to_cartesian(&self, config: &RingWorldConfig) -> Vector3<f64> {
        // r decreases toward center (sun), surface is at radius, height goes inward
        let r = config.radius - self.height;
        let x = r * self.theta.cos();
        let z = r * self.theta.sin();
        let y = self.y;
        Vector3::new(x, y, z)
    }

    /// Convert Cartesian coordinates back to ring position
    pub fn from_cartesian(pos: Vector3<f64>, config: &RingWorldConfig) -> Self {
        let r = (pos.x * pos.x + pos.z * pos.z).sqrt();
        let theta = pos.z.atan2(pos.x);
        let theta = if theta < 0.0 {
            theta + 2.0 * std::f64::consts::PI
        } else {
            theta
        };
        let height = config.radius - r;
        let y = pos.y;
        Self { theta, y, height }
    }

    /// Check if position is within ring bounds.
    ///
    /// The habitable volume is the half-open height range `[0, max_height)`:
    /// `max_height` itself is the exclusive ceiling (it equals
    /// `chunks_height * chunk_size`, which is one past the last valid voxel
    /// index), so a position at exactly `height == max_height` lies outside any
    /// chunk and must NOT be considered valid. The axial `y` range is the closed
    /// interval `[-W/2, W/2]` (the ring edges are valid surface positions).
    pub fn is_valid(&self, config: &RingWorldConfig) -> bool {
        let half_width = config.width / 2.0;
        self.y >= -half_width && self.y <= half_width && self.height >= 0.0 && self.height < config.max_height
    }

    /// Clamp to ring bounds (theta wraps, y and height clamp).
    ///
    /// The height ceiling is clamped to just BELOW `max_height` (which is the
    /// exclusive top of the voxel grid) so that a clamped position always
    /// satisfies `is_valid` — clamping to exactly `max_height` would land on the
    /// invalid ceiling boundary that has no containing voxel.
    pub fn clamp(&mut self, config: &RingWorldConfig) {
        self.normalize_theta();
        let half_width = config.width / 2.0;
        self.y = self.y.clamp(-half_width, half_width);
        // `max_height - voxel_size` keeps us in the top valid voxel layer.
        let height_ceiling = (config.max_height - config.voxel_size).max(0.0);
        self.height = self.height.clamp(0.0, height_ceiling);
    }
}

/// Chunk coordinate in the ring world grid
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChunkCoord {
    /// Index around the circumference [0, chunks_circumference)
    pub ring_index: u32,
    /// Index along the width [0, chunks_width)
    pub width_index: u32,
    /// Index in height [0, chunks_height)
    pub height_index: u32,
}

impl ChunkCoord {
    pub fn new(ring_index: u32, width_index: u32, height_index: u32) -> Self {
        Self {
            ring_index,
            width_index,
            height_index,
        }
    }

    /// Get the ring position of the chunk's origin (minimum corner)
    pub fn to_ring_position(&self, config: &RingWorldConfig) -> RingPosition {
        let theta = self.ring_index as f64 * config.chunk_angular_size();
        let y = -config.width / 2.0 + self.width_index as f64 * config.chunk_width_size();
        let height = self.height_index as f64 * config.chunk_height_size();
        RingPosition::new(theta, y, height)
    }

    /// Get chunk coordinate from a ring position
    pub fn from_ring_position(pos: &RingPosition, config: &RingWorldConfig) -> Self {
        let ring_index = (pos.theta / config.chunk_angular_size()) as u32 % config.chunks_circumference;
        let width_index = ((pos.y + config.width / 2.0) / config.chunk_width_size()) as u32;
        let width_index = width_index.min(config.chunks_width - 1);
        let height_index = (pos.height / config.chunk_height_size()) as u32;
        let height_index = height_index.min(config.chunks_height - 1);
        Self::new(ring_index, width_index, height_index)
    }

    /// Get neighbor chunk, wrapping around the ring for ring_index
    pub fn neighbor(&self, d_ring: i32, d_width: i32, d_height: i32, config: &RingWorldConfig) -> Option<Self> {
        let ring = (self.ring_index as i32 + d_ring).rem_euclid(config.chunks_circumference as i32) as u32;
        let width = self.width_index as i32 + d_width;
        let height = self.height_index as i32 + d_height;

        if width < 0 || width >= config.chunks_width as i32 {
            return None; // Edge of the ring
        }
        if height < 0 || height >= config.chunks_height as i32 {
            return None;
        }

        Some(Self::new(ring, width as u32, height as u32))
    }
}

/// Compute the local-to-world transform for a chunk.
///
/// The ring lies in the XZ plane. The ring's axis is along Y.
/// For a chunk at angle theta on the ring:
/// - The chunk's local X axis should be tangent to the ring (perpendicular to radial, in XZ plane)
/// - The chunk's local Y axis should point radially INWARD (toward center/sun) = "up" for player
/// - The chunk's local Z axis should be along the ring's axis (Y in world space)
///
/// In the chunk's local space:
/// - x: [0, chunk_size] = along ring circumference (tangent)
/// - y: [0, chunk_size] = height above surface (toward sun)
/// - z: [0, chunk_size] = along ring width (axial)
pub fn chunk_transform(coord: &ChunkCoord, config: &RingWorldConfig) -> Matrix4<f32> {
    let pos = coord.to_ring_position(config);
    let theta = pos.theta;
    
    // The radial direction at this theta (pointing outward from center)
    let cos_t = theta.cos() as f32;
    let sin_t = theta.sin() as f32;
    
    // Radial outward direction in XZ plane
    let _radial_out = Vector3::new(cos_t, 0.0, sin_t);
    // Radial inward = toward sun = local "up" for the player
    let radial_in = Vector3::new(-cos_t, 0.0, -sin_t);
    
    // Tangent direction (perpendicular to radial, in XZ plane) = local X
    // We use (sin(theta), 0, -cos(theta)) to maintain right-handed coordinate system
    // This ensures positive determinant so backface culling works correctly
    let tangent = Vector3::new(sin_t, 0.0, -cos_t);
    
    // Axial direction = world Y axis = local Z for the chunk
    let axial = Vector3::new(0.0, 1.0, 0.0);
    
    // Scale: each voxel in the chunk maps to world units
    // Local X spans the angular arc of the chunk at the chunk's radial position
    // At height h above surface, the effective radius is (radius - h), so arc length
    // per voxel = (radius - h) * d_theta where d_theta = chunk_angular_size / chunk_size
    // This corrects the stretching that occurs because voxels at different heights
    // subtend different arc lengths.
    let chunk_center_height = pos.height + config.chunk_height_size() * 0.5;
    let effective_radius = config.radius - chunk_center_height;
    let chunk_voxel_size_tangent = (config.chunk_angular_size() * effective_radius / config.chunk_size as f64) as f32;
    let chunk_voxel_size_height = (config.chunk_height_size() / config.chunk_size as f64) as f32;
    let chunk_voxel_size_width = (config.chunk_width_size() / config.chunk_size as f64) as f32;
    
    // Position of chunk origin in world space:
    // The chunk sits at radius R (inner surface), offset by height
    // Position = center + radial_out * (R - height) + axial * y_offset
    let r = config.radius - pos.height;
    let world_x = (r * theta.cos()) as f32;
    let world_z = (r * theta.sin()) as f32;
    let world_y = pos.y as f32;
    
    // Build the transform matrix:
    // Column 0 (local X -> world): tangent * voxel_size_tangent
    // Column 1 (local Y -> world): radial_in * voxel_size_height (Y up = toward sun)
    // Column 2 (local Z -> world): axial * voxel_size_width
    // Column 3 (translation): world position
    #[rustfmt::skip]
    let transform = Matrix4::new(
        tangent.x * chunk_voxel_size_tangent, tangent.y * chunk_voxel_size_tangent, tangent.z * chunk_voxel_size_tangent, 0.0,
        radial_in.x * chunk_voxel_size_height, radial_in.y * chunk_voxel_size_height, radial_in.z * chunk_voxel_size_height, 0.0,
        axial.x * chunk_voxel_size_width, axial.y * chunk_voxel_size_width, axial.z * chunk_voxel_size_width, 0.0,
        world_x, world_y, world_z, 1.0,
    );
    
    transform
}

/// Exact curved mapping from a chunk-local mesh position to world space.
///
/// This is the geometry-side counterpart of the ANGULAR voxel grid that
/// collision / raycast / spawn already use (`is_position_solid` maps theta
/// uniformly in ANGLE). The old rendering path approximated each chunk as a
/// FLAT box via a single linear `chunk_transform`, whose tangent scale was
/// picked at the chunk's center height. A linear matrix cannot represent the
/// polar mapping, so the flat box diverged from the collision grid by up to
/// ~0.2 blocks at ring-chunk boundaries at the surface (visible GAPS/seams)
/// and ~0.4+ blocks across height-chunk boundaries: the player fell through
/// blocks they could see, stood on invisible ground, and appeared to spawn
/// inside terrain even though the collision-side spawn logic was correct.
///
/// This function instead maps EVERY mesh vertex through the exact ring
/// equation. The global voxel angle is computed from
/// `ring_index * chunk_size + local_x` so a boundary vertex shared by two
/// adjacent chunks evaluates to a bit-identical f64 angle in both -> the ring
/// closes seamlessly with no cracks.
///
/// Local axes (same convention as `chunk_transform`):
/// - local x: along ring circumference (theta)
/// - local y: height above surface (radial, toward sun)
/// - local z: along ring width (axial)
pub fn curved_local_to_world(coord: &ChunkCoord, local: [f32; 3], config: &RingWorldConfig) -> [f32; 3] {
    let cs = config.chunk_size as f64;
    let voxel_dtheta = config.chunk_angular_size() / cs;
    let h_per_voxel = config.chunk_height_size() / cs;
    let w_per_voxel = config.chunk_width_size() / cs;

    // Global voxel-grid coordinates (f64, integer-exact at voxel corners).
    let gx = coord.ring_index as f64 * cs + local[0] as f64;
    let gh = coord.height_index as f64 * cs + local[1] as f64;
    let gw = coord.width_index as f64 * cs + local[2] as f64;

    let theta = gx * voxel_dtheta;
    let height = gh * h_per_voxel;
    let y = -config.width / 2.0 + gw * w_per_voxel;

    let r = config.radius - height;
    [(r * theta.cos()) as f32, y as f32, (r * theta.sin()) as f32]
}

/// World-space direction of a chunk-local normal at the given local-x (theta)
/// position, for curved meshes produced with [`curved_local_to_world`].
///
/// The local frame at angle theta is the orthonormal right-handed basis
/// (tangent, radial_in, axial), a pure rotation, so normals stay unit-length
/// and no inverse-transpose is needed.
pub fn curved_normal(coord: &ChunkCoord, local_x: f32, normal: [f32; 3], config: &RingWorldConfig) -> [f32; 3] {
    let cs = config.chunk_size as f64;
    let voxel_dtheta = config.chunk_angular_size() / cs;
    let theta = (coord.ring_index as f64 * cs + local_x as f64) * voxel_dtheta;
    let (sin_t, cos_t) = (theta.sin() as f32, theta.cos() as f32);

    // local x -> TRUE tangent dP/dtheta = (-sin, 0, cos) (NOT chunk_transform's
    // mirrored (sin, 0, -cos), which pointed ring-direction normals backwards),
    // local y -> radial_in (toward sun), local z -> axial.
    let (nx, ny, nz) = (normal[0], normal[1], normal[2]);
    [
        nx * (-sin_t) + ny * (-cos_t),
        nz,
        nx * cos_t + ny * (-sin_t),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn approx_eq(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn ring_position_round_trip() {
        let config = RingWorldConfig::default();
        // Pick a position well away from boundaries so theta doesn't wrap ambiguously
        let original = RingPosition::new(1.0, 12.5, 5.0);
        let cart = original.to_cartesian(&config);
        let recovered = RingPosition::from_cartesian(cart, &config);

        assert!(approx_eq(original.theta, recovered.theta, 1e-6), "theta {} vs {}", original.theta, recovered.theta);
        assert!(approx_eq(original.y, recovered.y, 1e-6), "y {} vs {}", original.y, recovered.y);
        assert!(approx_eq(original.height, recovered.height, 1e-6), "height {} vs {}", original.height, recovered.height);
    }

    #[test]
    fn ring_position_round_trip_multiple_angles() {
        let config = RingWorldConfig::default();
        for &theta in &[0.1, 0.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0] {
            let original = RingPosition::new(theta, 0.0, 2.0);
            let cart = original.to_cartesian(&config);
            let recovered = RingPosition::from_cartesian(cart, &config);
            assert!(approx_eq(original.theta, recovered.theta, 1e-6), "theta {} vs {}", original.theta, recovered.theta);
            assert!(approx_eq(original.height, recovered.height, 1e-6), "height {} vs {}", original.height, recovered.height);
        }
    }

    #[test]
    fn normalize_theta_wraps_into_range() {
        let mut p = RingPosition::new(2.0 * PI + 1.0, 0.0, 0.0);
        p.normalize_theta();
        assert!(p.theta >= 0.0 && p.theta < 2.0 * PI);
        assert!(approx_eq(p.theta, 1.0, 1e-9));
    }

    #[test]
    fn normalize_theta_handles_negative() {
        let mut p = RingPosition::new(-1.0, 0.0, 0.0);
        p.normalize_theta();
        assert!(p.theta >= 0.0 && p.theta < 2.0 * PI);
        assert!(approx_eq(p.theta, 2.0 * PI - 1.0, 1e-9));
    }

    #[test]
    fn normalize_theta_at_two_pi_wraps_to_zero() {
        let mut p = RingPosition::new(2.0 * PI, 0.0, 0.0);
        p.normalize_theta();
        assert!(p.theta >= 0.0 && p.theta < 2.0 * PI);
        assert!(approx_eq(p.theta, 0.0, 1e-9));
    }

    #[test]
    fn from_cartesian_theta_zero_at_positive_x() {
        let config = RingWorldConfig::default();
        // A point directly along +X axis should map to theta = 0
        let pos = RingPosition::from_cartesian(Vector3::new(config.radius, 0.0, 0.0), &config);
        assert!(approx_eq(pos.theta, 0.0, 1e-9));
        assert!(approx_eq(pos.height, 0.0, 1e-6));
    }

    #[test]
    fn from_cartesian_normalizes_negative_theta_to_positive_range() {
        let config = RingWorldConfig::default();
        // A point with negative z gives a negative atan2, which should be wrapped
        let pos = RingPosition::from_cartesian(Vector3::new(config.radius, 0.0, -1.0), &config);
        assert!(pos.theta >= 0.0 && pos.theta < 2.0 * PI);
    }

    #[test]
    fn chunk_coord_from_ring_position_origin() {
        let config = RingWorldConfig::default();
        let pos = RingPosition::new(0.0, -config.width / 2.0, 0.0);
        let coord = ChunkCoord::from_ring_position(&pos, &config);
        assert_eq!(coord.ring_index, 0);
        assert_eq!(coord.width_index, 0);
        assert_eq!(coord.height_index, 0);
    }

    #[test]
    fn chunk_coord_from_ring_position_maps_indices() {
        let config = RingWorldConfig::default();
        // Place position at the start of chunk index 5 around the ring
        let theta = 5.0 * config.chunk_angular_size() + config.chunk_angular_size() * 0.5;
        let pos = RingPosition::new(theta, 0.0, 0.0);
        let coord = ChunkCoord::from_ring_position(&pos, &config);
        assert_eq!(coord.ring_index, 5);
    }

    #[test]
    fn chunk_coord_from_ring_position_clamps_indices() {
        let config = RingWorldConfig::default();
        // Position at far edge of width / max height should clamp within bounds
        let pos = RingPosition::new(0.0, config.width / 2.0, config.max_height);
        let coord = ChunkCoord::from_ring_position(&pos, &config);
        assert!(coord.width_index < config.chunks_width);
        assert!(coord.height_index < config.chunks_height);
    }

    #[test]
    fn chunk_coord_to_and_from_ring_position_consistent() {
        let config = RingWorldConfig::default();
        let coord = ChunkCoord::new(3, 2, 1);
        let pos = coord.to_ring_position(&config);
        let recovered = ChunkCoord::from_ring_position(&pos, &config);
        assert_eq!(coord, recovered);
    }

    #[test]
    fn neighbor_wraps_ring_index_around() {
        let config = RingWorldConfig::default();
        let coord = ChunkCoord::new(0, 5, 1);
        // Going -1 in ring index from 0 should wrap to chunks_circumference - 1
        let n = coord.neighbor(-1, 0, 0, &config).expect("ring wraps, should be Some");
        assert_eq!(n.ring_index, config.chunks_circumference - 1);
        assert_eq!(n.width_index, 5);
        assert_eq!(n.height_index, 1);
    }

    #[test]
    fn neighbor_wraps_ring_index_at_top() {
        let config = RingWorldConfig::default();
        let coord = ChunkCoord::new(config.chunks_circumference - 1, 0, 0);
        let n = coord.neighbor(1, 0, 0, &config).expect("ring wraps, should be Some");
        assert_eq!(n.ring_index, 0);
    }

    #[test]
    fn neighbor_returns_none_off_width_edge() {
        let config = RingWorldConfig::default();
        let coord = ChunkCoord::new(0, 0, 0);
        assert!(coord.neighbor(0, -1, 0, &config).is_none());
        let coord_top = ChunkCoord::new(0, config.chunks_width - 1, 0);
        assert!(coord_top.neighbor(0, 1, 0, &config).is_none());
    }

    #[test]
    fn neighbor_returns_none_off_height_edge() {
        let config = RingWorldConfig::default();
        let coord = ChunkCoord::new(0, 0, 0);
        assert!(coord.neighbor(0, 0, -1, &config).is_none());
        let coord_top = ChunkCoord::new(0, 0, config.chunks_height - 1);
        assert!(coord_top.neighbor(0, 0, 1, &config).is_none());
    }

    #[test]
    fn is_valid_bounds_check() {
        let config = RingWorldConfig::default();
        let valid = RingPosition::new(1.0, 0.0, 1.0);
        assert!(valid.is_valid(&config));
        let invalid_y = RingPosition::new(1.0, config.width, 1.0);
        assert!(!invalid_y.is_valid(&config));
        let invalid_h = RingPosition::new(1.0, 0.0, config.max_height + 10.0);
        assert!(!invalid_h.is_valid(&config));

        // Bug 6 regression: the height ceiling is EXCLUSIVE. A position at
        // exactly `max_height` has no containing voxel (the valid voxel grid is
        // 0..max_height), so it must be rejected. Just below the ceiling is OK.
        let at_ceiling = RingPosition::new(1.0, 0.0, config.max_height);
        assert!(!at_ceiling.is_valid(&config), "height == max_height must be invalid");
        let below_ceiling = RingPosition::new(1.0, 0.0, config.max_height - 0.5);
        assert!(below_ceiling.is_valid(&config));
        // The floor is inclusive.
        let at_floor = RingPosition::new(1.0, 0.0, 0.0);
        assert!(at_floor.is_valid(&config));
    }

    #[test]
    fn clamp_height_stays_strictly_below_ceiling() {
        // After clamping, a position must satisfy is_valid (height < max_height),
        // so clamp's ceiling is consistent with the tightened bounds check.
        let config = RingWorldConfig::default();
        let mut over = RingPosition::new(0.0, 0.0, config.max_height + 5.0);
        over.clamp(&config);
        assert!(over.height < config.max_height);
        assert!(over.is_valid(&config));
    }

    /// The local-to-world chunk transform must PRESERVE triangle winding for
    /// EVERY face, otherwise GPU back-face culling drops the faces whose
    /// screen-space winding got flipped. This is the root-cause guard for the
    /// "missing vertical side faces" bug: the four side faces (+X/-X tangent and
    /// +Z/-Z axial) wind opposite to the top/bottom faces under the ring's
    /// non-uniform / handedness transform unless the matrix has a positive
    /// determinant AND the mesher winding is consistent.
    ///
    /// We transform each local face's 4 CCW positions to world space, compute
    /// the world-space geometric normal from the winding, transform the local
    /// outward normal to world space, and assert they point the same way. If
    /// any face's geometric winding flips, its dot goes negative and the GPU
    /// would back-face-cull it -> invisible side face.
    #[test]
    fn chunk_transform_preserves_face_winding_all_directions() {
        let config = RingWorldConfig::default();
        // Local CCW quads (matching Face::greedy_positions winding) + outward
        // normal for each of the 6 cube faces of a unit voxel at the origin.
        let unit = 1.0f32;
        let faces: [([[f32; 3]; 4], [f32; 3]); 6] = [
            // +X
            ([[unit, 0.0, 0.0], [unit, unit, 0.0], [unit, unit, unit], [unit, 0.0, unit]],
             [1.0, 0.0, 0.0]),
            // -X
            ([[0.0, 0.0, unit], [0.0, unit, unit], [0.0, unit, 0.0], [0.0, 0.0, 0.0]],
             [-1.0, 0.0, 0.0]),
            // +Y
            ([[0.0, unit, 0.0], [0.0, unit, unit], [unit, unit, unit], [unit, unit, 0.0]],
             [0.0, 1.0, 0.0]),
            // -Y
            ([[unit, 0.0, 0.0], [unit, 0.0, unit], [0.0, 0.0, unit], [0.0, 0.0, 0.0]],
             [0.0, -1.0, 0.0]),
            // +Z
            ([[unit, 0.0, unit], [unit, unit, unit], [0.0, unit, unit], [0.0, 0.0, unit]],
             [0.0, 0.0, 1.0]),
            // -Z
            ([[0.0, 0.0, 0.0], [0.0, unit, 0.0], [unit, unit, 0.0], [unit, 0.0, 0.0]],
             [0.0, 0.0, -1.0]),
        ];

        // Exhaustively test EVERY ring index (at multiple heights) so we catch
        // any theta/height-dependent winding flip ANYWHERE on the ring.
        let mut coords = Vec::new();
        for ring in 0..config.chunks_circumference {
            for h in 0..config.chunks_height {
                coords.push(ChunkCoord::new(ring, 8, h));
            }
        }

        for coord in coords {
            let m = chunk_transform(&coord, &config);
            for (idx, (local_pos, local_normal)) in faces.iter().enumerate() {
                // Transform the 4 positions to world space.
                let wp: Vec<Vector3<f32>> = local_pos
                    .iter()
                    .map(|p| {
                        let v = cgmath::Vector4::new(p[0], p[1], p[2], 1.0);
                        let r = m * v;
                        Vector3::new(r.x, r.y, r.z)
                    })
                    .collect();
                // Geometric normal from the (now world-space) winding.
                let e1 = wp[1] - wp[0];
                let e2 = wp[2] - wp[0];
                let geo = cgmath::Vector3::new(
                    e1.y * e2.z - e1.z * e2.y,
                    e1.z * e2.x - e1.x * e2.z,
                    e1.x * e2.y - e1.y * e2.x,
                );
                // Transform the declared outward normal to world space (w=0).
                let nv = m * cgmath::Vector4::new(
                    local_normal[0], local_normal[1], local_normal[2], 0.0);
                let world_normal = Vector3::new(nv.x, nv.y, nv.z);
                let dot = geo.x * world_normal.x + geo.y * world_normal.y + geo.z * world_normal.z;
                assert!(
                    dot > 0.0,
                    "face {} at chunk {:?} flips winding under chunk_transform \
                     (geo·outward = {}); GPU would back-face-cull it",
                    idx, coord, dot
                );
            }
        }
    }

    /// Curved-mesh seam guard: a boundary vertex shared by two adjacent chunks
    /// must map to a BIT-IDENTICAL world position from both sides, on all
    /// three axes (ring — including the 255->0 wrap — height, and width).
    /// This is the regression test for the visible gaps/seams produced by the
    /// old flat per-chunk transform.
    #[test]
    fn curved_boundary_vertices_bit_identical_across_chunks() {
        let config = RingWorldConfig::default();
        let cs = config.chunk_size as f32;

        // Ring axis, including the wrap seam.
        for ring in [0u32, 7, 100, config.chunks_circumference - 1] {
            let a = ChunkCoord::new(ring, 3, 1);
            let b = ChunkCoord::new((ring + 1) % config.chunks_circumference, 3, 1);
            for &(ly, lz) in &[(0.0f32, 0.0f32), (5.0, 9.0), (16.0, 16.0)] {
                let pa = curved_local_to_world(&a, [cs, ly, lz], &config);
                let pb = curved_local_to_world(&b, [0.0, ly, lz], &config);
                let wrap = ring == config.chunks_circumference - 1;
                for i in 0..3 {
                    let d = (pa[i] - pb[i]).abs();
                    // Non-wrap boundaries must be bit-exact; the wrap seam
                    // (theta 2*PI vs 0) is allowed f64 rounding noise.
                    let tol = if wrap { 1e-3 } else { 0.0 };
                    assert!(d <= tol, "ring seam at {} axis {}: {} vs {}", ring, i, pa[i], pb[i]);
                }
            }
        }

        // Height axis (stacked chunks) — where the old code diverged worst.
        let a = ChunkCoord::new(42, 3, 0);
        let b = ChunkCoord::new(42, 3, 1);
        for &(lx, lz) in &[(0.0f32, 0.0f32), (16.0, 4.0), (11.0, 16.0)] {
            let pa = curved_local_to_world(&a, [lx, cs, lz], &config);
            let pb = curved_local_to_world(&b, [lx, 0.0, lz], &config);
            assert_eq!(pa, pb, "height seam at ({}, {})", lx, lz);
        }

        // Width axis.
        let a = ChunkCoord::new(42, 3, 1);
        let b = ChunkCoord::new(42, 4, 1);
        for &(lx, ly) in &[(0.0f32, 0.0f32), (16.0, 16.0), (8.0, 2.0)] {
            let pa = curved_local_to_world(&a, [lx, ly, cs], &config);
            let pb = curved_local_to_world(&b, [lx, ly, 0.0], &config);
            assert_eq!(pa, pb, "width seam at ({}, {})", lx, ly);
        }
    }

    /// The curved mesh mapping must agree with the ANGULAR voxel grid that
    /// collision / raycast / spawn use (RingPosition::to_cartesian), so what
    /// you see is exactly what you collide with.
    #[test]
    fn curved_mapping_matches_collision_grid() {
        let config = RingWorldConfig::default();
        let cs = config.chunk_size as f64;
        for &(ring, w, h) in &[(0u32, 0u32, 0u32), (5, 3, 1), (200, 15, 3)] {
            let coord = ChunkCoord::new(ring, w, h);
            for &(lx, ly, lz) in &[(0.0f64, 0.0, 0.0), (7.0, 3.0, 12.0), (16.0, 16.0, 16.0)] {
                let ring_pos = RingPosition::new(
                    (ring as f64 * cs + lx) * (config.chunk_angular_size() / cs),
                    -config.width / 2.0 + (w as f64 * cs + lz) * (config.chunk_width_size() / cs),
                    (h as f64 * cs + ly) * (config.chunk_height_size() / cs),
                );
                let expect = ring_pos.to_cartesian(&config);
                let got = curved_local_to_world(&coord, [lx as f32, ly as f32, lz as f32], &config);
                assert!((got[0] as f64 - expect.x).abs() < 1e-3, "x {} vs {}", got[0], expect.x);
                assert!((got[1] as f64 - expect.y).abs() < 1e-3, "y {} vs {}", got[1], expect.y);
                assert!((got[2] as f64 - expect.z).abs() < 1e-3, "z {} vs {}", got[2], expect.z);
            }
        }
    }

    /// The exact curved mapping is a REFLECTION of chunk-local space (the true
    /// ring tangent frame is left-handed), so raw CCW mesh winding flips for
    /// EVERY face at EVERY ring index: the geometric normal from the raw corner
    /// order must point OPPOSITE the outward `curved_normal`. `curve_mesh_data`
    /// compensates by reversing each triangle's index order (verified at the
    /// mesh level in chunk::tests::curved_mesh_triangles_wind_ccw_outward).
    /// This test pins the reflection property itself, so an accidental
    /// double-flip (or a future "fix" that removes the reversal) fails loudly.
    #[test]
    fn curved_mapping_reflects_raw_winding_all_directions() {
        let config = RingWorldConfig::default();
        let unit = 1.0f32;
        let faces: [([[f32; 3]; 4], [f32; 3]); 6] = [
            ([[unit, 0.0, 0.0], [unit, unit, 0.0], [unit, unit, unit], [unit, 0.0, unit]], [1.0, 0.0, 0.0]),
            ([[0.0, 0.0, unit], [0.0, unit, unit], [0.0, unit, 0.0], [0.0, 0.0, 0.0]], [-1.0, 0.0, 0.0]),
            ([[0.0, unit, 0.0], [0.0, unit, unit], [unit, unit, unit], [unit, unit, 0.0]], [0.0, 1.0, 0.0]),
            ([[unit, 0.0, 0.0], [unit, 0.0, unit], [0.0, 0.0, unit], [0.0, 0.0, 0.0]], [0.0, -1.0, 0.0]),
            ([[unit, 0.0, unit], [unit, unit, unit], [0.0, unit, unit], [0.0, 0.0, unit]], [0.0, 0.0, 1.0]),
            ([[0.0, 0.0, 0.0], [0.0, unit, 0.0], [unit, unit, 0.0], [unit, 0.0, 0.0]], [0.0, 0.0, -1.0]),
        ];

        for ring in 0..config.chunks_circumference {
            for h in 0..config.chunks_height {
                let coord = ChunkCoord::new(ring, 8, h);
                for (idx, (local_pos, local_normal)) in faces.iter().enumerate() {
                    let wp: Vec<[f32; 3]> = local_pos
                        .iter()
                        .map(|p| curved_local_to_world(&coord, *p, &config))
                        .collect();
                    let e1 = [wp[1][0] - wp[0][0], wp[1][1] - wp[0][1], wp[1][2] - wp[0][2]];
                    let e2 = [wp[2][0] - wp[0][0], wp[2][1] - wp[0][1], wp[2][2] - wp[0][2]];
                    let geo = [
                        e1[1] * e2[2] - e1[2] * e2[1],
                        e1[2] * e2[0] - e1[0] * e2[2],
                        e1[0] * e2[1] - e1[1] * e2[0],
                    ];
                    let wn = curved_normal(&coord, local_pos[0][0], *local_normal, &config);
                    let dot = geo[0] * wn[0] + geo[1] * wn[1] + geo[2] * wn[2];
                    assert!(
                        dot < 0.0,
                        "face {} at chunk {:?}: raw winding unexpectedly matches the outward \
                         normal (dot = {}); the curved frame reflection changed - revisit \
                         curve_mesh_data's triangle index reversal",
                        idx, coord, dot
                    );
                }
            }
        }
    }

    /// Curved normals must stay unit-length and match the local frame at the
    /// vertex's own theta.
    #[test]
    fn curved_normal_is_unit_and_radial_up_points_inward() {
        let config = RingWorldConfig::default();
        for ring in [0u32, 31, 64, 128, 200, 255] {
            let coord = ChunkCoord::new(ring, 8, 0);
            for lx in [0.0f32, 8.0, 16.0] {
                let up = curved_normal(&coord, lx, [0.0, 1.0, 0.0], &config);
                let len = (up[0] * up[0] + up[1] * up[1] + up[2] * up[2]).sqrt();
                assert!((len - 1.0).abs() < 1e-5);
                // "Up" (radial-in) must point from the vertex toward the ring
                // axis: dot(up, -position_xz) > 0.
                let p = curved_local_to_world(&coord, [lx, 0.0, 8.0], &config);
                let dot = up[0] * (-p[0]) + up[2] * (-p[2]);
                assert!(dot > 0.0, "up at ring {} lx {} points outward", ring, lx);
            }
        }
    }

    #[test]
    fn clamp_keeps_position_in_bounds() {
        let config = RingWorldConfig::default();
        let mut p = RingPosition::new(2.0 * PI + 0.5, config.width, config.max_height + 100.0);
        p.clamp(&config);
        assert!(p.theta >= 0.0 && p.theta < 2.0 * PI);
        assert!(p.y <= config.width / 2.0 && p.y >= -config.width / 2.0);
        assert!(p.height >= 0.0 && p.height <= config.max_height);
    }
}
