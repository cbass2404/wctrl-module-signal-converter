//! The keyboard's inputs, as distinct from any panel's.
//!
//! A panel's report says nothing about the keyboard: an MCDU key pressed with
//! Ctrl held sends the same bytes as without. So what the keyboard holds is
//! asked of Windows on its own, and belongs to the keyboard, not to any panel
//! it is used with. Which modifier means what is a setting, so the type lives
//! with the settings in `dsc-config`; asking Windows about it lives here.

use dsc_config::settings::Modifier;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};

/// Whether a modifier is down on the keyboard right now, whichever window has
/// focus. Asked at the moment a panel key goes down, so nothing polls it.
///
/// Left and right are one key: the generic virtual keys answer for either
/// side, which is the point, as nobody should have to remember which Ctrl.
pub fn held(m: Modifier) -> bool {
    let key = match m {
        Modifier::Ctrl => VK_CONTROL,
        Modifier::Shift => VK_SHIFT,
        Modifier::Alt => VK_MENU,
    };
    // The high bit is the key's state now; the low bit is only whether it
    // was pressed since some earlier call, which is no use here.
    (unsafe { GetAsyncKeyState(key as i32) }) < 0
}

/// Every modifier down right now.
pub fn held_now() -> Vec<Modifier> {
    Modifier::ALL.into_iter().filter(|m| held(*m)).collect()
}
