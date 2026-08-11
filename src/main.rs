#![allow(dead_code)]

mod ring_world;
mod voxel;
mod block;
mod chunk;
mod renderer;
mod camera;
mod terrain;
mod structures;
mod sun;
mod shadow_squares;
mod sky;
mod input;
mod player;
mod distant_ring;
mod texture;
mod hud;
mod inventory;
mod entity;
mod crafting;
mod lighting;
mod audio;
mod effects;
mod persistence;

use std::sync::Arc;
use winit::{
    event::*,
    event_loop::EventLoop,
    window::{WindowBuilder, CursorGrabMode},
    keyboard::{KeyCode, PhysicalKey},
};
use pollster::block_on;
use crate::input::InputState;

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Ring World - Voxel Game")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720))
            .build(&event_loop)
            .unwrap(),
    );

    let mut state = block_on(renderer::State::new(window.clone()));
    let mut last_render_time = std::time::Instant::now();
    let mut input_state = InputState::new();

    event_loop.run(move |event, elwt| {
        match event {
            Event::DeviceEvent {
                event: DeviceEvent::MouseMotion { delta },
                ..
            } => {
                // Only process mouse motion for camera when cursor is locked and inventory is closed
                if input_state.cursor_locked && !state.player.inventory_open {
                    state.player.camera_controller.process_mouse(delta.0, delta.1);
                }
            }
            Event::WindowEvent {
                ref event,
                window_id,
            } if window_id == window.id() => {
                match event {
                    WindowEvent::CloseRequested => {
                        // Persist the session before quitting.
                        state.save_world();
                        elwt.exit();
                    }
                    WindowEvent::Focused(focused) => {
                        if *focused && !input_state.cursor_locked {
                            // Re-lock cursor on focus gain
                            lock_cursor(&window, &mut input_state);
                        }
                    }
                    WindowEvent::Resized(physical_size) => {
                        state.resize(*physical_size);
                    }
                    WindowEvent::KeyboardInput {
                        event: KeyEvent {
                            physical_key: PhysicalKey::Code(key),
                            state: key_state,
                            ..
                        },
                        ..
                    } => {
                        match (*key, *key_state) {
                            // Escape toggles cursor lock
                            (KeyCode::Escape, ElementState::Pressed) => {
                                if input_state.cursor_locked {
                                    unlock_cursor(&window, &mut input_state);
                                } else {
                                    lock_cursor(&window, &mut input_state);
                                }
                            }
                            // Jump / Fly up (Space key)
                            (KeyCode::Space, ElementState::Pressed) => {
                                if state.player.is_flying {
                                    state.player.fly_up = true;
                                } else {
                                    state.player.request_jump();
                                }
                            }
                            (KeyCode::Space, ElementState::Released) => {
                                state.player.fly_up = false;
                            }
                            // Sprint (Left Ctrl)
                            (KeyCode::ControlLeft, ElementState::Pressed) => {
                                state.player.set_sprinting(true);
                            }
                            (KeyCode::ControlLeft, ElementState::Released) => {
                                state.player.set_sprinting(false);
                            }
                            // Crouch / Fly down (Left Shift)
                            (KeyCode::ShiftLeft, ElementState::Pressed) => {
                                if state.player.is_flying {
                                    state.player.fly_down = true;
                                } else {
                                    state.player.set_crouching(true);
                                }
                            }
                            (KeyCode::ShiftLeft, ElementState::Released) => {
                                state.player.fly_down = false;
                                state.player.set_crouching(false);
                            }
                            // Toggle inventory (E key)
                            (KeyCode::KeyE, ElementState::Pressed) => {
                                state.player.toggle_inventory();
                            }
                            // Toggle creative mode (F1)
                            (KeyCode::F1, ElementState::Pressed) => {
                                state.player.toggle_creative_mode();
                            }
                            // Toggle debug overlay (F3)
                            (KeyCode::F3, ElementState::Pressed) => {
                                state.debug_visible = !state.debug_visible;
                            }
                            // Toggle frustum culling (F4) - for A/B testing rendering
                            (KeyCode::F4, ElementState::Pressed) => {
                                state.enable_frustum_cull = !state.enable_frustum_cull;
                            }
                            // Toggle occlusion culling (F5) - for A/B testing rendering
                            (KeyCode::F5, ElementState::Pressed) => {
                                state.enable_occlusion_cull = !state.enable_occlusion_cull;
                            }
                            // Toggle render-diagnostic mode (F6): disables all
                            // culling, forces full-bright per-face normal tints,
                            // no fog, no alpha discard. Helps distinguish missing
                            // geometry vs. bad texture vs. culling bug.
                            (KeyCode::F6, ElementState::Pressed) => {
                                state.debug_render = !state.debug_render;
                            }
                            // Toggle greedy meshing (F7): re-meshes ALL loaded
                            // chunks. With greedy OFF every block face is emitted
                            // as its own 1x1 quad (no merging), which is the A/B
                            // test for "is the greedy merge dropping faces?".
                            (KeyCode::F7, ElementState::Pressed) => {
                                state.toggle_greedy_mesh();
                            }
                            // Cycle day/night time scale (F8): 1x/20x/120x
                            (KeyCode::F8, ElementState::Pressed) => {
                                state.cycle_time_scale();
                            }
                            // Toggle borderless fullscreen (F11)
                            (KeyCode::F11, ElementState::Pressed) => {
                                if window.fullscreen().is_some() {
                                    window.set_fullscreen(None);
                                } else {
                                    window.set_fullscreen(Some(
                                        winit::window::Fullscreen::Borderless(None),
                                    ));
                                }
                            }
                            // Toggle flying (F key, creative mode only)
                            (KeyCode::KeyF, ElementState::Pressed) => {
                                state.player.toggle_flying();
                            }
                            // Toggle crafting (C key)
                            (KeyCode::KeyC, ElementState::Pressed) => {
                                state.player.toggle_crafting();
                            }
                            // Hotbar selection (1-9)
                            (KeyCode::Digit1, ElementState::Pressed) => {
                                state.player.select_hotbar_slot(0);
                            }
                            (KeyCode::Digit2, ElementState::Pressed) => {
                                state.player.select_hotbar_slot(1);
                            }
                            (KeyCode::Digit3, ElementState::Pressed) => {
                                state.player.select_hotbar_slot(2);
                            }
                            (KeyCode::Digit4, ElementState::Pressed) => {
                                state.player.select_hotbar_slot(3);
                            }
                            (KeyCode::Digit5, ElementState::Pressed) => {
                                state.player.select_hotbar_slot(4);
                            }
                            (KeyCode::Digit6, ElementState::Pressed) => {
                                state.player.select_hotbar_slot(5);
                            }
                            (KeyCode::Digit7, ElementState::Pressed) => {
                                state.player.select_hotbar_slot(6);
                            }
                            (KeyCode::Digit8, ElementState::Pressed) => {
                                state.player.select_hotbar_slot(7);
                            }
                            (KeyCode::Digit9, ElementState::Pressed) => {
                                state.player.select_hotbar_slot(8);
                            }
                            _ => {}
                        }
                        // Forward to camera controller (WASD movement)
                        state.player.camera_controller.process_keyboard(*key, *key_state);
                    }
                    WindowEvent::MouseInput {
                        state: button_state,
                        button,
                        ..
                    } => {
                        if !input_state.cursor_locked {
                            // First click locks the cursor
                            if *button_state == ElementState::Pressed {
                                lock_cursor(&window, &mut input_state);
                            }
                        } else {
                            match (button, button_state) {
                                (MouseButton::Left, ElementState::Pressed) => {
                                    // Start continuous breaking
                                    state.player.left_mouse_held = true;
                                    // In creative mode, instant break on click
                                    if state.player.creative_mode {
                                        if !state.destroy_block() {
                                            // No block hit - try to attack an entity
                                            state.attack_entity();
                                        }
                                    } else {
                                        // In survival mode, try to attack entity if no block in reach
                                        if !state.player.target_in_reach {
                                            state.attack_entity();
                                        }
                                    }
                                }
                                (MouseButton::Left, ElementState::Released) => {
                                    // Stop breaking
                                    state.player.left_mouse_held = false;
                                    state.player.reset_breaking();
                                }
                                (MouseButton::Right, ElementState::Pressed) => {
                                    // Use player's selected block
                                    let block_type = state.player.selected_block;
                                    state.place_block(block_type);
                                }
                                _ => {}
                            }
                        }
                    }
                    WindowEvent::RedrawRequested => {
                        let now = std::time::Instant::now();
                        let dt = now - last_render_time;
                        last_render_time = now;
                        // Cap dt to prevent physics explosion on first frame or lag spikes
                        let dt = std::time::Duration::from_secs_f64(dt.as_secs_f64().min(0.05));
                        state.update(dt);
                        match state.render() {
                            Ok(_) => {}
                            Err(wgpu::SurfaceError::Lost) => state.resize(state.size),
                            Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                            Err(e) => eprintln!("{:?}", e),
                        }
                    }
                    _ => {}
                }
            }
            Event::AboutToWait => {
                window.request_redraw();
            }
            _ => {}
        }
    }).unwrap();
}

/// Lock/grab the cursor and hide it
fn lock_cursor(window: &winit::window::Window, input_state: &mut InputState) {
    // Try Confined first (more widely supported), fall back to Locked
    let result = window.set_cursor_grab(CursorGrabMode::Confined)
        .or_else(|_| window.set_cursor_grab(CursorGrabMode::Locked));
    if result.is_ok() {
        window.set_cursor_visible(false);
        input_state.cursor_locked = true;
    }
}

/// Release the cursor and show it
fn unlock_cursor(window: &winit::window::Window, input_state: &mut InputState) {
    let _ = window.set_cursor_grab(CursorGrabMode::None);
    window.set_cursor_visible(true);
    input_state.cursor_locked = false;
}
