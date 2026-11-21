//! Mô hình thuần của cửa sổ Tùy chọn macOS (Task 107): đổi giữa `Config` và `Form` của core,
//! tab chứa lỗi, ô nhận tiêu điểm, bắt phím qua `keymap`, trạng thái dòng phím tắt, ghi cấu hình.
//! Không đụng AppKit nên chạy test được trên máy không có giao diện.
//!
//! Khóa bí mật: `Form` giữ khóa trong `secret_access_key` (ô để trống = giữ khóa cũ, đúng
//! `apply_form`); `Debug` của `Form` đã che khóa. Module này không bao giờ in hay trả khóa trong
//! thông báo lỗi.

use crate::keymap::{is_windows_only_key, mac_describe, mac_kind, mac_to_vk, HotkeyFailures};
use darkshot_core::app_flow::OpenReason;
use darkshot_core::config::{self, Config, Expiry, Hotkey, HotkeyRole, HotkeySlot, Hotkeys};
use darkshot_core::config::{ConfigError, SecretStore};
use darkshot_core::hotkey_keys::{Capture, CaptureState, KeyEv};
use darkshot_core::i18n::{Lang, Msg};
use darkshot_core::settings_form::{initial_tab, tab_of, Field, Form, FormError, Tab};
use std::path::Path;

/// Biểu mẫu ban đầu của một cấu hình. Ô khóa bí mật để TRỐNG (không bao giờ hiện lại khóa đã lưu);
/// các trường bản Mac không có điều khiển (khởi động cùng máy, cập nhật, lịch sử, bắt cửa sổ,
/// dòng "chụp lại vùng trước") giữ nguyên giá trị cấu hình để mở rồi OK không làm đổi gì.
pub fn form_from_config(cfg: &Config) -> Form {
    Form {
        account_id: cfg.r2.account_id.clone(),
        access_key_id: cfg.r2.access_key_id.clone(),
        secret_access_key: String::new(),
        bucket: cfg.r2.bucket.clone(),
        public_base_url: cfg.r2.public_base_url.clone(),
        expiry: cfg.expiry,
        hotkeys: cfg.hotkeys,
        save_dir: cfg.save_dir.to_string_lossy().into_owned(),
        autostart: cfg.autostart,
        language: cfg.language,
        show_success_notices: cfg.show_success_notices,
        check_updates: cfg.check_updates,
        keep_selection: cfg.keep_selection,
        include_cursor: cfg.include_cursor,
        upload_format: cfg.upload_format,
        jpeg_quality: cfg.jpeg_quality,
        history_enabled: cfg.history_enabled,
        snap_windows: cfg.snap_windows,
    }
}

/// Các dòng phím tắt có hiện trên Mac (`keymap::mac_kind`), theo thứ tự `HotkeyRole::ALL`.
pub fn visible_roles() -> Vec<HotkeyRole> {
    HotkeyRole::ALL
        .into_iter()
        .filter(|r| mac_kind(*r).is_some())
        .collect()
}

/// Ô nhận tiêu điểm khi `field` sai: dòng phím tắt bị ẩn trên Mac thì về dòng đầu còn hiện.
pub fn focus_field(field: Field) -> Field {
    match field {
        Field::HotkeyRow(r) if mac_kind(r).is_none() => Field::HotkeyRow(
            visible_roles()
                .first()
                .copied()
                .unwrap_or(HotkeyRole::Region),
        ),
        f => f,
    }
}

/// (tab, ô) để chuyển tới khi `apply_form` báo lỗi.
pub fn error_target(e: &FormError) -> (Tab, Field) {
    let field = focus_field(e.field);
    (tab_of(field), field)
}

/// Chỉ số tab (theo `Tab::ALL`) mở đầu cho lý do mở.
pub fn initial_tab_index(reason: OpenReason) -> usize {
    initial_tab(reason).index()
}

/// Chỉ số của `e` trong `Expiry::ALL` (thứ tự mục của danh sách thả).
pub fn expiry_index(e: Expiry) -> usize {
    Expiry::ALL.iter().position(|x| *x == e).unwrap_or(0)
}

/// `Expiry` của chỉ số mục; ngoài khoảng thì mặc định.
pub fn expiry_from_index(i: isize) -> Expiry {
    usize::try_from(i)
        .ok()
        .and_then(|i| Expiry::ALL.get(i).copied())
        .unwrap_or_default()
}

/// Chỉ số của `l` trong `Lang::ALL`.
pub fn lang_index(l: Lang) -> usize {
    Lang::ALL.iter().position(|x| *x == l).unwrap_or(0)
}

/// `Lang` của chỉ số mục; ngoài khoảng thì `fallback`.
pub fn lang_from_index(i: isize, fallback: Lang) -> Lang {
    usize::try_from(i)
        .ok()
        .and_then(|i| Lang::ALL.get(i).copied())
        .unwrap_or(fallback)
}

/// Trạng thái hiện cạnh một dòng phím tắt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowStatus {
    Ok,
    /// Lần đăng ký gần nhất báo phím đang bị ứng dụng khác giữ.
    Taken,
    /// Phím không có trên Mac.
    Unsupported,
    /// PrintScreen/ScrollLock/Pause mang từ Windows: F13–F15.
    WindowsKey,
}

/// Trạng thái dòng `role` theo lần đăng ký gần nhất (`failures`); dòng tắt luôn `Ok`.
pub fn row_status(keys: &Hotkeys, role: HotkeyRole, failures: &HotkeyFailures) -> RowStatus {
    let slot = keys.get(role);
    if !slot.enabled || mac_kind(role).is_none() {
        RowStatus::Ok
    } else if failures.unsupported.contains(&role) {
        RowStatus::Unsupported
    } else if failures.taken.contains(&role) {
        RowStatus::Taken
    } else if is_windows_only_key(slot.key.vk) {
        RowStatus::WindowsKey
    } else {
        RowStatus::Ok
    }
}

/// Câu hiện cho một trạng thái; `Ok` không có câu.
pub fn status_msg(s: RowStatus) -> Option<Msg> {
    match s {
        RowStatus::Ok => None,
        RowStatus::Taken => Some(Msg::MacHotkeyStatusTaken),
        RowStatus::Unsupported => Some(Msg::MacHotkeyStatusUnsupported),
        RowStatus::WindowsKey => Some(Msg::MacHotkeyStatusWindowsKey),
    }
}

/// Một sự kiện phím của ô bắt phím (mã phím macOS và phím bổ trợ lúc xảy ra).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyInput {
    pub code: u16,
    pub down: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub cmd: bool,
}

/// Kết quả của một sự kiện phím khi đang bắt phím.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureStep {
    /// Chưa đủ (nhả phím, phím bổ trợ): chờ tiếp.
    Wait,
    /// `Esc` trơn: bỏ việc bắt phím.
    Cancel,
    /// Phím thường cần kèm Control, Option hoặc Command.
    NeedModifier,
    /// Phím không có trong bảng đổi mã của bản Mac.
    Unsupported,
    /// Tổ hợp hợp lệ (dạng cấu hình: `vk` của Windows, `win` = Command).
    Got(Hotkey),
}

/// Đổi một sự kiện phím macOS thành bước bắt phím, dùng `keymap::mac_to_vk` rồi máy trạng thái
/// `CaptureState` của core (luật phím hợp lệ nằm ở core).
pub fn capture_step(state: &mut CaptureState, input: KeyInput) -> CaptureStep {
    /// `kVK_Escape`.
    const MAC_ESCAPE: u16 = 0x35;
    let bare = !(input.ctrl || input.alt || input.shift || input.cmd);
    if input.down && input.code == MAC_ESCAPE && bare {
        return CaptureStep::Cancel;
    }
    let Some(vk) = mac_to_vk(u32::from(input.code)) else {
        // Nhả phím lạ: bỏ qua; bấm phím lạ: báo chưa hỗ trợ.
        return if input.down {
            CaptureStep::Unsupported
        } else {
            CaptureStep::Wait
        };
    };
    let (ctrl, alt, shift, win) = (input.ctrl, input.alt, input.shift, input.cmd);
    let ev = if input.down {
        KeyEv::Down {
            vk,
            ctrl,
            alt,
            shift,
            win,
        }
    } else {
        KeyEv::Up {
            vk,
            ctrl,
            alt,
            shift,
            win,
        }
    };
    match state.feed(ev) {
        Capture::Got(hk) => CaptureStep::Got(hk),
        Capture::NeedModifier => CaptureStep::NeedModifier,
        // Alt+F4 không có nghĩa trên Mac; coi như chưa đủ.
        Capture::Ignore | Capture::PassThrough => CaptureStep::Wait,
    }
}

/// Tên tổ hợp hiện trong ô phím (kiểu Mac).
pub fn hotkey_text(slot: &HotkeySlot) -> String {
    mac_describe(&slot.key)
}

/// Ghi cấu hình bằng `config::save`; khóa bí mật chỉ đi vào `store`. Lỗi trả về là câu của core
/// (không chứa khóa).
pub fn persist(path: &Path, cfg: &Config, store: &dyn SecretStore) -> Result<(), String> {
    config::save(path, cfg, store).map_err(|e| match e {
        ConfigError::Io(m) | ConfigError::Corrupt(m) | ConfigError::Secret(m) => m,
    })
}

/// Cỡ cửa sổ và cột (điểm), dùng chung cho dựng giao diện và kiểm tra chữ không bị cắt.
pub mod layout {
    /// Ước lượng bề rộng trung bình một ký tự (điểm) của phông hệ thống 13 pt, hơi rộng để an toàn
    /// với chữ có dấu. Chỉ dùng để kiểm thử chữ không bị cắt.
    #[cfg(test)]
    pub const AVG_CHAR_W: f64 = 7.6;
    pub const WINDOW_W: f64 = 620.0;
    pub const WINDOW_H: f64 = 490.0;
    /// Lề trong của vùng nội dung một tab.
    pub const PAD: f64 = 20.0;
    /// Bề rộng cột nhãn của các hàng "nhãn + ô".
    pub const LABEL_W: f64 = 190.0;
    /// Ô hiện tổ hợp phím (đủ cho "Control+Option+Shift+Command+Backspace" ở phông 13 pt) và nút
    /// "Đổi…" cạnh nó.
    pub const KEY_FIELD_W: f64 = 320.0;
    pub const CHANGE_BTN_W: f64 = 90.0;
    /// Hộp tích của một dòng phím tắt (nằm riêng một dòng, trên ô phím).
    pub const CHECK_W: f64 = 500.0;
    /// Bề rộng vùng nội dung của một tab.
    pub const CONTENT_W: f64 = 560.0;

    /// Chữ `text` (một dòng) vừa trong `width` điểm theo ước lượng.
    #[cfg(test)]
    pub fn text_fits(text: &str, width: f64) -> bool {
        text.chars().count() as f64 * AVG_CHAR_W <= width
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkshot_core::config::UploadFormat;
    use darkshot_core::i18n::t;
    use darkshot_core::settings_form::apply_form;
    use std::path::PathBuf;
    use std::sync::Mutex;

    /// Cấu hình mẫu: khóa giả (không phải khóa thật), mọi cờ ở giá trị không mặc định.
    fn sample() -> Config {
        let mut c = Config::defaults(Path::new("/tmp/darkshot-pics"), Lang::En);
        c.r2.account_id = "acc-1".into();
        c.r2.access_key_id = "kid".into();
        c.r2.secret_access_key = "SENTINEL-SECRET".into();
        c.r2.bucket = "my.bucket".into();
        c.r2.public_base_url = "https://img.example.com".into();
        c.expiry = Expiry::D7;
        c.show_success_notices = false;
        c.keep_selection = true;
        c.include_cursor = true;
        c.upload_format = UploadFormat::Jpeg;
        c.jpeg_quality = 77;
        c.autostart = true;
        c.check_updates = false;
        c.history_enabled = true;
        c.snap_windows = true;
        c.hotkeys = crate::keymap::mac_default_hotkeys();
        c.hotkeys.save_full.enabled = true;
        c.hotkeys.repeat_last.enabled = true; // ẩn trên Mac nhưng phải được giữ
        c
    }

    #[test]
    fn open_then_ok_changes_nothing() {
        for cfg in [
            sample(),
            Config::defaults(Path::new("/tmp/x"), Lang::Vi),
            Config::defaults(Path::new("/tmp/x"), Lang::En),
        ] {
            let form = form_from_config(&cfg);
            for l in Lang::ALL {
                assert_eq!(apply_form(&cfg, &form, l).as_ref(), Ok(&cfg), "{l:?}");
            }
        }
    }

    #[test]
    fn secret_box_starts_empty_and_empty_keeps_the_old_key() {
        let cfg = sample();
        let mut form = form_from_config(&cfg);
        assert_eq!(form.secret_access_key, "");
        let kept = apply_form(&cfg, &form, Lang::Vi).expect("ok");
        assert_eq!(kept.r2.secret_access_key, "SENTINEL-SECRET");
        form.secret_access_key = "  NEW-FAKE  ".into();
        let changed = apply_form(&cfg, &form, Lang::Vi).expect("ok");
        assert_eq!(changed.r2.secret_access_key, "NEW-FAKE");
    }

    #[test]
    fn form_debug_never_shows_the_secret() {
        let mut form = form_from_config(&sample());
        form.secret_access_key = "SENTINEL-TYPED".into();
        let dbg = format!("{form:?}");
        assert!(!dbg.contains("SENTINEL"), "{dbg}");
    }

    #[test]
    fn form_errors_come_from_core_and_point_at_the_right_tab() {
        let cfg = sample();
        let at = |edit: &dyn Fn(&mut Form)| {
            let mut f = form_from_config(&cfg);
            edit(&mut f);
            let e = apply_form(&cfg, &f, Lang::En).expect_err("must fail");
            // Câu lỗi không bao giờ chứa khóa.
            assert!(!e.message.contains("SENTINEL"));
            error_target(&e)
        };
        assert_eq!(
            at(&|f| f.save_dir = "  ".into()),
            (Tab::General, Field::SaveDir)
        );
        assert_eq!(
            at(&|f| f.account_id = "bad id!".into()),
            (Tab::R2, Field::AccountId)
        );
        assert_eq!(at(&|f| f.bucket = "a/b".into()), (Tab::R2, Field::Bucket));
        assert_eq!(
            at(&|f| f.public_base_url = "http://x".into()),
            (Tab::R2, Field::PublicUrl)
        );
        // Trùng phím tắt: lỗi ở dòng thứ hai, tab Phím tắt.
        assert_eq!(
            at(&|f| {
                f.hotkeys.save_full.enabled = true;
                f.hotkeys.save_full.key = f.hotkeys.region.key;
            }),
            (Tab::Hotkeys, Field::HotkeyRow(HotkeyRole::SaveFull))
        );
    }

    #[test]
    fn hidden_hotkey_row_errors_focus_a_visible_row() {
        assert_eq!(
            visible_roles(),
            vec![
                HotkeyRole::Region,
                HotkeyRole::SaveFull,
                HotkeyRole::UploadFull
            ]
        );
        assert_eq!(
            focus_field(Field::HotkeyRow(HotkeyRole::RepeatLast)),
            Field::HotkeyRow(HotkeyRole::Region)
        );
        assert_eq!(
            focus_field(Field::HotkeyRow(HotkeyRole::UploadFull)),
            Field::HotkeyRow(HotkeyRole::UploadFull)
        );
        assert_eq!(focus_field(Field::Bucket), Field::Bucket);
        let e = FormError {
            field: Field::HotkeyRow(HotkeyRole::RepeatLast),
            message: String::new(),
        };
        assert_eq!(
            error_target(&e),
            (Tab::Hotkeys, Field::HotkeyRow(HotkeyRole::Region))
        );
    }

    #[test]
    fn tabs_and_initial_tab() {
        assert_eq!(Tab::ALL, [Tab::General, Tab::Hotkeys, Tab::Format, Tab::R2]);
        assert_eq!(initial_tab_index(OpenReason::User), 0);
        assert_eq!(initial_tab_index(OpenReason::NeedsR2), 3);
        assert_eq!(Tab::ALL[initial_tab_index(OpenReason::NeedsR2)], Tab::R2);
    }

    #[test]
    fn list_indexes_round_trip() {
        for (i, e) in Expiry::ALL.into_iter().enumerate() {
            assert_eq!(expiry_index(e), i);
            assert_eq!(expiry_from_index(i as isize), e);
        }
        assert_eq!(expiry_from_index(-1), Expiry::default());
        assert_eq!(expiry_from_index(99), Expiry::default());
        for (i, l) in Lang::ALL.into_iter().enumerate() {
            assert_eq!(lang_index(l), i);
            assert_eq!(lang_from_index(i as isize, Lang::Vi), l);
        }
        assert_eq!(lang_from_index(-1, Lang::En), Lang::En);
        assert_eq!(lang_from_index(7, Lang::Vi), Lang::Vi);
    }

    fn key(code: u16, down: bool, ctrl: bool, alt: bool, shift: bool, cmd: bool) -> KeyInput {
        KeyInput {
            code,
            down,
            ctrl,
            alt,
            shift,
            cmd,
        }
    }

    #[test]
    fn capture_translates_mac_codes_and_follows_core_rules() {
        let mut st = CaptureState::new();
        // Control+Shift+4 (kVK_ANSI_4 = 0x15): VK 0x34, ctrl và shift.
        let got = capture_step(&mut st, key(0x15, true, true, false, true, false));
        assert_eq!(
            got,
            CaptureStep::Got(Hotkey {
                ctrl: true,
                alt: false,
                shift: true,
                win: false,
                vk: 0x34
            })
        );
        // Nhả phím đã nhận: bỏ qua.
        assert_eq!(
            capture_step(&mut st, key(0x15, false, true, false, true, false)),
            CaptureStep::Wait
        );
        // Command thành `win`.
        assert_eq!(
            capture_step(&mut st, key(0x01, true, false, false, false, true)),
            CaptureStep::Got(Hotkey {
                ctrl: false,
                alt: false,
                shift: false,
                win: true,
                vk: 0x53
            })
        );
        // Chữ trần hoặc chỉ Shift: cần phím bổ trợ.
        assert_eq!(
            capture_step(&mut st, key(0x01, true, false, false, false, false)),
            CaptureStep::NeedModifier
        );
        assert_eq!(
            capture_step(&mut st, key(0x01, true, false, false, true, false)),
            CaptureStep::NeedModifier
        );
        // F13 trần (0x69) là PrintScreen dạng chuẩn; F5 trần hợp lệ.
        assert!(matches!(
            capture_step(&mut st, key(0x69, true, false, false, false, false)),
            CaptureStep::Got(Hotkey { vk: 0x2C, .. })
        ));
        assert!(matches!(
            capture_step(&mut st, key(0x60, true, false, false, false, false)),
            CaptureStep::Got(Hotkey { vk: 0x74, .. })
        ));
    }

    #[test]
    fn capture_escape_cancels_only_when_bare() {
        let mut st = CaptureState::new();
        assert_eq!(
            capture_step(&mut st, key(0x35, true, false, false, false, false)),
            CaptureStep::Cancel
        );
        // Control+Esc là tổ hợp hợp lệ.
        assert!(matches!(
            capture_step(&mut st, key(0x35, true, true, false, false, false)),
            CaptureStep::Got(Hotkey {
                ctrl: true,
                vk: 0x1B,
                ..
            })
        ));
    }

    #[test]
    fn capture_reports_keys_without_a_mac_mapping() {
        let mut st = CaptureState::new();
        // Mã 0x0A (phím ISO bên cạnh Shift trái), 0x41 (số bàn phụ `.`), 0x7F.
        for code in [0x0A_u16, 0x41, 0x7F] {
            assert_eq!(mac_to_vk(u32::from(code)), None);
            assert_eq!(
                capture_step(&mut st, key(code, true, true, false, false, false)),
                CaptureStep::Unsupported,
                "{code:#x}"
            );
            assert_eq!(
                capture_step(&mut st, key(code, false, true, false, false, false)),
                CaptureStep::Wait
            );
        }
    }

    #[test]
    fn capture_order_is_down_then_up_so_repeats_do_not_double_report() {
        let mut st = CaptureState::new();
        let down = key(0x00, true, true, false, false, false);
        assert!(matches!(capture_step(&mut st, down), CaptureStep::Got(_)));
        // Nhả rồi bấm lại: nhận lại.
        assert_eq!(
            capture_step(
                &mut st,
                KeyInput {
                    down: false,
                    ..down
                }
            ),
            CaptureStep::Wait
        );
        assert!(matches!(capture_step(&mut st, down), CaptureStep::Got(_)));
    }

    #[test]
    fn mac_key_labels() {
        let keys = crate::keymap::mac_default_hotkeys();
        assert_eq!(hotkey_text(&keys.region), "Control+Shift+4");
        assert_eq!(hotkey_text(&keys.upload_full), "Control+Option+3");
        let slot = HotkeySlot {
            enabled: true,
            key: Hotkey {
                ctrl: false,
                alt: false,
                shift: false,
                win: true,
                vk: 0x2C,
            },
        };
        assert_eq!(hotkey_text(&slot), "Command+F13");
    }

    #[test]
    fn row_status_separates_taken_unsupported_and_windows_keys() {
        let mut keys = crate::keymap::mac_default_hotkeys();
        keys.save_full.enabled = true;
        keys.save_full.key.vk = 0x2D; // Insert: không có trên Mac
        keys.upload_full.enabled = true;
        keys.upload_full.key.vk = 0x2C; // PrintScreen
        let failed = [HotkeyRole::Region, HotkeyRole::SaveFull];
        let f = crate::keymap::classify_failed(&keys, &failed);
        assert_eq!(row_status(&keys, HotkeyRole::Region, &f), RowStatus::Taken);
        assert_eq!(
            row_status(&keys, HotkeyRole::SaveFull, &f),
            RowStatus::Unsupported
        );
        assert_eq!(
            row_status(&keys, HotkeyRole::UploadFull, &f),
            RowStatus::WindowsKey
        );
        // Dòng tắt không báo gì, dù phím gì.
        keys.upload_full.enabled = false;
        assert_eq!(row_status(&keys, HotkeyRole::UploadFull, &f), RowStatus::Ok);
        assert_eq!(row_status(&keys, HotkeyRole::RepeatLast, &f), RowStatus::Ok);
        assert_eq!(status_msg(RowStatus::Ok), None);
        for s in [
            RowStatus::Taken,
            RowStatus::Unsupported,
            RowStatus::WindowsKey,
        ] {
            let m = status_msg(s).expect("message");
            for l in Lang::ALL {
                assert!(!t(l, m).is_empty());
            }
        }
        // "Bị chiếm" và "chưa hỗ trợ" là hai câu khác nhau.
        assert_ne!(
            status_msg(RowStatus::Taken),
            status_msg(RowStatus::Unsupported)
        );
    }

    /// Kho khóa giả: ghi nhớ thứ nó nhận, không chạm Keychain.
    struct FakeStore(Mutex<Vec<Vec<u8>>>);
    impl SecretStore for FakeStore {
        fn protect(&self, plain: &[u8]) -> Result<Vec<u8>, String> {
            if let Ok(mut v) = self.0.lock() {
                v.push(plain.to_vec());
            }
            Ok(plain.iter().rev().copied().collect())
        }
        fn unprotect(&self, cipher: &[u8]) -> Result<Vec<u8>, String> {
            Ok(cipher.iter().rev().copied().collect())
        }
    }
    struct FailStore;
    impl SecretStore for FailStore {
        fn protect(&self, _: &[u8]) -> Result<Vec<u8>, String> {
            Err("keychain unavailable".into())
        }
        fn unprotect(&self, _: &[u8]) -> Result<Vec<u8>, String> {
            Err("keychain unavailable".into())
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("darkshot_opt_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d.join("config.json")
    }

    #[test]
    fn persist_sends_the_secret_only_to_the_store() {
        let path = temp_path("persist");
        let store = FakeStore(Mutex::new(Vec::new()));
        let cfg = sample();
        persist(&path, &cfg, &store).expect("saved");
        assert_eq!(
            store.0.lock().map(|v| v.clone()).unwrap_or_default(),
            vec![b"SENTINEL-SECRET".to_vec()]
        );
        let text = std::fs::read_to_string(&path).expect("file");
        assert!(!text.contains("SENTINEL-SECRET"), "secret leaked to file");
        // Đọc lại: khóa về nguyên vẹn qua kho giả.
        let loaded = config::load(&path, &store, Path::new("/tmp/x"), Lang::En).expect("load");
        assert_eq!(loaded.config.r2.secret_access_key, "SENTINEL-SECRET");
        let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
    }

    #[test]
    fn persist_failure_is_a_plain_message_without_the_secret() {
        let path = temp_path("persist_fail");
        let err = persist(&path, &sample(), &FailStore).expect_err("must fail");
        assert!(err.contains("keychain unavailable"));
        assert!(!err.contains("SENTINEL"));
        // Lỗi kho khóa xảy ra trước khi ghi: không có tệp nào.
        assert!(!path.exists());
    }

    #[test]
    fn tab_labels_come_from_core() {
        for (tab, msg) in [
            (Tab::General, Msg::TabGeneral),
            (Tab::Hotkeys, Msg::TabHotkeys),
            (Tab::Format, Msg::TabFormat),
            (Tab::R2, Msg::TabR2),
        ] {
            assert_eq!(tab.label(), msg);
        }
        // Tab nào cũng có tên khác nhau ở cả hai ngôn ngữ.
        for l in Lang::ALL {
            let names: Vec<&str> = Tab::ALL.iter().map(|t_| t(l, t_.label())).collect();
            for (i, a) in names.iter().enumerate() {
                assert!(names.iter().skip(i + 1).all(|b| a != b));
            }
        }
        assert_eq!(tab_of(Field::SaveDir), Tab::General);
    }

    #[test]
    fn longest_combo_fits_the_key_field() {
        let all = Hotkey {
            ctrl: true,
            alt: true,
            shift: true,
            win: true,
            vk: 0x08, // Backspace: tên phím dài nhất trong bảng
        };
        let text = mac_describe(&all);
        assert_eq!(text, "Control+Option+Shift+Command+Backspace");
        // Ô có đệm 8 điểm mỗi bên.
        assert!(layout::text_fits(&text, layout::KEY_FIELD_W - 16.0));
        // Hàng phím tắt: lề + thụt + ô + khe + nút vừa trong vùng nội dung.
        let right = layout::PAD + 20.0 + layout::KEY_FIELD_W + 8.0 + layout::CHANGE_BTN_W;
        assert!(right <= layout::CONTENT_W - layout::PAD / 2.0);
    }

    #[test]
    fn labels_fit_their_columns_in_both_languages() {
        use layout::*;
        for l in Lang::ALL {
            // Cột nhãn của hàng "nhãn + ô".
            for m in [
                Msg::LabelSaveDir,
                Msg::LabelLanguage,
                Msg::LabelBucket,
                Msg::LabelPublicUrl,
                Msg::LabelExpiry,
                Msg::LabelJpegQuality,
            ] {
                assert!(text_fits(t(l, m), LABEL_W), "{m:?} {l:?}");
            }
            // Tab, nút.
            for m in [Msg::BtnChoose, Msg::BtnChange, Msg::BtnCancel] {
                assert!(text_fits(t(l, m), CHANGE_BTN_W - 16.0), "{m:?} {l:?}");
            }
            assert!(text_fits(t(l, Msg::BtnTest), 150.0), "{l:?}");
            // Dòng phím tắt: hộp tích kèm nhãn.
            for r in HotkeyRole::ALL {
                assert!(text_fits(t(l, r.label()), CHECK_W - 24.0), "{r:?} {l:?}");
            }
            // Hộp tích tab Thông thường và nút chọn định dạng (một dòng trong CONTENT_W).
            for m in [
                Msg::ChkShowSuccessNotices,
                Msg::ChkKeepSelection,
                Msg::ChkIncludeCursor,
                Msg::RadioPng,
                Msg::RadioJpeg,
                Msg::NoteSavedAlwaysPng,
                Msg::SecretNone,
                Msg::SecretSaved,
                Msg::LabelUploadFormat,
            ] {
                assert!(
                    text_fits(t(l, m), CONTENT_W - 2.0 * PAD - 24.0),
                    "{m:?} {l:?}"
                );
            }
        }
        // Ba nhãn tab vừa một hàng trong cửa sổ (mỗi tab thêm 24 điểm đệm).
        for l in Lang::ALL {
            let total: f64 = Tab::ALL
                .iter()
                .map(|tb| t(l, tb.label()).chars().count() as f64 * AVG_CHAR_W + 24.0)
                .sum();
            assert!(total <= WINDOW_W, "{l:?} {total}");
        }
        const { assert!(WINDOW_W >= CONTENT_W) };
    }
}
