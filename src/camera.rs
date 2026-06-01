/// Camera system for the ring world
/// The camera exists on the inner surface of the ring and looks "up" toward the sun
/// or along the ring surface. The local coordinate frame rotates with the player's
/// position on the ring.

use cgmath::{
    Deg, InnerSpace, Matrix4, Point3, Rad, Vector3, perspective, SquareMatrix,
};

/// Correction matrix mapping cgmath/OpenGL clip space (NDC z in [-1, 1]) to the
/// wgpu / Direct3D / Metal convention (NDC z in [0, 1]).
///
/// cgmath's `perspective()` builds an OpenGL-style projection whose z output
/// spans [-1, 1]. wgpu expects [0, 1]. Without this remap, every fragment that
/// projects into the NEAR HALF of the depth range (OpenGL z in [-1, 0]) is
/// clipped / depth-tested incorrectly, so roughly half of the visible geometry
/// silently disappears regardless of back-face culling (it stays missing even
/// in the F6 no-cull diagnostic). Pre-multiplying the projection by this matrix
/// squashes z into [0, 1] and fixes the "half the world is see-through / the
/// view doesn't track the camera correctly" artifact.
#[rustfmt::skip]
pub const OPENGL_TO_WGPU_MATRIX: Matrix4<f32> = Matrix4::new(
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 0.5, 0.0,
    0.0, 0.0, 0.5, 1.0,
);
use winit::event::ElementState;
use winit::keyboard::KeyCode;

/// Camera uniform data sent to the GPU
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub view_position: [f32; 4],
}

impl CameraUniform {
    pub fn new() -> Self {
        Self {
            view_proj: Matrix4::identity().into(),
            view_position: [0.0; 4],
        }
    }

    pub fn update_view_proj(&mut self, camera: &Camera, projection: &Projection) {
        self.view_position = [camera.position.x, camera.position.y, camera.position.z, 1.0];
        self.view_proj = (projection.calc_matrix() * camera.calc_matrix()).into();
    }
}

/// The camera that views the ring world
pub struct Camera {
    pub position: Point3<f32>,
    /// Yaw angle (rotation around local up axis)
    pub yaw: Rad<f32>,
    /// Pitch angle (rotation around local right axis)
    pub pitch: Rad<f32>,
    /// The "up" direction at the camera's current position (toward ring center)
    pub up: Vector3<f32>,
}

impl Camera {
    pub fn new<P: Into<Point3<f32>>>(position: P, yaw: Deg<f32>, pitch: Deg<f32>, up: Vector3<f32>) -> Self {
        Self {
            position: position.into(),
            yaw: yaw.into(),
            pitch: pitch.into(),
            up: up.normalize(),
        }
    }

    /// Calculate the view matrix using the local up vector
    pub fn calc_matrix(&self) -> Matrix4<f32> {
        let forward = self.forward();
        let target = self.position + forward;
        Matrix4::look_at_rh(self.position, target, self.up)
    }

    /// Get the forward direction vector based on yaw/pitch relative to local frame
    /// The local frame is defined by:
    /// - up = radially inward (toward sun)
    /// - We need a reference "forward" direction (tangent to ring)
    pub fn forward(&self) -> Vector3<f32> {
        // Build a local coordinate frame:
        // up = self.up (toward center)
        // We need a reference tangent direction. We can derive it from the position.
        // At position P on the ring (in XZ plane), tangent = normalize(up × world_Y) or similar
        // But since up might be in any direction, let's use a stable reference:
        
        // The ring axis is world Y. The radial direction is in XZ plane.
        // "forward" reference (tangent) = up × ring_axis_normalized
        // But up IS perpendicular to ring axis (since ring is in XZ plane, up is in XZ plane)
        // So tangent = up × Y_axis gives us the tangent direction
        
        let ring_axis = Vector3::new(0.0, 1.0, 0.0);
        let tangent = self.up.cross(ring_axis).normalize();
        // If up is parallel to Y (shouldn't happen on ring), fallback
        let tangent = if tangent.magnitude2() < 0.001 {
            Vector3::new(1.0, 0.0, 0.0)
        } else {
            tangent
        };
        // "right" in local frame = tangent (perpendicular to up and ring axis)
        // Actually let's define: 
        // local_forward_ref = tangent (along ring circumference)
        // local_right_ref = ring_axis direction (along width)
        // local_up = self.up (toward center)
        
        // Apply yaw rotation around up axis, then pitch rotation around right axis
        let (sin_yaw, cos_yaw) = self.yaw.0.sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch.0.sin_cos();
        
        // Forward after yaw (rotation around up): mix tangent and ring_axis
        let forward_horiz = tangent * cos_yaw + ring_axis * sin_yaw;
        
        // Apply pitch (tilt up/down relative to the surface)
        let forward = forward_horiz * cos_pitch + self.up * sin_pitch;
        
        forward.normalize()
    }

    /// Get the right direction vector
    pub fn right(&self) -> Vector3<f32> {
        self.forward().cross(self.up).normalize()
    }

    /// Smoothly interpolate the camera position toward a target world position.
    /// `factor` is the base lerp factor (e.g. 0.15) and is scaled by `60 * dt`
    /// to remain frame-rate independent.
    pub fn lerp_position(&mut self, target: Point3<f32>, factor: f32, dt: f32) {
        let t = (factor * 60.0 * dt).clamp(0.0, 1.0);
        self.position.x += (target.x - self.position.x) * t;
        self.position.y += (target.y - self.position.y) * t;
        self.position.z += (target.z - self.position.z) * t;
    }

    /// Update the up vector based on current position on the ring
    /// Up = direction toward ring center (origin) from current position
    pub fn update_up_from_position(&mut self) {
        // Ring is in XZ plane, center at origin
        // Radial direction = -normalize(position projected onto XZ)
        let radial = Vector3::new(self.position.x, 0.0, self.position.z);
        if radial.magnitude2() > 0.001 {
            self.up = (-radial).normalize(); // toward center
        }
    }
}

/// Projection settings
pub struct Projection {
    pub aspect: f32,
    pub fovy: Rad<f32>,
    pub znear: f32,
    pub zfar: f32,
}

impl Projection {
    pub fn new(width: u32, height: u32, fovy: Deg<f32>, znear: f32, zfar: f32) -> Self {
        Self {
            aspect: width as f32 / height as f32,
            fovy: fovy.into(),
            znear,
            zfar,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.aspect = width as f32 / height as f32;
    }

    pub fn calc_matrix(&self) -> Matrix4<f32> {
        // Remap cgmath's OpenGL [-1,1] clip-space z to wgpu's [0,1] so the near
        // half of the depth range is no longer clipped away.
        OPENGL_TO_WGPU_MATRIX * perspective(self.fovy, self.aspect, self.znear, self.zfar)
    }
}

/// Camera controller for FPS-style movement on the ring
pub struct CameraController {
    amount_left: f32,
    amount_right: f32,
    amount_forward: f32,
    amount_backward: f32,
    amount_up: f32,
    amount_down: f32,
    rotate_horizontal: f32,
    rotate_vertical: f32,
    speed: f32,
    sensitivity: f32,
}

impl CameraController {
    pub fn new(speed: f32, sensitivity: f32) -> Self {
        Self {
            amount_left: 0.0,
            amount_right: 0.0,
            amount_forward: 0.0,
            amount_backward: 0.0,
            amount_up: 0.0,
            amount_down: 0.0,
            rotate_horizontal: 0.0,
            rotate_vertical: 0.0,
            speed,
            sensitivity,
        }
    }

    pub fn process_keyboard(&mut self, key: KeyCode, state: ElementState) {
        let amount = if state == ElementState::Pressed {
            1.0
        } else {
            0.0
        };
        match key {
            KeyCode::KeyW | KeyCode::ArrowUp => self.amount_forward = amount,
            KeyCode::KeyS | KeyCode::ArrowDown => self.amount_backward = amount,
            KeyCode::KeyA | KeyCode::ArrowLeft => self.amount_left = amount,
            KeyCode::KeyD | KeyCode::ArrowRight => self.amount_right = amount,
            KeyCode::Space => self.amount_up = amount,
            KeyCode::ShiftLeft => self.amount_down = amount,
            _ => {}
        }
    }

    pub fn process_mouse(&mut self, mouse_dx: f64, mouse_dy: f64) {
        self.rotate_horizontal = mouse_dx as f32;
        self.rotate_vertical = mouse_dy as f32;
    }

    /// Get movement input as (forward, right, up) normalized amounts
    /// forward: positive = forward along yaw direction
    /// right: positive = right perpendicular to forward
    /// These are raw input values (-1 to 1), not scaled by speed or dt
    pub fn get_movement(&self) -> (f32, f32, f32) {
        let forward = self.amount_forward - self.amount_backward;
        let right = self.amount_right - self.amount_left;
        let up = self.amount_up - self.amount_down;
        (forward, right, up)
    }

    /// Update only the camera rotation (mouse look)
    pub fn update_rotation(&mut self, camera: &mut Camera, dt: std::time::Duration) {
        let dt = dt.as_secs_f32();

        // Rotation
        camera.yaw += Rad(self.rotate_horizontal) * self.sensitivity * dt;
        camera.pitch += Rad(-self.rotate_vertical) * self.sensitivity * dt;

        // Clamp pitch
        let max_pitch: f32 = std::f32::consts::FRAC_PI_2 - 0.01;
        camera.pitch = Rad(camera.pitch.0.clamp(-max_pitch, max_pitch));

        // Reset mouse delta
        self.rotate_horizontal = 0.0;
        self.rotate_vertical = 0.0;
    }

    /// Legacy: update both movement and rotation (for backward compat)
    pub fn update_camera(&mut self, camera: &mut Camera, dt: std::time::Duration) {
        let dt_f = dt.as_secs_f32();

        // Update up vector based on current position
        camera.update_up_from_position();

        // Get local frame vectors
        let up = camera.up;
        let ring_axis = Vector3::new(0.0, 1.0, 0.0);
        let tangent = up.cross(ring_axis).normalize();
        let tangent = if tangent.magnitude2() < 0.001 {
            Vector3::new(1.0, 0.0, 0.0)
        } else {
            tangent
        };

        // Movement along the ring surface
        let (sin_yaw, cos_yaw) = camera.yaw.0.sin_cos();
        let move_forward = tangent * cos_yaw + ring_axis * sin_yaw;
        let move_right = move_forward.cross(up).normalize();

        camera.position += move_forward * (self.amount_forward - self.amount_backward) * self.speed * dt_f;
        camera.position += move_right * (self.amount_right - self.amount_left) * self.speed * dt_f;
        camera.position += up * (self.amount_up - self.amount_down) * self.speed * dt_f;

        // Rotation
        camera.yaw += Rad(self.rotate_horizontal) * self.sensitivity * dt_f;
        camera.pitch += Rad(-self.rotate_vertical) * self.sensitivity * dt_f;

        // Clamp pitch
        let max_pitch: f32 = std::f32::consts::FRAC_PI_2 - 0.01;
        camera.pitch = Rad(camera.pitch.0.clamp(-max_pitch, max_pitch));

        // Reset mouse delta
        self.rotate_horizontal = 0.0;
        self.rotate_vertical = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cgmath::Vector4;

    /// The projection must map the camera-space near and far planes to wgpu's
    /// [0, 1] NDC z range (near -> 0, far -> 1), NOT OpenGL's [-1, 1]. Without
    /// the OPENGL_TO_WGPU correction the near plane mapped to -1, so the near
    /// half of the depth range was clipped and ~half the world rendered
    /// see-through (even with back-face culling off).
    #[test]
    fn projection_maps_depth_to_wgpu_zero_one_range() {
        let proj = Projection::new(800, 600, Deg(70.0), 0.1, 1000.0);
        let m = proj.calc_matrix();

        // A point on the near plane (camera looks down -Z in right-handed view
        // space) should map to NDC z ~= 0.
        let near = Vector4::new(0.0, 0.0, -0.1, 1.0);
        let clip_near = m * near;
        let ndc_near_z = clip_near.z / clip_near.w;
        assert!(
            ndc_near_z.abs() < 1e-3,
            "near plane NDC z should be ~0 (wgpu), got {}",
            ndc_near_z
        );

        // A far point should map toward NDC z ~= 1.
        let far = Vector4::new(0.0, 0.0, -1000.0, 1.0);
        let clip_far = m * far;
        let ndc_far_z = clip_far.z / clip_far.w;
        assert!(
            (ndc_far_z - 1.0).abs() < 1e-2,
            "far plane NDC z should be ~1 (wgpu), got {}",
            ndc_far_z
        );
    }
}
