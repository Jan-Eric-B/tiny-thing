//! What each input on the Device does.

use crate::{DeviceInput, TouchControl};
use protocol::InputAction;

/// The action a physical input stands for: the dial press and button 2 play
/// or pause, buttons 1 and 3 go to the previous and the next song, and
/// turning the dial steps the Volume up (clockwise) or down.
pub fn device_input_action(input: DeviceInput) -> InputAction {
    match input {
        DeviceInput::DialPress | DeviceInput::Button2 => InputAction::PlayPause,
        DeviceInput::Button1 => InputAction::Previous,
        DeviceInput::Button3 => InputAction::Next,
        DeviceInput::DialClockwise => InputAction::VolumeUp,
        DeviceInput::DialCounterClockwise => InputAction::VolumeDown,
    }
}

/// The action an on-screen control stands for.
pub fn touch_control_action(control: TouchControl) -> InputAction {
    match control {
        TouchControl::Previous => InputAction::Previous,
        TouchControl::PlayPause => InputAction::PlayPause,
        TouchControl::Next => InputAction::Next,
    }
}
