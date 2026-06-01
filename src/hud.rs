/// HUD module - renders 2D overlay elements (crosshair, health bar, hotbar, etc.)

use wgpu::util::DeviceExt;
use crate::voxel::{VoxelType, FaceDir};

/// Vertex type for HUD elements (position in NDC + color)
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct HudVertex {
    position: [f32; 2],
    color: [f32; 4],
}

impl HudVertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<HudVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// Data needed to render the HUD each frame
pub struct HudRenderData {
    pub health: f32,
    pub max_health: f32,
    pub hotbar: [VoxelType; 9],
    pub hotbar_index: usize,
    pub debug_visible: bool,
    pub target_in_reach: bool,
    /// Debug overlay text lines (rendered top-left when `debug_visible`).
    pub debug_lines: Vec<String>,
}

/// HUD rendering state
pub struct Hud {
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    num_vertices: u32,
    /// Separate vertex buffer for debug text glyph quads (same pipeline).
    text_vertex_buffer: wgpu::Buffer,
    num_text_vertices: u32,
    /// Cached screen dimensions for rebuilding HUD
    screen_width: u32,
    screen_height: u32,
    /// Maximum buffer size (in vertices) to avoid reallocation
    max_buffer_vertices: u32,
    /// Maximum size of the text vertex buffer (in vertices).
    max_text_vertices: u32,
}

impl Hud {
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
        // Create HUD shader
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("HUD Shader"),
            source: wgpu::ShaderSource::Wgsl(HUD_SHADER.into()),
        });

        // Pipeline layout (no bind groups needed for simple HUD)
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("HUD Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        // Render pipeline - no depth test, alpha blending
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("HUD Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[HudVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None, // No depth test for HUD
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        // Create initial vertex buffer with enough space for all HUD elements
        let max_buffer_vertices = 2048u32;
        let initial_data = vec![HudVertex { position: [0.0, 0.0], color: [0.0; 4] }; max_buffer_vertices as usize];
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("HUD Vertex Buffer"),
            contents: bytemuck::cast_slice(&initial_data),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        // Separate buffer for debug text glyph quads. Each glyph "on" pixel is a
        // quad (6 vertices); a screen of debug text can need a lot of them, so
        // allocate generously.
        let max_text_vertices = 262144u32;
        let text_initial = vec![HudVertex { position: [0.0, 0.0], color: [0.0; 4] }; max_text_vertices as usize];
        let text_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("HUD Text Vertex Buffer"),
            contents: bytemuck::cast_slice(&text_initial),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            render_pipeline,
            vertex_buffer,
            num_vertices: 0,
            text_vertex_buffer,
            num_text_vertices: 0,
            screen_width: width,
            screen_height: height,
            max_buffer_vertices,
            max_text_vertices,
        }
    }

    /// Create crosshair vertices in NDC coordinates
    /// Crosshair is ~20px total size, 2px thick lines
    fn create_crosshair_vertices(width: u32, height: u32, color: [f32; 4]) -> Vec<HudVertex> {
        let mut vertices = Vec::new();

        // Convert pixel sizes to NDC
        let px_x = 2.0 / width as f32;
        let px_y = 2.0 / height as f32;

        let half_length = 10.0; // 10px from center = 20px total
        let half_thickness = 1.0; // 1px from center = 2px thick

        // Horizontal bar (centered at origin in NDC)
        let h_left = -half_length * px_x;
        let h_right = half_length * px_x;
        let h_top = half_thickness * px_y;
        let h_bottom = -half_thickness * px_y;

        // Two triangles for horizontal bar
        vertices.push(HudVertex { position: [h_left, h_top], color });
        vertices.push(HudVertex { position: [h_left, h_bottom], color });
        vertices.push(HudVertex { position: [h_right, h_bottom], color });

        vertices.push(HudVertex { position: [h_left, h_top], color });
        vertices.push(HudVertex { position: [h_right, h_bottom], color });
        vertices.push(HudVertex { position: [h_right, h_top], color });

        // Vertical bar (centered at origin in NDC)
        let v_left = -half_thickness * px_x;
        let v_right = half_thickness * px_x;
        let v_top = half_length * px_y;
        let v_bottom = -half_length * px_y;

        // Two triangles for vertical bar
        vertices.push(HudVertex { position: [v_left, v_top], color });
        vertices.push(HudVertex { position: [v_left, v_bottom], color });
        vertices.push(HudVertex { position: [v_right, v_bottom], color });

        vertices.push(HudVertex { position: [v_left, v_top], color });
        vertices.push(HudVertex { position: [v_right, v_bottom], color });
        vertices.push(HudVertex { position: [v_right, v_top], color });

        vertices
    }

    /// Create health bar vertices (bottom-left of screen)
    /// Shows 10 heart-sized rectangles, filled based on health/max_health
    fn create_health_bar_vertices(width: u32, height: u32, health: f32, max_health: f32) -> Vec<HudVertex> {
        let mut vertices = Vec::new();

        let px_x = 2.0 / width as f32;
        let px_y = 2.0 / height as f32;

        // Position: bottom-left, 10px from edges
        let start_x = -1.0 + 10.0 * px_x;
        let start_y = -1.0 + 10.0 * px_y;

        let heart_width = 16.0 * px_x;
        let heart_height = 16.0 * px_y;
        let heart_spacing = 2.0 * px_x;

        let filled_hearts = (health / max_health * 10.0).ceil() as usize;
        let health_fraction = health / max_health;

        for i in 0..10 {
            let x = start_x + i as f32 * (heart_width + heart_spacing);
            let y = start_y;

            // Background (dark red)
            let bg_color = [0.3, 0.0, 0.0, 0.6];
            push_quad(&mut vertices, x, y, heart_width, heart_height, bg_color);

            // Filled portion (bright red)
            if i < filled_hearts {
                let fill_color = if i as f32 / 10.0 < health_fraction {
                    [0.9, 0.1, 0.1, 0.9]
                } else {
                    [0.5, 0.1, 0.1, 0.7]
                };
                // Slightly inset
                let inset = 2.0 * px_x;
                let inset_y = 2.0 * px_y;
                push_quad(&mut vertices, x + inset, y + inset_y,
                    heart_width - 2.0 * inset, heart_height - 2.0 * inset_y, fill_color);
            }
        }

        vertices
    }

    /// Create hunger bar vertices (next to health bar)
    /// Always full for now (placeholder)
    fn create_hunger_bar_vertices(width: u32, height: u32) -> Vec<HudVertex> {
        let mut vertices = Vec::new();

        let px_x = 2.0 / width as f32;
        let px_y = 2.0 / height as f32;

        // Position: bottom-left, above health bar
        let start_x = -1.0 + 10.0 * px_x;
        let start_y = -1.0 + 32.0 * px_y; // Above health bar

        let piece_width = 16.0 * px_x;
        let piece_height = 16.0 * px_y;
        let piece_spacing = 2.0 * px_x;

        for i in 0..10 {
            let x = start_x + i as f32 * (piece_width + piece_spacing);
            let y = start_y;

            // Background (dark brown)
            let bg_color = [0.2, 0.1, 0.0, 0.6];
            push_quad(&mut vertices, x, y, piece_width, piece_height, bg_color);

            // Filled (orange/brown - always full)
            let fill_color = [0.8, 0.5, 0.1, 0.9];
            let inset = 2.0 * px_x;
            let inset_y = 2.0 * px_y;
            push_quad(&mut vertices, x + inset, y + inset_y,
                piece_width - 2.0 * inset, piece_height - 2.0 * inset_y, fill_color);
        }

        vertices
    }

    /// Create hotbar display vertices (bottom-center of screen)
    /// 9 squares with the selected slot highlighted
    fn create_hotbar_vertices(width: u32, height: u32, hotbar: &[VoxelType; 9], selected: usize) -> Vec<HudVertex> {
        let mut vertices = Vec::new();

        let px_x = 2.0 / width as f32;
        let px_y = 2.0 / height as f32;

        let slot_size = 36.0; // pixels
        let slot_spacing = 4.0; // pixels
        let total_width = 9.0 * slot_size + 8.0 * slot_spacing;

        // Center horizontally, at bottom
        let start_x = -(total_width * 0.5) * px_x;
        let start_y = -1.0 + 10.0 * px_y;

        let slot_w = slot_size * px_x;
        let slot_h = slot_size * px_y;
        let spacing = slot_spacing * px_x;

        for i in 0..9 {
            let x = start_x + i as f32 * (slot_w + spacing);
            let y = start_y;

            // Slot background
            let bg_color = if i == selected {
                [0.6, 0.6, 0.6, 0.8] // Highlighted slot
            } else {
                [0.2, 0.2, 0.2, 0.7] // Normal slot
            };
            push_quad(&mut vertices, x, y, slot_w, slot_h, bg_color);

            // Slot border
            let border_color = if i == selected {
                [1.0, 1.0, 1.0, 0.9] // Bright border for selected
            } else {
                [0.4, 0.4, 0.4, 0.6] // Dim border
            };
            let border_thickness = if i == selected { 3.0 } else { 1.0 };
            let bt = border_thickness * px_x;
            let bt_y = border_thickness * px_y;

            // Top border
            push_quad(&mut vertices, x, y + slot_h - bt_y, slot_w, bt_y, border_color);
            // Bottom border
            push_quad(&mut vertices, x, y, slot_w, bt_y, border_color);
            // Left border
            push_quad(&mut vertices, x, y, bt, slot_h, border_color);
            // Right border
            push_quad(&mut vertices, x + slot_w - bt, y, bt, slot_h, border_color);

            // Block color indicator (inner square showing the block type)
            let block_type = hotbar[i];
            if block_type != VoxelType::Air {
                let block_color = block_type.face_color(FaceDir::Top);
                let inset = 6.0 * px_x;
                let inset_y = 6.0 * px_y;
                push_quad(&mut vertices, x + inset, y + inset_y,
                    slot_w - 2.0 * inset, slot_h - 2.0 * inset_y, block_color);
            }
        }

        vertices
    }

    /// Update the entire HUD each frame with current game state
    pub fn update(&mut self, queue: &wgpu::Queue, data: &HudRenderData) {
        let mut all_vertices = Vec::new();

        // Crosshair
        let crosshair_color = if data.debug_visible {
            [0.0, 1.0, 0.0, 0.9] // Green when debug is on
        } else if data.target_in_reach {
            [1.0, 1.0, 1.0, 0.8] // White - block in reach
        } else {
            [0.5, 0.5, 0.5, 0.5] // Gray - nothing targeted
        };
        all_vertices.extend(Self::create_crosshair_vertices(
            self.screen_width, self.screen_height, crosshair_color));

        // Health bar
        all_vertices.extend(Self::create_health_bar_vertices(
            self.screen_width, self.screen_height, data.health, data.max_health));

        // Hunger bar
        all_vertices.extend(Self::create_hunger_bar_vertices(
            self.screen_width, self.screen_height));

        // Hotbar
        all_vertices.extend(Self::create_hotbar_vertices(
            self.screen_width, self.screen_height, &data.hotbar, data.hotbar_index));

        // 3x pixel scale, glyphs are 5x7 with 1px spacing => 6px advance.
        let scale = 3.0f32;
        let glyph_h = GLYPH_HEIGHT as f32 * scale;
        let line_spacing = 4.0f32 * scale;
        let margin = 6.0f32;

        // ---- Debug text background panel (drawn with the main HUD quads, i.e.
        // BEFORE the glyph quads) so the F3 overlay stays readable over bright
        // terrain. We size the panel to the text block bounds.
        if data.debug_visible && !data.debug_lines.is_empty() {
            // Width = longest line in glyph advances; height = lines * line box.
            let max_chars = data
                .debug_lines
                .iter()
                .map(|l| l.chars().count())
                .max()
                .unwrap_or(0) as f32;
            let advance = (GLYPH_WIDTH as f32 + 1.0) * scale;
            let pad = 4.0f32;
            let panel_w_px = max_chars * advance + pad * 2.0;
            let panel_h_px =
                data.debug_lines.len() as f32 * (glyph_h + line_spacing) + pad * 2.0;

            let px_x = 2.0 / self.screen_width as f32;
            let px_y = 2.0 / self.screen_height as f32;

            // Top-left corner in NDC (margin from the top-left of the screen).
            let left_px = margin - pad;
            let top_px = margin - pad;
            let ndc_left = left_px * px_x - 1.0;
            let ndc_top = 1.0 - top_px * px_y;
            let w = panel_w_px * px_x;
            let h = panel_h_px * px_y;
            // push_quad takes the bottom-left corner and grows up by h.
            // Near-opaque dark panel so bright terrain does not bleed through and
            // wash out the text (this was the main F3 readability problem).
            let panel_color = [0.05, 0.05, 0.08, 0.92];
            push_quad(&mut all_vertices, ndc_left, ndc_top - h, w, h, panel_color);
        }

        // Write to buffer (truncate if exceeds max)
        let vertex_count = all_vertices.len().min(self.max_buffer_vertices as usize);
        self.num_vertices = vertex_count as u32;

        if vertex_count > 0 {
            queue.write_buffer(
                &self.vertex_buffer,
                0,
                bytemuck::cast_slice(&all_vertices[..vertex_count]),
            );
        }

        // ---- Debug text overlay (top-left) ----
        // Crisp white glyphs over the near-opaque dark panel pushed above. The
        // previous drop-shadow pass doubled the glyph quad count (risking text
        // buffer overflow / truncation) and softened the text; with a solid
        // panel behind it, a single high-contrast pass is far more readable.
        let mut text_vertices: Vec<HudVertex> = Vec::new();
        if data.debug_visible && !data.debug_lines.is_empty() {
            let text_color = [1.0, 1.0, 1.0, 1.0];

            let mut cursor_y = margin;
            for line in &data.debug_lines {
                Self::push_text(
                    &mut text_vertices,
                    self.screen_width,
                    self.screen_height,
                    line,
                    margin,
                    cursor_y,
                    scale,
                    text_color,
                );
                cursor_y += glyph_h + line_spacing;
            }
        }

        let text_count = text_vertices.len().min(self.max_text_vertices as usize);
        self.num_text_vertices = text_count as u32;
        if text_count > 0 {
            queue.write_buffer(
                &self.text_vertex_buffer,
                0,
                bytemuck::cast_slice(&text_vertices[..text_count]),
            );
        }
    }

    /// Append the glyph quads for `text` rendered at pixel position (px, py)
    /// (top-left origin) with the given scale and color into `vertices`.
    fn push_text(
        vertices: &mut Vec<HudVertex>,
        screen_width: u32,
        screen_height: u32,
        text: &str,
        px: f32,
        py: f32,
        scale: f32,
        color: [f32; 4],
    ) {
        let px_per_pixel_x = 2.0 / screen_width as f32;
        let px_per_pixel_y = 2.0 / screen_height as f32;

        let mut cursor_x = px;
        for ch in text.chars() {
            let glyph = glyph_bits(ch);
            // For each "on" bit, emit a small filled rectangle (1 scaled pixel).
            for row in 0..GLYPH_HEIGHT {
                let bits = glyph[row];
                for col in 0..GLYPH_WIDTH {
                    // Most-significant bit is the left-most column.
                    let mask = 1u8 << (GLYPH_WIDTH - 1 - col);
                    if bits & mask == 0 {
                        continue;
                    }
                    // Pixel position of this glyph cell (top-left origin).
                    let cell_px = cursor_x + col as f32 * scale;
                    let cell_py = py + row as f32 * scale;
                    // Convert to NDC. NDC y is flipped (top = +1).
                    let ndc_x = cell_px * px_per_pixel_x - 1.0;
                    let ndc_y = 1.0 - cell_py * px_per_pixel_y;
                    let w = scale * px_per_pixel_x;
                    let h = scale * px_per_pixel_y;
                    // push_quad expects (x, y) as the bottom-left and grows up by h.
                    // Our NDC y is the top of the cell, so the bottom is ndc_y - h.
                    push_quad(vertices, ndc_x, ndc_y - h, w, h, color);
                }
            }
            // Advance cursor by glyph width + 1px spacing.
            cursor_x += (GLYPH_WIDTH as f32 + 1.0) * scale;
        }
    }

    /// Update HUD when window is resized
    pub fn resize(&mut self, _device: &wgpu::Device, width: u32, height: u32) {
        self.screen_width = width;
        self.screen_height = height;
    }

    /// Legacy method for backward compatibility - now handled by update()
    pub fn update_crosshair_color(&mut self, _queue: &wgpu::Queue, _target_in_reach: bool) {
        // Now handled by the unified update() method
    }

    /// Render the HUD overlay
    pub fn render<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        render_pass.set_pipeline(&self.render_pipeline);
        // Main HUD elements (crosshair, health, hotbar).
        if self.num_vertices > 0 {
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.draw(0..self.num_vertices, 0..1);
        }
        // Debug text glyph quads (reuse the same pipeline).
        if self.num_text_vertices > 0 {
            render_pass.set_vertex_buffer(0, self.text_vertex_buffer.slice(..));
            render_pass.draw(0..self.num_text_vertices, 0..1);
        }
    }
}

/// Helper: push a quad (2 triangles, 6 vertices) into the vertex list
fn push_quad(vertices: &mut Vec<HudVertex>, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
    // Bottom-left, bottom-right, top-right, top-left
    vertices.push(HudVertex { position: [x, y + h], color });
    vertices.push(HudVertex { position: [x, y], color });
    vertices.push(HudVertex { position: [x + w, y], color });

    vertices.push(HudVertex { position: [x, y + h], color });
    vertices.push(HudVertex { position: [x + w, y], color });
    vertices.push(HudVertex { position: [x + w, y + h], color });
}

/// WGSL shader for HUD rendering
const HUD_SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(in.position, 0.0, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

// =====================================================================
// Built-in 5x7 bitmap font.
//
// BIT CONVENTION (documented + worked example below):
//   Each glyph is 5 pixels wide and 7 pixels tall, stored as 7 row bytes,
//   row 0 at the TOP. Within a row byte the LOW 5 bits encode the columns:
//
//       bit 4 = column 0 (LEFT-most)
//       bit 3 = column 1
//       bit 2 = column 2
//       bit 1 = column 3
//       bit 0 = column 4 (RIGHT-most)
//       bits 7..5 = unused (always 0)
//
//   So the value 0b10001 (0x11) means "pixels at the far-left AND far-right
//   of that row". A set bit ("1") is a filled pixel.
//
//   This convention EXACTLY matches how push_text() reads the data:
//       let mask = 1 << (GLYPH_WIDTH - 1 - col);   // col 0 -> bit 4
//   The previous font was authored using bits 7..3 (e.g. 0x70) while the
//   reader used bits 4..0, shifting every glyph 3 columns to the right and
//   discarding bits -> illegible mush. Every glyph below is now <= 0x1F.
//
//   WORKED EXAMPLE -- the letter 'A' (`.` = off, `#` = on):
//       row 0:  . # # # .   = 0b01110 = 0x0E
//       row 1:  # . . . #   = 0b10001 = 0x11
//       row 2:  # . . . #   = 0b10001 = 0x11
//       row 3:  # # # # #   = 0b11111 = 0x1F
//       row 4:  # . . . #   = 0b10001 = 0x11
//       row 5:  # . . . #   = 0b10001 = 0x11
//       row 6:  # . . . #   = 0b10001 = 0x11
//   => ['A'] = [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11]
//
// Supported: space, 0-9, A-Z, a-z (lowercase mapped to uppercase shapes),
// and the punctuation actually used by the debug overlay: : . , / - + = ( ) %
// plus < > ? _ . Unknown characters render as a small box so they stay visible.
// =====================================================================

/// Glyph cell width in pixels.
pub const GLYPH_WIDTH: usize = 5;
/// Glyph cell height in pixels.
pub const GLYPH_HEIGHT: usize = 7;

/// Return the 7-row bitmap for a character. Each row uses the LOW 5 bits,
/// with bit 4 = left-most column (see the convention block above).
fn glyph_bits(c: char) -> [u8; GLYPH_HEIGHT] {
    // Lowercase letters reuse the uppercase glyph shapes for simplicity
    // (this is documented; we do not claim distinct lowercase glyphs).
    let c = if c.is_ascii_lowercase() {
        c.to_ascii_uppercase()
    } else {
        c
    };
    match c {
        ' ' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],

        // Digits.
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        '3' => [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],

        // Uppercase letters.
        'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'D' => [0x1C, 0x12, 0x11, 0x11, 0x11, 0x12, 0x1C],
        'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        'G' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
        'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'I' => [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        'J' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        'M' => [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        'S' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
        'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x1B, 0x11],
        'X' => [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04],
        'Z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],

        // Punctuation used by the debug overlay.
        ':' => [0x00, 0x06, 0x06, 0x00, 0x06, 0x06, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x06],
        ',' => [0x00, 0x00, 0x00, 0x00, 0x06, 0x06, 0x04],
        '/' => [0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10],
        '-' => [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00],
        '+' => [0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00],
        '=' => [0x00, 0x00, 0x1F, 0x00, 0x1F, 0x00, 0x00],
        '(' => [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02],
        ')' => [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08],
        '<' => [0x02, 0x04, 0x08, 0x10, 0x08, 0x04, 0x02],
        '>' => [0x08, 0x04, 0x02, 0x01, 0x02, 0x04, 0x08],
        '%' => [0x19, 0x19, 0x02, 0x04, 0x08, 0x13, 0x13],
        '?' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04],
        '_' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1F],

        // Unknown / unsupported: small box so it is still visible.
        _ => [0x00, 0x0E, 0x0A, 0x0A, 0x0A, 0x0E, 0x00],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plot a glyph into a 5-wide x 7-tall boolean grid using the SAME bit
    /// convention as push_text(): bit 4 = left-most column (col 0).
    fn plot(c: char) -> [[bool; GLYPH_WIDTH]; GLYPH_HEIGHT] {
        let g = glyph_bits(c);
        let mut grid = [[false; GLYPH_WIDTH]; GLYPH_HEIGHT];
        for row in 0..GLYPH_HEIGHT {
            for col in 0..GLYPH_WIDTH {
                let mask = 1u8 << (GLYPH_WIDTH - 1 - col);
                grid[row][col] = g[row] & mask != 0;
            }
        }
        grid
    }

    /// Render a `&[&str]` of 5-char rows ('#'/'.') into the expected grid.
    fn expect(rows: [&str; GLYPH_HEIGHT]) -> [[bool; GLYPH_WIDTH]; GLYPH_HEIGHT] {
        let mut grid = [[false; GLYPH_WIDTH]; GLYPH_HEIGHT];
        for (r, line) in rows.iter().enumerate() {
            let chars: Vec<char> = line.chars().collect();
            assert_eq!(chars.len(), GLYPH_WIDTH, "row {:?} must be 5 chars", line);
            for (col, ch) in chars.iter().enumerate() {
                grid[r][col] = *ch == '#';
            }
        }
        grid
    }

    #[test]
    fn glyph_dimensions_are_constant() {
        assert_eq!(GLYPH_WIDTH, 5);
        assert_eq!(GLYPH_HEIGHT, 7);
        let g = glyph_bits('A');
        assert_eq!(g.len(), GLYPH_HEIGHT);
    }

    #[test]
    fn space_glyph_is_blank() {
        assert_eq!(glyph_bits(' '), [0u8; GLYPH_HEIGHT]);
    }

    #[test]
    fn all_glyph_rows_fit_in_five_bits() {
        // The reader only inspects bits 4..0; any glyph with a bit set above
        // bit 4 was authored with the wrong (garbled) convention.
        for c in " 0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ:.,/-+=()<>%?_".chars() {
            let g = glyph_bits(c);
            for (r, &row) in g.iter().enumerate() {
                assert!(
                    row <= 0x1F,
                    "glyph {:?} row {} = {:#04x} uses bits above bit 4",
                    c, r, row
                );
            }
        }
    }

    #[test]
    fn known_glyphs_have_set_pixels() {
        for c in "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ:./-".chars() {
            let g = glyph_bits(c);
            assert!(g.iter().any(|&row| row != 0), "glyph for {:?} is blank", c);
        }
    }

    #[test]
    fn lowercase_maps_to_uppercase_shape() {
        assert_eq!(glyph_bits('a'), glyph_bits('A'));
        assert_eq!(glyph_bits('z'), glyph_bits('Z'));
    }

    // ---- Hand-plotted glyph verification: these lock the font so it can never
    // silently regress into garbage again. ----

    #[test]
    fn glyph_a_matches_hand_plot() {
        assert_eq!(
            plot('A'),
            expect([
                ".###.",
                "#...#",
                "#...#",
                "#####",
                "#...#",
                "#...#",
                "#...#",
            ])
        );
    }

    #[test]
    fn glyph_e_matches_hand_plot() {
        assert_eq!(
            plot('E'),
            expect([
                "#####",
                "#....",
                "#....",
                "####.",
                "#....",
                "#....",
                "#####",
            ])
        );
    }

    #[test]
    fn glyph_zero_matches_hand_plot() {
        assert_eq!(
            plot('0'),
            expect([
                ".###.",
                "#...#",
                "#..##",
                "#.#.#",
                "##..#",
                "#...#",
                ".###.",
            ])
        );
    }

    #[test]
    fn glyph_one_matches_hand_plot() {
        assert_eq!(
            plot('1'),
            expect([
                "..#..",
                ".##..",
                "..#..",
                "..#..",
                "..#..",
                "..#..",
                ".###.",
            ])
        );
    }

    #[test]
    fn glyph_eight_matches_hand_plot() {
        assert_eq!(
            plot('8'),
            expect([
                ".###.",
                "#...#",
                "#...#",
                ".###.",
                "#...#",
                "#...#",
                ".###.",
            ])
        );
    }

    #[test]
    fn glyph_colon_matches_hand_plot() {
        assert_eq!(
            plot(':'),
            expect([
                ".....",
                "..##.",
                "..##.",
                ".....",
                "..##.",
                "..##.",
                ".....",
            ])
        );
    }

    #[test]
    fn glyph_period_matches_hand_plot() {
        assert_eq!(
            plot('.'),
            expect([
                ".....",
                ".....",
                ".....",
                ".....",
                ".....",
                "..##.",
                "..##.",
            ])
        );
    }

    #[test]
    fn glyph_slash_matches_hand_plot() {
        assert_eq!(
            plot('/'),
            expect([
                "....#",
                "....#",
                "...#.",
                "..#..",
                ".#...",
                "#....",
                "#....",
            ])
        );
    }

    #[test]
    fn push_text_emits_sane_quad_count_and_no_overlap() {
        // 6 vertices per "on" pixel; advance must be >= glyph width so cells
        // never overlap into mush.
        let scale = 3.0f32;
        let mut v: Vec<HudVertex> = Vec::new();
        Hud::push_text(&mut v, 800, 600, "ABC", 10.0, 10.0, scale, [1.0, 1.0, 1.0, 1.0]);

        // Count "on" pixels in A, B, C and expect 6 vertices each.
        let on_pixels: usize = "ABC"
            .chars()
            .map(|c| {
                glyph_bits(c)
                    .iter()
                    .map(|row| (row & 0x1F).count_ones() as usize)
                    .sum::<usize>()
            })
            .sum();
        assert!(on_pixels > 0);
        assert_eq!(v.len(), on_pixels * 6, "expected 6 vertices per lit pixel");

        // Advance per glyph (6 * scale px) must be >= glyph pixel width (5 *
        // scale px), guaranteeing adjacent glyph cells do not overlap.
        let advance = (GLYPH_WIDTH as f32 + 1.0) * scale;
        let glyph_width_px = GLYPH_WIDTH as f32 * scale;
        assert!(advance >= glyph_width_px, "glyph cells would overlap");
    }
}
