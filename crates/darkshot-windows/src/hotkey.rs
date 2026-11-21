//! Đăng ký phím tắt toàn cục.

use darkshot_core::config::{Hotkey, HotkeyRole, Hotkeys};
// Luật phím tắt nằm ở core; giữ tên cũ cho `windows_main.rs` và `settings.rs`.
pub use darkshot_core::hotkey_keys::{describe, help_keys, id_of, role_of};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
    MOD_SHIFT, MOD_WIN,
};

/// Hủy đăng ký mọi dòng; dòng chưa đăng ký thì bỏ qua.
pub fn unregister_all(hwnd: HWND) {
    for r in HotkeyRole::ALL {
        // SAFETY: hủy phím đã đăng ký cho cửa sổ này; lỗi (chưa đăng ký) được bỏ qua.
        unsafe {
            let _ = UnregisterHotKey(Some(hwnd), id_of(r));
        }
    }
}

/// Hủy mọi dòng rồi đăng ký các dòng đang bật theo `HotkeyRole::ALL`; dòng tắt không bao giờ
/// được đăng ký. Trả về các dòng đang bật mà không đăng ký được.
pub fn register_enabled(hwnd: HWND, keys: &Hotkeys) -> Vec<HotkeyRole> {
    // RegisterHotKey với cùng hwnd và id không thay phím cũ mà giữ cả hai: luôn hủy trước.
    unregister_all(hwnd);
    HotkeyRole::ALL
        .into_iter()
        .filter(|r| {
            let slot = keys.get(*r);
            slot.enabled && !register(hwnd, *r, &slot.key)
        })
        .collect()
}

/// RegisterHotKey (MOD_NOREPEAT) cho dòng `role`. Trả về `false` khi phím đang bị ứng dụng khác chiếm.
pub fn register(hwnd: HWND, role: HotkeyRole, hk: &Hotkey) -> bool {
    let mut mods: HOT_KEY_MODIFIERS = MOD_NOREPEAT;
    if hk.ctrl {
        mods |= MOD_CONTROL;
    }
    if hk.alt {
        mods |= MOD_ALT;
    }
    if hk.shift {
        mods |= MOD_SHIFT;
    }
    if hk.win {
        mods |= MOD_WIN;
    }
    // SAFETY: hwnd là cửa sổ của luồng gọi; các tham số là giá trị thuần.
    unsafe { RegisterHotKey(Some(hwnd), id_of(role), mods, hk.vk).is_ok() }
}
