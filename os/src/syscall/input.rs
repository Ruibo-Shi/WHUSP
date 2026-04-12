use crate::drivers::{KEYBOARD_DEVICE, MOUSE_DEVICE};

pub fn sys_event_get() -> isize {
    if let Some(kb) = KEYBOARD_DEVICE.as_ref() {
        if !kb.is_empty() {
            return kb.read_event() as isize;
        }
    }
    if let Some(mouse) = MOUSE_DEVICE.as_ref() {
        if !mouse.is_empty() {
            return mouse.read_event() as isize;
        }
    }
    0
}

use crate::drivers::chardev::UART;

/// check UART's read-buffer is empty or not
pub fn sys_key_pressed() -> isize {
    let res = !UART.read_buffer_is_empty();
    if res { 1 } else { 0 }
}
