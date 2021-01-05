use pilka_lib::winit::event::{ElementState, VirtualKeyCode};

#[derive(Debug, Default)]
pub struct Input {
    pub up_pressed: bool,
    pub down_pressed: bool,
    pub right_pressed: bool,
    pub left_pressed: bool,
    pub slash_pressed: bool,
    pub right_shift_pressed: bool,
    pub enter_pressed: bool,
    pub space_pressed: bool,
}

impl Input {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn update(&mut self, key: &VirtualKeyCode, state: &ElementState) -> bool {
        let pressed = state == &ElementState::Pressed;
        match key {
            VirtualKeyCode::Up => {
                self.up_pressed = pressed;
                true
            }
            VirtualKeyCode::Down => {
                self.down_pressed = pressed;
                true
            }
            VirtualKeyCode::Left => {
                self.left_pressed = pressed;
                true
            }
            VirtualKeyCode::Right => {
                self.right_pressed = pressed;
                true
            }
            VirtualKeyCode::Slash => {
                self.slash_pressed = pressed;
                true
            }
            VirtualKeyCode::RShift => {
                self.right_shift_pressed = pressed;
                true
            }
            VirtualKeyCode::Return => {
                self.enter_pressed = pressed;
                true
            }
            VirtualKeyCode::Space => {
                self.space_pressed = pressed;
                true
            }
            _ => false,
        }
    }
}
