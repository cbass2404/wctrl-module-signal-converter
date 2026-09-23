//! The keyboard's inputs, as distinct from any panel's.
//!
//! A panel's report says nothing about the keyboard: an MCDU key pressed with
//! Ctrl held sends the same bytes as without. So what the keyboard holds is
//! asked of Windows on its own, and belongs to the keyboard, not to any panel
//! it is used with.

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};

/// A keyboard key held with a panel's key to mean something else.
///
/// Left and right are one key: the generic virtual keys answer for either
/// side, which is the point, as nobody should have to remember which Ctrl.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modifier {
    Ctrl,
    Shift,
    Alt,
}

impl Modifier {
    pub const ALL: [Modifier; 3] = [Modifier::Ctrl, Modifier::Shift, Modifier::Alt];

    pub fn name(self) -> &'static str {
        match self {
            Modifier::Ctrl => "Ctrl",
            Modifier::Shift => "Shift",
            Modifier::Alt => "Alt",
        }
    }

    /// Whether it is down on the keyboard right now, whichever window has
    /// focus. Asked at the moment a panel key goes down, so nothing polls it.
    pub fn held(self) -> bool {
        let key = match self {
            Modifier::Ctrl => VK_CONTROL,
            Modifier::Shift => VK_SHIFT,
            Modifier::Alt => VK_MENU,
        };
        // The high bit is the key's state now; the low bit is only whether it
        // was pressed since some earlier call, which is no use here.
        (unsafe { GetAsyncKeyState(key as i32) }) < 0
    }
}
