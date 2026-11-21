//! Bảng đổi mã phím: mã VK của Windows (dạng chuẩn trong cấu hình, quyết định 131) ↔ mã phím ảo
//! của macOS (`kVK_*` trong `HIToolbox/Events.h`), phím bổ trợ Carbon, bộ phím tắt mặc định riêng
//! cho Mac (quyết định 139) và tên tổ hợp theo cách gọi của Mac. Mọi hàm ở đây thuần, có test.

use darkshot_core::capture_gate::{kind_of, CaptureKind};
use darkshot_core::config::{Hotkey, HotkeyRole, HotkeySlot, Hotkeys};
use darkshot_core::hotkey_keys::describe;
use darkshot_core::i18n::{self, Lang, Msg};

/// Bit phím bổ trợ của Carbon (`Events.h`: `cmdKeyBit` 8, `shiftKeyBit` 9, `optionKeyBit` 11,
/// `controlKeyBit` 12).
pub const MOD_COMMAND: u32 = 1 << 8;
pub const MOD_SHIFT: u32 = 1 << 9;
pub const MOD_OPTION: u32 = 1 << 11;
pub const MOD_CONTROL: u32 = 1 << 12;

/// Cặp (VK Windows, `kVK_*` macOS) một-một. PrintScreen/ScrollLock/Pause của Windows ứng với
/// F13/F14/F15 của bàn phím Apple mở rộng (quyết định 139).
const PAIRS: &[(u32, u32)] = &[
    // Chữ A–Z (`kVK_ANSI_*`).
    (0x41, 0x00), // A
    (0x42, 0x0B), // B
    (0x43, 0x08), // C
    (0x44, 0x02), // D
    (0x45, 0x0E), // E
    (0x46, 0x03), // F
    (0x47, 0x05), // G
    (0x48, 0x04), // H
    (0x49, 0x22), // I
    (0x4A, 0x26), // J
    (0x4B, 0x28), // K
    (0x4C, 0x25), // L
    (0x4D, 0x2E), // M
    (0x4E, 0x2D), // N
    (0x4F, 0x1F), // O
    (0x50, 0x23), // P
    (0x51, 0x0C), // Q
    (0x52, 0x0F), // R
    (0x53, 0x01), // S
    (0x54, 0x11), // T
    (0x55, 0x20), // U
    (0x56, 0x09), // V
    (0x57, 0x0D), // W
    (0x58, 0x07), // X
    (0x59, 0x10), // Y
    (0x5A, 0x06), // Z
    // Số 0–9 hàng trên (`kVK_ANSI_0`…`kVK_ANSI_9`).
    (0x30, 0x1D),
    (0x31, 0x12),
    (0x32, 0x13),
    (0x33, 0x14),
    (0x34, 0x15),
    (0x35, 0x17),
    (0x36, 0x16),
    (0x37, 0x1A),
    (0x38, 0x1C),
    (0x39, 0x19),
    // F1–F12.
    (0x70, 0x7A),
    (0x71, 0x78),
    (0x72, 0x63),
    (0x73, 0x76),
    (0x74, 0x60),
    (0x75, 0x61),
    (0x76, 0x62),
    (0x77, 0x64),
    (0x78, 0x65),
    (0x79, 0x6D),
    (0x7A, 0x67),
    (0x7B, 0x6F),
    // F16–F20 (F13–F15 dành cho PrintScreen/ScrollLock/Pause, xem `ALIASES`).
    (0x7F, 0x6A),
    (0x80, 0x40),
    (0x81, 0x4F),
    (0x82, 0x50),
    (0x83, 0x5A),
    // PrintScreen, ScrollLock, Pause → F13, F14, F15.
    (0x2C, 0x69),
    (0x91, 0x6B),
    (0x13, 0x71),
    // Mũi tên: Left, Up, Right, Down.
    (0x25, 0x7B),
    (0x26, 0x7E),
    (0x27, 0x7C),
    (0x28, 0x7D),
    // Space, Return, Tab, Esc.
    (0x20, 0x31),
    (0x0D, 0x24),
    (0x09, 0x30),
    (0x1B, 0x35),
    // Backspace (`kVK_Delete`), Delete (`kVK_ForwardDelete`).
    (0x08, 0x33),
    (0x2E, 0x75),
    // Home, End, PageUp, PageDown.
    (0x24, 0x73),
    (0x23, 0x77),
    (0x21, 0x74),
    (0x22, 0x79),
];

/// VK F13/F14/F15 của Windows cũng là F13/F14/F15 trên Mac (tệp cấu hình mang từ Windows sang
/// vẫn chạy), nhưng chiều ngược lại F13/F14/F15 đọc ra PrintScreen/ScrollLock/Pause (dạng chuẩn).
const ALIASES: &[(u32, u32)] = &[(0x7C, 0x69), (0x7D, 0x6B), (0x7E, 0x71)];

/// Mã phím macOS của một mã VK; `None` khi bản Mac chưa hỗ trợ phím đó.
pub fn vk_to_mac(vk: u32) -> Option<u32> {
    PAIRS
        .iter()
        .chain(ALIASES)
        .find(|(v, _)| *v == vk)
        .map(|(_, m)| *m)
}

/// Mã VK (dạng chuẩn trong cấu hình) của một mã phím macOS; `None` khi chưa hỗ trợ.
pub fn mac_to_vk(code: u32) -> Option<u32> {
    PAIRS.iter().find(|(_, m)| *m == code).map(|(v, _)| *v)
}

/// Mặt nạ phím bổ trợ Carbon của tổ hợp: `ctrl`→Control, `alt`→Option, `shift`→Shift,
/// `win`→Command.
pub fn carbon_modifiers(hk: &Hotkey) -> u32 {
    [
        (hk.ctrl, MOD_CONTROL),
        (hk.alt, MOD_OPTION),
        (hk.shift, MOD_SHIFT),
        (hk.win, MOD_COMMAND),
    ]
    .into_iter()
    .filter(|(on, _)| *on)
    .fold(0, |acc, (_, bit)| acc | bit)
}

/// (mã phím macOS, mặt nạ Carbon) để đăng ký `hk`; `None` khi phím chưa hỗ trợ trên Mac.
pub fn to_carbon(hk: &Hotkey) -> Option<(u32, u32)> {
    vk_to_mac(hk.vk).map(|code| (code, carbon_modifiers(hk)))
}

/// Tổ hợp (dạng cấu hình) từ mã phím macOS và mặt nạ Carbon; bit khác (Caps Lock…) bị bỏ qua.
pub fn from_carbon(code: u32, mods: u32) -> Option<Hotkey> {
    mac_to_vk(code).map(|vk| Hotkey {
        ctrl: mods & MOD_CONTROL != 0,
        alt: mods & MOD_OPTION != 0,
        shift: mods & MOD_SHIFT != 0,
        win: mods & MOD_COMMAND != 0,
        vk,
    })
}

/// Tên tổ hợp theo cách gọi của Mac, thứ tự Control, Option, Shift, Command (như menu macOS):
/// "Control+Shift+4". PrintScreen/ScrollLock/Pause hiện là F13/F14/F15 (phím thật trên Mac).
/// Tên phím không dịch (như `hotkey_keys::describe` của core).
pub fn mac_describe(hk: &Hotkey) -> String {
    let mut s = String::new();
    for (on, name) in [
        (hk.ctrl, "Control"),
        (hk.alt, "Option"),
        (hk.shift, "Shift"),
        (hk.win, "Command"),
    ] {
        if on {
            s.push_str(name);
            s.push('+');
        }
    }
    let key = match hk.vk {
        0x2C | 0x7C => "F13".to_string(),
        0x91 | 0x7D => "F14".to_string(),
        0x13 | 0x7E => "F15".to_string(),
        vk => describe(&Hotkey {
            ctrl: false,
            alt: false,
            shift: false,
            win: false,
            vk,
        }),
    };
    s.push_str(&key);
    s
}

/// Các dòng đăng ký không được, tách theo lý do (Task 107, rà soát Task 106).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HotkeyFailures {
    /// Phím có trên Mac nhưng ứng dụng khác đã giữ độc quyền (`eventHotKeyExistsErr`) hoặc không
    /// cài được bộ xử lý. Giới hạn của Carbon: chỉ phát hiện được ứng dụng đăng ký độc quyền;
    /// phím tắt của hệ thống hay ứng dụng đăng ký không độc quyền KHÔNG làm đăng ký thất bại, nên
    /// "không báo gì" không có nghĩa là không trùng.
    pub taken: Vec<HotkeyRole>,
    /// Phím không có trên Mac (`vk_to_mac` là `None`, ví dụ Insert hay phím số bàn phụ từ cấu
    /// hình Windows).
    pub unsupported: Vec<HotkeyRole>,
}

impl HotkeyFailures {
    pub fn is_empty(&self) -> bool {
        self.taken.is_empty() && self.unsupported.is_empty()
    }
}

/// Chia các dòng `failed` (kết quả của `hotkey::apply`) thành "bị chiếm" và "chưa hỗ trợ trên
/// Mac", mỗi nhóm theo thứ tự `HotkeyRole::ALL`.
pub fn classify_failed(keys: &Hotkeys, failed: &[HotkeyRole]) -> HotkeyFailures {
    let mut out = HotkeyFailures::default();
    for r in HotkeyRole::ALL.into_iter().filter(|r| failed.contains(r)) {
        if to_carbon(&keys.get(r).key).is_none() {
            out.unsupported.push(r);
        } else {
            out.taken.push(r);
        }
    }
    out
}

/// PrintScreen, ScrollLock, Pause: có trong cấu hình mang từ Windows, trên Mac đăng ký thành
/// F13/F14/F15 mà nhiều bàn phím Mac không có.
pub fn is_windows_only_key(vk: u32) -> bool {
    matches!(vk, 0x2C | 0x91 | 0x13)
}

/// Các dòng đang bật (và bản Mac có làm) dùng phím theo kiểu Windows (`is_windows_only_key`),
/// theo thứ tự `HotkeyRole::ALL`.
pub fn windows_key_roles(keys: &Hotkeys) -> Vec<HotkeyRole> {
    roles_to_register(keys)
        .into_iter()
        .filter(|r| is_windows_only_key(keys.get(*r).key.vk))
        .collect()
}

fn mac_combos(keys: &Hotkeys, roles: &[HotkeyRole]) -> Vec<String> {
    HotkeyRole::ALL
        .into_iter()
        .filter(|r| roles.contains(r))
        .map(|r| mac_describe(&keys.get(r).key))
        .collect()
}

/// Thông báo "Phím tắt chưa hoạt động" như `notices::hotkeys_taken_notice` của core, nhưng tên
/// phím theo cách gọi của Mac (core gọi Command là "Win", Option là "Alt") và tách hai lý do:
/// bị chiếm / chưa hỗ trợ trên Mac (hai đoạn, cách nhau một dòng). `None` khi không có dòng lỗi.
pub fn mac_taken_notice(
    lang: Lang,
    keys: &Hotkeys,
    failed: &[HotkeyRole],
) -> Option<(&'static str, String)> {
    let f = classify_failed(keys, failed);
    if f.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    if !f.taken.is_empty() {
        parts.push(i18n::hotkeys_taken_body(lang, &mac_combos(keys, &f.taken)));
    }
    if !f.unsupported.is_empty() {
        parts.push(i18n::hotkeys_unsupported_body(
            lang,
            &mac_combos(keys, &f.unsupported),
        ));
    }
    Some((i18n::t(lang, Msg::HotkeysNotWorkingTitle), parts.join("\n")))
}

/// Thông báo một lần lúc khởi động khi cấu hình dùng PrintScreen/ScrollLock/Pause (mục 3 rà soát
/// Task 106); `None` khi không có dòng nào như vậy.
pub fn mac_windows_keys_notice(lang: Lang, keys: &Hotkeys) -> Option<(&'static str, String)> {
    let roles = windows_key_roles(keys);
    if roles.is_empty() {
        return None;
    }
    Some((
        i18n::t(lang, Msg::HotkeysNotWorkingTitle),
        i18n::hotkeys_windows_keys_body(lang, &mac_combos(keys, &roles)),
    ))
}

/// Bộ phím tắt mặc định của bản Mac (quyết định 139), dùng khi chưa có tệp cấu hình. Không đụng
/// `Config::defaults` của core (dùng PrintScreen, bàn phím Mac không có). Tránh phím hệ thống
/// mặc định của macOS (Command+Shift+3/4/5, Control+Command+Shift+3/4): mọi tổ hợp ở đây dùng
/// Control, không dùng Command.
///
/// | Vai trò | Phím | Bật |
/// |---|---|---|
/// | Chụp vùng | Control+Shift+4 | có |
/// | Lưu toàn màn hình | Control+Shift+3 | không |
/// | Tải lên toàn màn hình | Control+Option+3 | không |
/// | Lặp lại lần chụp trước | Control+Option+4 | không (chưa có trên Mac) |
pub fn mac_default_hotkeys() -> Hotkeys {
    let key = |alt: bool, shift: bool, vk: u32| Hotkey {
        ctrl: true,
        alt,
        shift,
        win: false,
        vk,
    };
    Hotkeys {
        region: HotkeySlot {
            enabled: true,
            key: key(false, true, 0x34),
        },
        save_full: HotkeySlot {
            enabled: false,
            key: key(false, true, 0x33),
        },
        upload_full: HotkeySlot {
            enabled: false,
            key: key(true, false, 0x33),
        },
        repeat_last: HotkeySlot {
            enabled: false,
            key: key(true, false, 0x34),
        },
    }
}

/// Loại chụp bản Mac làm cho một dòng phím tắt. `RepeatLast` là `None`: chưa làm trên Mac vì
/// "lần chụp trước" của Windows là toạ độ trên một canvas ảo chung, còn Mac chụp mỗi màn hình một
/// ảnh (quyết định 135), nên cần thiết kế lại; dòng này không được đăng ký và không hiện ra.
pub fn mac_kind(role: HotkeyRole) -> Option<CaptureKind> {
    match role {
        HotkeyRole::RepeatLast => None,
        r => Some(kind_of(r)),
    }
}

/// Các dòng sẽ đăng ký với hệ thống: đang bật và bản Mac có làm (`mac_kind`), theo thứ tự
/// `HotkeyRole::ALL`.
pub fn roles_to_register(keys: &Hotkeys) -> Vec<HotkeyRole> {
    HotkeyRole::ALL
        .into_iter()
        .filter(|r| keys.get(*r).enabled && mac_kind(*r).is_some())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkshot_core::hotkey_keys::taken_combos;

    fn bare(vk: u32) -> Hotkey {
        Hotkey {
            ctrl: false,
            alt: false,
            shift: false,
            win: false,
            vk,
        }
    }

    /// Mọi VK được hỗ trợ đi một vòng VK → Mac → VK về đúng mã cũ; bảng là một-một.
    #[test]
    fn every_supported_code_round_trips() {
        let mut vks = Vec::new();
        let mut macs = Vec::new();
        for (vk, mac) in PAIRS {
            assert_eq!(vk_to_mac(*vk), Some(*mac), "vk {vk:#x}");
            assert_eq!(mac_to_vk(*mac), Some(*vk), "mac {mac:#x}");
            assert!(!vks.contains(vk) && !macs.contains(mac), "duplicate");
            vks.push(*vk);
            macs.push(*mac);
        }
        // Đủ nhóm phím theo gói: 26 chữ, 10 số, F1–F20, 4 mũi tên, Space, Return, Tab, Esc,
        // Backspace, Delete, Home, End, PageUp, PageDown.
        assert_eq!(PAIRS.len(), 26 + 10 + 20 + 4 + 4 + 2 + 4);
        // Mọi mã Mac có tên rồi quay lại: VK → Mac → VK → Mac.
        for code in 0..=0x7F_u32 {
            if let Some(vk) = mac_to_vk(code) {
                assert_eq!(vk_to_mac(vk), Some(code));
            }
        }
    }

    #[test]
    fn known_values_match_events_h() {
        assert_eq!(vk_to_mac(0x41), Some(0x00)); // kVK_ANSI_A
        assert_eq!(vk_to_mac(0x34), Some(0x15)); // kVK_ANSI_4
        assert_eq!(vk_to_mac(0x33), Some(0x14)); // kVK_ANSI_3
        assert_eq!(vk_to_mac(0x35), Some(0x17)); // kVK_ANSI_5
        assert_eq!(vk_to_mac(0x70), Some(0x7A)); // kVK_F1
        assert_eq!(vk_to_mac(0x83), Some(0x5A)); // kVK_F20
        assert_eq!(vk_to_mac(0x20), Some(0x31)); // kVK_Space
        assert_eq!(vk_to_mac(0x1B), Some(0x35)); // kVK_Escape
        assert_eq!(vk_to_mac(0x26), Some(0x7E)); // kVK_UpArrow
        assert_eq!(vk_to_mac(0x2E), Some(0x75)); // kVK_ForwardDelete
    }

    #[test]
    fn printscreen_scrolllock_pause_are_f13_f14_f15() {
        assert_eq!(vk_to_mac(0x2C), Some(0x69)); // kVK_F13
        assert_eq!(vk_to_mac(0x91), Some(0x6B)); // kVK_F14
        assert_eq!(vk_to_mac(0x13), Some(0x71)); // kVK_F15
        assert_eq!(mac_to_vk(0x69), Some(0x2C));
        assert_eq!(mac_to_vk(0x6B), Some(0x91));
        assert_eq!(mac_to_vk(0x71), Some(0x13));
        // VK F13–F15 của Windows cũng bấm bằng F13–F15, đọc ngược ra dạng chuẩn.
        assert_eq!(vk_to_mac(0x7C), Some(0x69));
        assert_eq!(vk_to_mac(0x7D), Some(0x6B));
        assert_eq!(vk_to_mac(0x7E), Some(0x71));
    }

    #[test]
    fn unknown_codes_are_none() {
        // VK: Insert, phím số bàn phụ, Ctrl, F21–F24, mã rác.
        for vk in [0x2D, 0x60, 0x11, 0x84, 0x87, 0xFF, 0x1_0000] {
            assert_eq!(vk_to_mac(vk), None, "vk {vk:#x}");
        }
        // Mac: kVK_Command, kVK_Function, kVK_VolumeUp, kVK_ANSI_Keypad0, kVK_Help, mã rác.
        for code in [0x37, 0x3F, 0x48, 0x52, 0x72, 0x80, 0xFFFF] {
            assert_eq!(mac_to_vk(code), None, "mac {code:#x}");
        }
    }

    #[test]
    fn modifiers_map_to_carbon_bits() {
        let hk = |ctrl, alt, shift, win| Hotkey {
            ctrl,
            alt,
            shift,
            win,
            vk: 0x41,
        };
        assert_eq!(carbon_modifiers(&hk(false, false, false, false)), 0);
        assert_eq!(carbon_modifiers(&hk(true, false, false, false)), 0x1000);
        assert_eq!(carbon_modifiers(&hk(false, true, false, false)), 0x0800);
        assert_eq!(carbon_modifiers(&hk(false, false, true, false)), 0x0200);
        assert_eq!(carbon_modifiers(&hk(false, false, false, true)), 0x0100);
        assert_eq!(carbon_modifiers(&hk(true, true, true, true)), 0x1B00);
        // Mọi tổ hợp phím bổ trợ đi một vòng.
        for bits in 0..16u8 {
            let h = hk(bits & 1 != 0, bits & 2 != 0, bits & 4 != 0, bits & 8 != 0);
            let (code, mods) = to_carbon(&h).expect("A is supported");
            assert_eq!(code, 0x00);
            assert_eq!(from_carbon(code, mods), Some(h));
        }
        // Caps Lock (alphaLock, bit 10) bị bỏ qua; phím lạ thì không có tổ hợp.
        assert_eq!(
            from_carbon(0x00, MOD_CONTROL | 1 << 10),
            Some(hk(true, false, false, false))
        );
        assert_eq!(to_carbon(&bare(0x2D)), None);
        assert_eq!(from_carbon(0x37, MOD_SHIFT), None);
    }

    #[test]
    fn mac_names_use_mac_modifiers_and_keys() {
        let d = mac_default_hotkeys();
        assert_eq!(mac_describe(&d.region.key), "Control+Shift+4");
        assert_eq!(mac_describe(&d.upload_full.key), "Control+Option+3");
        let all = Hotkey {
            ctrl: true,
            alt: true,
            shift: true,
            win: true,
            vk: 0x53,
        };
        assert_eq!(mac_describe(&all), "Control+Option+Shift+Command+S");
        assert_eq!(mac_describe(&bare(0x2C)), "F13");
        assert_eq!(mac_describe(&bare(0x7C)), "F13");
        assert_eq!(mac_describe(&bare(0x91)), "F14");
        assert_eq!(mac_describe(&bare(0x13)), "F15");
        assert_eq!(mac_describe(&bare(0x75)), "F6");
        assert_eq!(mac_describe(&bare(0x20)), "Space");
    }

    /// Thông báo phím bận: cùng tiêu đề và câu của core, tên phím kiểu Mac, thứ tự các dòng.
    #[test]
    fn mac_taken_notice_matches_core_wording() {
        let keys = mac_default_hotkeys();
        for lang in Lang::ALL {
            assert_eq!(mac_taken_notice(lang, &keys, &[]), None);
            let failed = [HotkeyRole::UploadFull, HotkeyRole::Region];
            let (title, body) = mac_taken_notice(lang, &keys, &failed).expect("notice");
            assert_eq!(title, i18n::t(lang, Msg::HotkeysNotWorkingTitle));
            assert_eq!(
                body,
                i18n::hotkeys_taken_body(
                    lang,
                    &[
                        "Control+Shift+4".to_string(),
                        "Control+Option+3".to_string()
                    ]
                )
            );
            // Không có tên kiểu Windows.
            assert!(!body.contains("Alt") && !body.contains("Win"));
        }
        // Core gọi Option là "Alt": chính vì thế mới có hàm riêng.
        assert_eq!(
            taken_combos(&keys, &[HotkeyRole::UploadFull]),
            vec!["Ctrl+Alt+3".to_string()]
        );
    }

    /// Phím mặc định hệ thống của macOS cho chụp màn hình (System Settings › Keyboard ›
    /// Keyboard Shortcuts › Screenshots).
    fn system_screenshot_keys() -> Vec<Hotkey> {
        let cmd_shift = |ctrl: bool, vk: u32| Hotkey {
            ctrl,
            alt: false,
            shift: true,
            win: true,
            vk,
        };
        vec![
            cmd_shift(false, 0x33),
            cmd_shift(false, 0x34),
            cmd_shift(false, 0x35),
            cmd_shift(true, 0x33),
            cmd_shift(true, 0x34),
        ]
    }

    #[test]
    fn mac_defaults_are_pressable_distinct_and_avoid_system_keys() {
        let d = mac_default_hotkeys();
        let keys: Vec<Hotkey> = HotkeyRole::ALL.iter().map(|r| d.get(*r).key).collect();
        for (i, k) in keys.iter().enumerate() {
            // Không dùng PrintScreen (bàn phím Mac không có) hay phím Mac chưa hỗ trợ.
            assert_ne!(k.vk, 0x2C);
            assert!(to_carbon(k).is_some());
            // Có phím bổ trợ, không dùng Command (vùng của phím hệ thống).
            assert!(k.ctrl && !k.win);
            assert!(!system_screenshot_keys().contains(k));
            // Không trùng nhau (kể cả dòng đang tắt: bật lên không xung đột).
            for other in &keys[i + 1..] {
                assert_ne!(k, other);
            }
        }
        assert_eq!(d.first_conflict(), None);
        // Chỉ chụp vùng bật sẵn; tên đọc được.
        assert!(d.region.enabled);
        assert!(!d.save_full.enabled && !d.upload_full.enabled && !d.repeat_last.enabled);
        assert_eq!(describe(&d.region.key), "Ctrl+Shift+4");
        assert_eq!(describe(&d.save_full.key), "Ctrl+Shift+3");
        assert_eq!(describe(&d.upload_full.key), "Ctrl+Alt+3");
        // Khác mặc định Windows (PrintScreen) của core.
        assert_ne!(d, Hotkeys::default());
    }

    #[test]
    fn mac_kind_follows_core_except_repeat_last() {
        assert_eq!(mac_kind(HotkeyRole::Region), Some(CaptureKind::Region));
        assert_eq!(mac_kind(HotkeyRole::SaveFull), Some(CaptureKind::SaveFull));
        assert_eq!(
            mac_kind(HotkeyRole::UploadFull),
            Some(CaptureKind::UploadFull)
        );
        assert_eq!(mac_kind(HotkeyRole::RepeatLast), None);
    }

    #[test]
    fn only_enabled_implemented_rows_are_registered() {
        let mut keys = mac_default_hotkeys();
        assert_eq!(roles_to_register(&keys), vec![HotkeyRole::Region]);
        for r in HotkeyRole::ALL {
            let mut s = keys.get(r);
            s.enabled = true;
            keys.set(r, s);
        }
        // Lặp lại lần chụp trước không bao giờ đăng ký trên Mac.
        assert_eq!(
            roles_to_register(&keys),
            vec![
                HotkeyRole::Region,
                HotkeyRole::SaveFull,
                HotkeyRole::UploadFull
            ]
        );
        let mut s = keys.region;
        s.enabled = false;
        keys.region = s;
        assert_eq!(
            roles_to_register(&keys),
            vec![HotkeyRole::SaveFull, HotkeyRole::UploadFull]
        );
    }

    #[test]
    fn failures_split_into_taken_and_unsupported() {
        let mut keys = mac_default_hotkeys();
        keys.save_full.enabled = true;
        // Insert (0x2D) và phím số bàn phụ (0x60) không có trên Mac.
        keys.save_full.key.vk = 0x2D;
        keys.upload_full = HotkeySlot {
            enabled: true,
            key: Hotkey {
                vk: 0x60,
                ..keys.upload_full.key
            },
        };
        let failed = [
            HotkeyRole::Region,
            HotkeyRole::SaveFull,
            HotkeyRole::UploadFull,
        ];
        let f = classify_failed(&keys, &failed);
        assert_eq!(f.taken, vec![HotkeyRole::Region]);
        assert_eq!(
            f.unsupported,
            vec![HotkeyRole::SaveFull, HotkeyRole::UploadFull]
        );
        assert!(classify_failed(&keys, &[]).is_empty());
        for lang in Lang::ALL {
            let (title, body) = mac_taken_notice(lang, &keys, &failed).expect("notice");
            assert_eq!(title, i18n::t(lang, Msg::HotkeysNotWorkingTitle));
            let parts: Vec<&str> = body.split('\n').collect();
            assert_eq!(parts.len(), 2, "{body}");
            assert!(parts[0].contains("Control+Shift+4"));
            assert!(parts[1].contains("0x2D") || parts[1].contains("Insert"));
            // Chỉ "chưa hỗ trợ": một đoạn, không nói "bị chiếm".
            let (_, only) = mac_taken_notice(lang, &keys, &[HotkeyRole::SaveFull]).expect("n");
            assert_eq!(
                only,
                i18n::hotkeys_unsupported_body(lang, &["Control+Shift+Insert".to_string()])
            );
        }
    }

    #[test]
    fn windows_only_keys_are_reported_once_at_startup() {
        let mut keys = Hotkeys::default(); // PrintScreen bật
        assert_eq!(windows_key_roles(&keys), vec![HotkeyRole::Region]);
        keys.save_full.enabled = true;
        keys.save_full.key.vk = 0x91;
        keys.upload_full.key.vk = 0x13; // tắt: không tính
        keys.repeat_last = HotkeySlot {
            enabled: true,
            key: Hotkey {
                vk: 0x2C,
                ..keys.repeat_last.key
            },
        }; // chưa làm trên Mac: không tính
        assert_eq!(
            windows_key_roles(&keys),
            vec![HotkeyRole::Region, HotkeyRole::SaveFull]
        );
        for lang in Lang::ALL {
            let (title, body) = mac_windows_keys_notice(lang, &keys).expect("notice");
            assert_eq!(title, i18n::t(lang, Msg::HotkeysNotWorkingTitle));
            assert!(body.contains("F13") && body.contains("Shift+F14"), "{body}");
        }
        assert_eq!(
            mac_windows_keys_notice(Lang::Vi, &mac_default_hotkeys()),
            None
        );
    }
}
