//! Đăng ký phím tắt toàn cục.

use darkshot_core::config::{Hotkey, HotkeyRole, Hotkeys};
use darkshot_core::i18n::HelpKeys;
pub use darkshot_core::settings_form::describe;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
    MOD_SHIFT, MOD_WIN,
};

/// Mã đăng ký với Windows của một dòng: Region = 1, SaveFull = 2, UploadFull = 3, RepeatLast = 4.
pub fn id_of(role: HotkeyRole) -> i32 {
    match role {
        HotkeyRole::Region => 1,
        HotkeyRole::SaveFull => 2,
        HotkeyRole::UploadFull => 3,
        HotkeyRole::RepeatLast => 4,
    }
}

/// Dòng ứng với mã `id` của `WM_HOTKEY`.
pub fn role_of(id: i32) -> Option<HotkeyRole> {
    HotkeyRole::ALL.into_iter().find(|r| id_of(*r) == id)
}

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

/// Tổ hợp của các dòng đang bật cho Trợ giúp; dòng tắt là `None`.
pub fn help_keys(keys: &Hotkeys) -> HelpKeys {
    let on = |r: HotkeyRole| {
        let slot = keys.get(r);
        slot.enabled.then(|| describe(&slot.key))
    };
    HelpKeys {
        region: on(HotkeyRole::Region),
        save_full: on(HotkeyRole::SaveFull),
        upload_full: on(HotkeyRole::UploadFull),
        repeat_last: on(HotkeyRole::RepeatLast),
    }
}

/// Tên tổ hợp của các dòng `failed`, theo thứ tự `HotkeyRole::ALL`.
pub fn taken_combos(keys: &Hotkeys, failed: &[HotkeyRole]) -> Vec<String> {
    HotkeyRole::ALL
        .into_iter()
        .filter(|r| failed.contains(r))
        .map(|r| describe(&keys.get(r).key))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        assert_eq!(id_of(HotkeyRole::Region), 1);
        let ids: Vec<i32> = HotkeyRole::ALL.iter().map(|r| id_of(*r)).collect();
        assert_eq!(ids, vec![1, 2, 3, 4]);
        for r in HotkeyRole::ALL {
            assert_eq!(role_of(id_of(r)), Some(r));
        }
        assert_eq!(role_of(0), None);
        assert_eq!(role_of(5), None);
    }

    #[test]
    fn help_keys_skip_disabled_rows() {
        let mut h = Hotkeys::default();
        assert_eq!(
            help_keys(&h),
            HelpKeys {
                region: Some("PrintScreen".into()),
                save_full: None,
                upload_full: None,
                repeat_last: None
            }
        );
        h.upload_full.enabled = true;
        h.region.enabled = false;
        assert_eq!(
            help_keys(&h),
            HelpKeys {
                region: None,
                save_full: None,
                upload_full: Some("Ctrl+PrintScreen".into()),
                repeat_last: None
            }
        );
    }

    #[test]
    fn taken_combos_follow_row_order() {
        let h = Hotkeys::default();
        assert_eq!(
            taken_combos(&h, &[HotkeyRole::UploadFull, HotkeyRole::Region]),
            vec!["PrintScreen".to_string(), "Ctrl+PrintScreen".to_string()]
        );
        assert!(taken_combos(&h, &[]).is_empty());
    }
}
