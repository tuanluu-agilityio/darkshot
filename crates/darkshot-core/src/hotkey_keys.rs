//! Luật phím tắt dùng chung mọi nền tảng: mã đăng ký, tên tổ hợp, bắt phím.
//! `Hotkey.vk` là mã VK của Windows (quyết định 131); lớp Mac chỉ thêm bảng đổi mã.

use crate::config::{Hotkey, HotkeyRole, Hotkeys};
use crate::i18n::HelpKeys;

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

/// "PrintScreen", "Ctrl+Shift+S"...
pub fn describe(hk: &Hotkey) -> String {
    let mut s = String::new();
    for (on, name) in [
        (hk.ctrl, "Ctrl"),
        (hk.alt, "Alt"),
        (hk.shift, "Shift"),
        (hk.win, "Win"),
    ] {
        if on {
            s.push_str(name);
            s.push('+');
        }
    }
    s.push_str(&key_name(hk.vk));
    s
}

fn key_name(vk: u32) -> String {
    match vk {
        0x2C => "PrintScreen".into(),
        0x08 => "Backspace".into(),
        0x09 => "Tab".into(),
        0x0D => "Enter".into(),
        0x13 => "Pause".into(),
        0x1B => "Esc".into(),
        0x20 => "Space".into(),
        0x21 => "PageUp".into(),
        0x22 => "PageDown".into(),
        0x23 => "End".into(),
        0x24 => "Home".into(),
        0x25 => "Left".into(),
        0x26 => "Up".into(),
        0x27 => "Right".into(),
        0x28 => "Down".into(),
        0x2D => "Insert".into(),
        0x2E => "Delete".into(),
        0x30..=0x39 | 0x41..=0x5A => char::from(vk as u8).to_string(),
        0x70..=0x87 => format!("F{}", vk - 0x70 + 1),
        _ => format!("0x{vk:02X}"),
    }
}

/// Kết quả xử lý một lần nhả phím khi đang chọn phím tắt mới.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capture {
    /// Phím bổ trợ đứng một mình: chờ tiếp.
    Ignore,
    /// Phím thường cần kèm Ctrl, Alt hoặc Win (Shift một mình không đủ).
    NeedModifier,
    /// Không nuốt phím: để hệ thống xử lý như thường (Alt+F4 đóng cửa sổ).
    PassThrough,
    Got(Hotkey),
}

/// Sự kiện phím khi đang chọn phím tắt, kèm trạng thái phím bổ trợ lúc xảy ra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEv {
    Down {
        vk: u32,
        ctrl: bool,
        alt: bool,
        shift: bool,
        win: bool,
    },
    Up {
        vk: u32,
        ctrl: bool,
        alt: bool,
        shift: bool,
        win: bool,
    },
}

/// Máy trạng thái của chế độ bắt phím: chốt tổ hợp ở KEYDOWN (đúng phím bổ trợ lúc bấm);
/// KEYUP không có KEYDOWN tương ứng bị bỏ, trừ PrintScreen (không sinh KEYDOWN).
#[derive(Debug, Default)]
pub struct CaptureState {
    downs: Vec<u32>,
}

impl CaptureState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, ev: KeyEv) -> Capture {
        match ev {
            KeyEv::Down {
                vk,
                ctrl,
                alt,
                shift,
                win,
            } => {
                let out = capture_hotkey(vk, ctrl, alt, shift, win);
                if matches!(out, Capture::Got(_) | Capture::NeedModifier)
                    && !self.downs.contains(&vk)
                {
                    self.downs.push(vk);
                }
                out
            }
            KeyEv::Up {
                vk,
                ctrl,
                alt,
                shift,
                win,
            } => {
                if let Some(i) = self.downs.iter().position(|k| *k == vk) {
                    self.downs.swap_remove(i);
                    Capture::Ignore
                } else if vk == 0x2C {
                    // PrintScreen không sinh KEYDOWN: nhận ở KEYUP.
                    capture_hotkey(vk, ctrl, alt, shift, win)
                } else {
                    Capture::Ignore
                }
            }
        }
    }
}

/// Phím cho phép đứng một mình làm phím tắt: PrintScreen, Pause, ScrollLock, F1..F24.
fn allowed_bare(vk: u32) -> bool {
    matches!(vk, 0x2C | 0x13 | 0x91 | 0x70..=0x87)
}

fn is_modifier_vk(vk: u32) -> bool {
    matches!(vk, 0x10 | 0x11 | 0x12 | 0x5B | 0x5C | 0xA0..=0xA5)
}

pub fn capture_hotkey(vk: u32, ctrl: bool, alt: bool, shift: bool, win: bool) -> Capture {
    if is_modifier_vk(vk) {
        return Capture::Ignore;
    }
    // Alt+F4 luôn là lệnh đóng cửa sổ của hệ thống.
    if vk == 0x73 && alt {
        return Capture::PassThrough;
    }
    // Shift một mình không đủ: sẽ chặn việc gõ chữ hoa.
    if ctrl || alt || win || allowed_bare(vk) {
        Capture::Got(Hotkey {
            ctrl,
            alt,
            shift,
            win,
            vk,
        })
    } else {
        Capture::NeedModifier
    }
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

    #[test]
    fn describe_hotkey() {
        assert_eq!(
            describe(&Hotkey {
                ctrl: false,
                alt: false,
                shift: false,
                win: false,
                vk: 0x2C
            }),
            "PrintScreen"
        );
        assert_eq!(
            describe(&Hotkey {
                ctrl: true,
                alt: false,
                shift: true,
                win: false,
                vk: 0x53
            }),
            "Ctrl+Shift+S"
        );
        assert_eq!(
            describe(&Hotkey {
                ctrl: false,
                alt: true,
                shift: false,
                win: false,
                vk: 0x71
            }),
            "Alt+F2"
        );
    }

    #[test]
    fn key_name_table() {
        assert_eq!(key_name(0x2C), "PrintScreen");
        assert_eq!(key_name(0x70), "F1");
        assert_eq!(key_name(0x7B), "F12");
        assert_eq!(key_name(0x87), "F24");
        assert_eq!(key_name(0x41), "A");
        assert_eq!(key_name(0x5A), "Z");
        assert_eq!(key_name(0x30), "0");
        assert_eq!(key_name(0x39), "9");
        assert_eq!(key_name(0xFF), "0xFF");
        assert_eq!(key_name(0x07), "0x07");
    }

    #[test]
    fn capture_hotkey_rules() {
        // PrintScreen đứng một mình được phép; phím chữ phải kèm phím bổ trợ.
        assert_eq!(
            capture_hotkey(0x2C, false, false, false, false),
            Capture::Got(Hotkey::default())
        );
        assert_eq!(
            capture_hotkey(0x53, false, false, false, false),
            Capture::NeedModifier
        );
        assert_eq!(
            capture_hotkey(0x53, true, false, true, false),
            Capture::Got(Hotkey {
                ctrl: true,
                alt: false,
                shift: true,
                win: false,
                vk: 0x53
            })
        );
        // Chỉ phím bổ trợ: bỏ qua.
        assert_eq!(
            capture_hotkey(0x11, true, false, false, false),
            Capture::Ignore
        );
        assert_eq!(
            capture_hotkey(0x5B, false, false, false, true),
            Capture::Ignore
        );
        // Shift + chữ không đủ.
        assert_eq!(
            capture_hotkey(0x41, false, false, true, false),
            Capture::NeedModifier
        );
        // F-key đứng một mình được phép.
        assert!(matches!(
            capture_hotkey(0x71, false, false, false, false),
            Capture::Got(_)
        ));
    }

    fn dn(vk: u32, ctrl: bool, alt: bool, shift: bool, win: bool) -> KeyEv {
        KeyEv::Down {
            vk,
            ctrl,
            alt,
            shift,
            win,
        }
    }
    fn up(vk: u32, ctrl: bool, alt: bool, shift: bool, win: bool) -> KeyEv {
        KeyEv::Up {
            vk,
            ctrl,
            alt,
            shift,
            win,
        }
    }
    fn hk(ctrl: bool, alt: bool, shift: bool, win: bool, vk: u32) -> Capture {
        Capture::Got(Hotkey {
            ctrl,
            alt,
            shift,
            win,
            vk,
        })
    }

    #[test]
    fn ctrl_released_before_main_key_still_gives_full_combo() {
        let mut st = CaptureState::new();
        // Nhấn Ctrl, Shift (bổ trợ: bỏ qua), rồi S với cả hai đang giữ.
        assert_eq!(
            st.feed(dn(0x11, true, false, false, false)),
            Capture::Ignore
        );
        assert_eq!(st.feed(dn(0x10, true, false, true, false)), Capture::Ignore);
        assert_eq!(
            st.feed(dn(0x53, true, false, true, false)),
            hk(true, false, true, false, 0x53)
        );
        // Ctrl nhả trước S: KEYUP không đổi kết quả.
        assert_eq!(
            st.feed(up(0x11, false, false, true, false)),
            Capture::Ignore
        );
        assert_eq!(
            st.feed(up(0x53, false, false, true, false)),
            Capture::Ignore
        );
    }

    #[test]
    fn orphan_keyup_is_ignored() {
        let mut st = CaptureState::new();
        assert_eq!(
            st.feed(up(0x0D, false, false, false, false)),
            Capture::Ignore
        );
        assert_eq!(
            st.feed(up(0x20, false, false, false, false)),
            Capture::Ignore
        );
    }

    #[test]
    fn printscreen_keyup_only_is_accepted() {
        let mut st = CaptureState::new();
        assert_eq!(
            st.feed(up(0x2C, false, false, false, false)),
            Capture::Got(Hotkey::default())
        );
    }

    #[test]
    fn shift_plus_letter_rejected_but_other_modifiers_accepted() {
        let mut st = CaptureState::new();
        assert_eq!(
            st.feed(dn(0x41, false, false, true, false)),
            Capture::NeedModifier
        );
        assert_eq!(
            st.feed(dn(0x41, true, false, false, false)),
            hk(true, false, false, false, 0x41)
        );
        assert_eq!(
            st.feed(dn(0x41, false, true, false, false)),
            hk(false, true, false, false, 0x41)
        );
        assert_eq!(
            st.feed(dn(0x41, false, false, false, true)),
            hk(false, false, false, true, 0x41)
        );
    }

    #[test]
    fn function_keys_alone_or_with_shift_accepted() {
        let mut st = CaptureState::new();
        assert_eq!(
            st.feed(dn(0x74, false, false, true, false)),
            hk(false, false, true, false, 0x74)
        );
        assert_eq!(
            st.feed(dn(0x74, false, false, false, false)),
            hk(false, false, false, false, 0x74)
        );
    }

    #[test]
    fn alt_f4_is_never_a_combo() {
        let mut st = CaptureState::new();
        assert_eq!(
            st.feed(dn(0x73, false, true, false, false)),
            Capture::PassThrough
        );
        assert_eq!(
            capture_hotkey(0x73, true, true, false, false),
            Capture::PassThrough
        );
    }
}
