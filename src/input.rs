/// Input handling module

use winit::event::{ElementState, MouseButton};
use winit::keyboard::KeyCode;

/// Tracks the current input state
pub struct InputState {
    pub keys_pressed: std::collections::HashSet<KeyCode>,
    pub mouse_buttons: std::collections::HashSet<MouseButton>,
    pub mouse_delta: (f64, f64),
    pub mouse_position: (f64, f64),
    pub cursor_locked: bool,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            keys_pressed: std::collections::HashSet::new(),
            mouse_buttons: std::collections::HashSet::new(),
            mouse_delta: (0.0, 0.0),
            mouse_position: (0.0, 0.0),
            cursor_locked: false,
        }
    }

    pub fn process_key(&mut self, key: KeyCode, state: ElementState) {
        match state {
            ElementState::Pressed => {
                self.keys_pressed.insert(key);
            }
            ElementState::Released => {
                self.keys_pressed.remove(&key);
            }
        }
    }

    pub fn process_mouse_button(&mut self, button: MouseButton, state: ElementState) {
        match state {
            ElementState::Pressed => {
                self.mouse_buttons.insert(button);
            }
            ElementState::Released => {
                self.mouse_buttons.remove(&button);
            }
        }
    }

    pub fn process_mouse_motion(&mut self, delta: (f64, f64)) {
        self.mouse_delta = delta;
    }

    pub fn is_key_pressed(&self, key: KeyCode) -> bool {
        self.keys_pressed.contains(&key)
    }

    pub fn reset_delta(&mut self) {
        self.mouse_delta = (0.0, 0.0);
    }
}
