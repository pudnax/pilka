use super::PushConstant;
use winit::{
    event::{ElementState, KeyEvent, RawKeyEvent},
    keyboard::{KeyCode, PhysicalKey},
};

#[derive(Debug, Default)]
pub struct Input {
    pub move_forward: bool,
    pub move_backward: bool,
    pub move_right: bool,
    pub move_left: bool,
    pub move_up: bool,
    pub move_down: bool,
}

impl Input {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn update_window_input(&mut self, key_event: &KeyEvent) {
        let pressed = key_event.state == ElementState::Pressed;
        if let PhysicalKey::Code(key) = key_event.physical_key {
            match key {
                KeyCode::KeyA => self.move_left = pressed,
                KeyCode::KeyD => self.move_right = pressed,
                KeyCode::KeyS => self.move_backward = pressed,
                KeyCode::KeyW => self.move_forward = pressed,
                KeyCode::Period | KeyCode::KeyQ => self.move_down = pressed,
                KeyCode::Slash | KeyCode::KeyE => self.move_up = pressed,
                _ => {}
            }
        }
    }

    pub fn update_device_input(&mut self, key_event: RawKeyEvent) {
        let pressed = key_event.state == ElementState::Pressed;
        if let PhysicalKey::Code(key) = key_event.physical_key {
            match key {
                KeyCode::ArrowLeft => self.move_left = pressed,
                KeyCode::ArrowRight => self.move_right = pressed,
                KeyCode::ArrowDown => self.move_backward = pressed,
                KeyCode::ArrowUp => self.move_forward = pressed,
                _ => {}
            }
        }
    }

    pub fn process_position(&self, push_constant: &mut PushConstant) {
        let dx = 0.01;
        if self.move_left {
            push_constant.pos[0] -= dx;
        }
        if self.move_right {
            push_constant.pos[0] += dx;
        }
        if self.move_backward {
            push_constant.pos[1] -= dx;
        }
        if self.move_forward {
            push_constant.pos[1] += dx;
        }
        if self.move_down {
            push_constant.pos[2] -= dx;
        }
        if self.move_up {
            push_constant.pos[2] += dx;
        }
    }
}
