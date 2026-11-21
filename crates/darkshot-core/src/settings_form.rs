//! Logic biểu mẫu của cửa sổ Tùy chọn, dùng chung mọi nền tảng: gộp biểu mẫu vào cấu hình, kiểm tra
//! theo thứ tự tab, tab mở đầu, định tuyến phím. Thuần: không đụng hệ điều hành.

use crate::app_flow::OpenReason;
use crate::config::{
    clamp_jpeg_quality, normalize_public_base_url, Config, Expiry, HotkeyRole, Hotkeys, R2Config,
    UploadFormat,
};
use crate::hotkey_keys::describe;
use crate::i18n::{self, t, Lang, Msg};
use std::path::PathBuf;

#[derive(Clone)]
pub struct Form {
    pub account_id: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub bucket: String,
    pub public_base_url: String,
    pub expiry: Expiry,
    pub hotkeys: Hotkeys,
    pub save_dir: String,
    pub autostart: bool,
    pub language: Lang,
    pub show_success_notices: bool,
    pub check_updates: bool,
    pub keep_selection: bool,
    pub include_cursor: bool,
    pub upload_format: UploadFormat,
    pub jpeg_quality: u8,
    /// Hộp tích "Bật lịch sử ảnh chụp" ở tab Thông thường.
    pub history_enabled: bool,
    /// Hộp tích "Gợi ý cửa sổ khi chọn vùng" ở tab Thông thường.
    pub snap_windows: bool,
}

// Debug tự viết để khóa bí mật không bao giờ lọt vào log hay thông báo lỗi.
impl std::fmt::Debug for Form {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Form")
            .field("account_id", &self.account_id)
            .field("access_key_id", &self.access_key_id)
            .field("secret_access_key", &"<redacted>")
            .field("bucket", &self.bucket)
            .field("public_base_url", &self.public_base_url)
            .field("expiry", &self.expiry)
            .field("hotkeys", &self.hotkeys)
            .field("save_dir", &self.save_dir)
            .field("autostart", &self.autostart)
            .field("language", &self.language)
            .field("show_success_notices", &self.show_success_notices)
            .field("check_updates", &self.check_updates)
            .field("keep_selection", &self.keep_selection)
            .field("include_cursor", &self.include_cursor)
            .field("upload_format", &self.upload_format)
            .field("jpeg_quality", &self.jpeg_quality)
            .field("history_enabled", &self.history_enabled)
            .field("snap_windows", &self.snap_windows)
            .finish()
    }
}

pub struct SettingsResult {
    pub config: Config,
}

fn is_account_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-'
}

fn is_bucket_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-'
}

/// Tab của cửa sổ Tùy chọn, theo thứ tự trên dải tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    General,
    Hotkeys,
    Format,
    R2,
}

/// Ô có thể sai khi bấm OK.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    SaveDir,
    HotkeyRow(HotkeyRole),
    AccountId,
    Bucket,
    PublicUrl,
}

/// Lỗi của biểu mẫu: ô sai và câu báo lỗi (không bao giờ chứa khóa bí mật).
#[derive(Debug, PartialEq, Eq)]
pub struct FormError {
    pub field: Field,
    pub message: String,
}

/// Tab chứa ô `field`.
pub fn tab_of(field: Field) -> Tab {
    match field {
        Field::SaveDir => Tab::General,
        Field::HotkeyRow(_) => Tab::Hotkeys,
        Field::AccountId | Field::Bucket | Field::PublicUrl => Tab::R2,
    }
}

impl Tab {
    /// Thứ tự trên dải tab.
    pub const ALL: [Tab; 4] = [Tab::General, Tab::Hotkeys, Tab::Format, Tab::R2];

    pub fn index(self) -> usize {
        match self {
            Tab::General => 0,
            Tab::Hotkeys => 1,
            Tab::Format => 2,
            Tab::R2 => 3,
        }
    }

    pub fn label(self) -> Msg {
        match self {
            Tab::General => Msg::TabGeneral,
            Tab::Hotkeys => Msg::TabHotkeys,
            Tab::Format => Msg::TabFormat,
            Tab::R2 => Msg::TabR2,
        }
    }

    /// Tab cách `step` bước về bên phải, vòng tròn (`len - 1` bước = tab bên trái).
    pub fn cycle(self, step: usize) -> Tab {
        Tab::ALL[(self.index() + step) % Tab::ALL.len()]
    }
}

/// Ba ô chất lượng (nhãn, thanh trượt, số) chỉ sáng khi tải lên bằng JPEG.
pub fn quality_enabled(format: UploadFormat) -> bool {
    format == UploadFormat::Jpeg
}

/// Dải tab vừa một hàng: tổng bề rộng các tab không vượt `client_w` (cùng đơn vị).
pub fn tabs_fit(tab_widths: &[i32], client_w: i32) -> bool {
    let total: i64 = tab_widths.iter().map(|w| i64::from(*w)).sum();
    total <= i64::from(client_w)
}

/// Tab mở đầu theo lý do mở.
pub fn initial_tab(reason: OpenReason) -> Tab {
    match reason {
        OpenReason::User => Tab::General,
        OpenReason::NeedsR2 => Tab::R2,
    }
}

/// Gộp biểu mẫu vào cấu hình hiện tại. Thuần: không đụng hệ điều hành.
/// Kiểm tra theo thứ tự tab: SaveDir, trùng tổ hợp giữa các dòng phím tắt đang bật, AccountId,
/// Bucket, PublicUrl; câu lỗi theo `lang`.
pub fn apply_form(current: &Config, form: &Form, lang: Lang) -> Result<Config, FormError> {
    let account_id = form.account_id.trim().to_string();
    let access_key_id = form.access_key_id.trim().to_string();
    let bucket = form.bucket.trim().to_string();
    let secret = form.secret_access_key.trim();
    let url = form.public_base_url.trim();
    let save_dir = form.save_dir.trim();
    let err = |field: Field, message: String| Err(FormError { field, message });

    if save_dir.is_empty() {
        return err(Field::SaveDir, t(lang, Msg::ErrPickSaveDir).into());
    }
    if let Some((first, second)) = form.hotkeys.first_conflict() {
        return err(
            Field::HotkeyRow(second),
            i18n::hotkeys_conflict(
                lang,
                t(lang, first.label()),
                t(lang, second.label()),
                &describe(&form.hotkeys.get(second).key),
            ),
        );
    }
    if !account_id.chars().all(is_account_char) {
        return err(Field::AccountId, t(lang, Msg::ErrAccountChars).into());
    }
    if !bucket.chars().all(is_bucket_char) {
        return err(Field::Bucket, t(lang, Msg::ErrBucketChars).into());
    }
    let public_base_url = if url.is_empty() {
        String::new()
    } else {
        match normalize_public_base_url(url) {
            Ok(u) => u,
            Err(e) => return err(Field::PublicUrl, e.message(lang)),
        }
    };
    let secret_access_key = if secret.is_empty() {
        current.r2.secret_access_key.clone()
    } else {
        secret.to_string()
    };
    Ok(Config {
        r2: R2Config {
            account_id,
            access_key_id,
            secret_access_key,
            bucket,
            public_base_url,
        },
        expiry: form.expiry,
        hotkeys: form.hotkeys,
        show_success_notices: form.show_success_notices,
        keep_selection: form.keep_selection,
        include_cursor: form.include_cursor,
        save_dir: PathBuf::from(save_dir),
        autostart: form.autostart,
        language: form.language,
        upload_format: form.upload_format,
        jpeg_quality: clamp_jpeg_quality(i64::from(form.jpeg_quality)),
        custom_colors: current.custom_colors,
        check_updates: form.check_updates,
        update_notified: current.update_notified,
        history_enabled: form.history_enabled,
        snap_windows: form.snap_windows,
    })
}

/// Một phím (WM_KEYDOWN) trong cửa sổ Tùy chọn đi về đâu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyRoute {
    Capture,
    NextTab,
    PrevTab,
    Cancel,
    Accept,
    Dialog,
}

/// Định tuyến phím; `combo_dropped` đúng khi một hộp chọn đang thả.
pub fn key_route(
    capturing: bool,
    vk: u32,
    ctrl: bool,
    shift: bool,
    combo_dropped: bool,
) -> KeyRoute {
    const TAB: u32 = 0x09;
    const VK_ESCAPE: u32 = 0x1B;
    const VK_RETURN: u32 = 0x0D;
    if capturing {
        return KeyRoute::Capture;
    }
    match vk {
        TAB if ctrl && shift => KeyRoute::PrevTab,
        TAB if ctrl => KeyRoute::NextTab,
        _ if combo_dropped => KeyRoute::Dialog,
        v if v == VK_ESCAPE => KeyRoute::Cancel,
        v if v == VK_RETURN => KeyRoute::Accept,
        _ => KeyRoute::Dialog,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Hotkey, HotkeySlot};
    use std::path::Path;

    fn base() -> Config {
        let mut c = Config::defaults(Path::new("C:/pics"), Lang::Vi);
        c.r2 = R2Config {
            account_id: "acc".into(),
            access_key_id: "kid".into(),
            secret_access_key: "OLD".into(),
            bucket: "b".into(),
            public_base_url: "https://img.example.com".into(),
        };
        c
    }

    fn form_of(c: &Config) -> Form {
        Form {
            account_id: c.r2.account_id.clone(),
            access_key_id: c.r2.access_key_id.clone(),
            secret_access_key: String::new(),
            bucket: c.r2.bucket.clone(),
            public_base_url: c.r2.public_base_url.clone(),
            expiry: c.expiry,
            hotkeys: c.hotkeys,
            save_dir: c.save_dir.to_string_lossy().into_owned(),
            autostart: c.autostart,
            language: c.language,
            show_success_notices: c.show_success_notices,
            check_updates: c.check_updates,
            keep_selection: c.keep_selection,
            include_cursor: c.include_cursor,
            upload_format: c.upload_format,
            jpeg_quality: c.jpeg_quality,
            history_enabled: c.history_enabled,
            snap_windows: c.snap_windows,
        }
    }

    #[test]
    fn history_enabled_is_kept_when_saving() {
        for on in [false, true] {
            let mut c = base();
            c.history_enabled = on;
            let n = apply_form(&c, &form_of(&c), Lang::Vi).unwrap();
            assert_eq!(n.history_enabled, on);
            assert_eq!(n, c);
        }
    }

    #[test]
    fn check_updates_follows_form_and_notified_is_kept() {
        let mut c = base();
        c.update_notified = Some(crate::update::Version {
            major: 0,
            minor: 3,
            patch: 0,
        });
        let mut f = form_of(&c);
        f.check_updates = false;
        let n = apply_form(&c, &f, Lang::Vi).unwrap();
        assert!(!n.check_updates);
        assert_eq!(n.update_notified, c.update_notified);
    }

    #[test]
    fn empty_secret_keeps_existing() {
        let c = base();
        let mut f = form_of(&c);
        assert_eq!(
            apply_form(&c, &f, Lang::Vi).unwrap().r2.secret_access_key,
            "OLD"
        );
        f.secret_access_key = "NEW".into();
        assert_eq!(
            apply_form(&c, &f, Lang::Vi).unwrap().r2.secret_access_key,
            "NEW"
        );
    }

    #[test]
    fn url_is_normalized_and_bad_url_rejected() {
        let c = base();
        let mut f = form_of(&c);
        f.public_base_url = "https://img.example.com/".into();
        assert_eq!(
            apply_form(&c, &f, Lang::Vi).unwrap().r2.public_base_url,
            "https://img.example.com"
        );
        f.public_base_url = "http://x".into();
        assert!(apply_form(&c, &f, Lang::Vi).is_err());
    }

    #[test]
    fn empty_url_allowed_when_r2_not_filled() {
        let c = Config::defaults(Path::new("C:/pics"), Lang::Vi);
        let mut f = form_of(&c);
        f.save_dir = "C:/pics".into();
        let out = apply_form(&c, &f, Lang::Vi).unwrap();
        assert!(!out.r2.is_complete());
        assert_eq!(out.r2.public_base_url, "");
    }

    #[test]
    fn fields_are_trimmed() {
        let c = base();
        let mut f = form_of(&c);
        f.bucket = " bkt ".into();
        f.account_id = " acc2 ".into();
        f.access_key_id = "\tk2 ".into();
        f.save_dir = "  C:/x  ".into();
        f.secret_access_key = " S ".into();
        let out = apply_form(&c, &f, Lang::Vi).unwrap();
        assert_eq!(out.r2.bucket, "bkt");
        assert_eq!(out.r2.account_id, "acc2");
        assert_eq!(out.r2.access_key_id, "k2");
        assert_eq!(out.r2.secret_access_key, "S");
        assert_eq!(out.save_dir, PathBuf::from("C:/x"));
    }

    #[test]
    fn empty_save_dir_rejected() {
        let c = base();
        let mut f = form_of(&c);
        f.save_dir = "   ".into();
        assert!(apply_form(&c, &f, Lang::Vi).is_err());
    }

    #[test]
    fn other_fields_are_carried() {
        let c = base();
        let mut f = form_of(&c);
        f.expiry = Expiry::Keep;
        f.autostart = true;
        f.hotkeys.region.key = Hotkey {
            ctrl: true,
            alt: false,
            shift: false,
            win: false,
            vk: 0x53,
        };
        let out = apply_form(&c, &f, Lang::Vi).unwrap();
        assert_eq!(out.expiry, Expiry::Keep);
        assert!(out.autostart);
        assert_eq!(out.hotkeys.region.key, f.hotkeys.region.key);
    }

    #[test]
    fn bad_account_or_bucket_chars_rejected() {
        let c = base();
        let mut f = form_of(&c);
        f.account_id = "a/b".into();
        assert!(apply_form(&c, &f, Lang::Vi).is_err());
        let mut f = form_of(&c);
        f.bucket = "my bucket".into();
        assert!(apply_form(&c, &f, Lang::Vi).is_err());
    }

    #[test]
    fn errors_and_debug_never_contain_secret() {
        let c = base();
        let mut f = form_of(&c);
        f.secret_access_key = "SUPER-SECRET".into();
        f.public_base_url = "http://x".into();
        for lang in Lang::ALL {
            let e = apply_form(&c, &f, lang).unwrap_err();
            assert!(!e.message.contains("SUPER-SECRET"));
            assert!(!format!("{e:?}").contains("SUPER-SECRET"));
        }
        assert!(!format!("{f:?}").contains("SUPER-SECRET"));
    }

    #[test]
    fn field_to_tab() {
        assert_eq!(tab_of(Field::SaveDir), Tab::General);
        for f in [Field::AccountId, Field::Bucket, Field::PublicUrl] {
            assert_eq!(tab_of(f), Tab::R2);
        }
    }

    #[test]
    fn hotkeys_round_trip_four_rows_through_form() {
        let c = base();
        let mut src = base();
        for (i, r) in HotkeyRole::ALL.into_iter().enumerate() {
            src.hotkeys.set(
                r,
                HotkeySlot {
                    enabled: i % 2 == 0,
                    key: Hotkey {
                        ctrl: true,
                        alt: false,
                        shift: false,
                        win: false,
                        vk: 0x41 + i as u32,
                    },
                },
            );
        }
        let f = form_of(&src);
        let out = apply_form(&c, &f, Lang::Vi).unwrap();
        for r in HotkeyRole::ALL {
            assert_eq!(out.hotkeys.get(r), src.hotkeys.get(r), "{r:?}");
        }
        assert_eq!(out.hotkeys, src.hotkeys);
    }

    #[test]
    fn repeat_last_conflict_with_region_is_reported_on_row_four() {
        let c = base();
        let mut f = form_of(&c);
        f.hotkeys.region.enabled = true;
        f.hotkeys.repeat_last = HotkeySlot {
            enabled: true,
            key: f.hotkeys.region.key,
        };
        let e = apply_form(&c, &f, Lang::Vi).unwrap_err();
        assert_eq!(e.field, Field::HotkeyRow(HotkeyRole::RepeatLast));
        assert!(e.message.contains(t(Lang::Vi, HotkeyRole::Region.label())));
        assert!(e
            .message
            .contains(t(Lang::Vi, HotkeyRole::RepeatLast.label())));
        f.hotkeys.repeat_last.enabled = false;
        assert!(apply_form(&c, &f, Lang::Vi).is_ok());
    }

    #[test]
    fn hotkey_rows_belong_to_hotkeys_tab() {
        for r in HotkeyRole::ALL {
            assert_eq!(tab_of(Field::HotkeyRow(r)), Tab::Hotkeys);
        }
    }

    #[test]
    fn hotkey_conflict_is_a_form_error() {
        let c = base();
        let mut f = form_of(&c);
        f.hotkeys.save_full = HotkeySlot {
            enabled: true,
            key: f.hotkeys.region.key,
        };
        let e = apply_form(&c, &f, Lang::Vi).unwrap_err();
        assert_eq!(e.field, Field::HotkeyRow(HotkeyRole::SaveFull));
        assert!(e.message.contains("PrintScreen"));
        assert!(e.message.contains(t(Lang::Vi, HotkeyRole::Region.label())));
        assert!(e
            .message
            .contains(t(Lang::Vi, HotkeyRole::SaveFull.label())));
        f.hotkeys.save_full.enabled = false; // dòng tắt không tính
        assert!(apply_form(&c, &f, Lang::Vi).is_ok());
    }

    #[test]
    fn error_order_is_savedir_hotkeys_r2() {
        let c = base();
        let mut f = form_of(&c);
        f.hotkeys.save_full = HotkeySlot {
            enabled: true,
            key: f.hotkeys.region.key,
        };
        f.bucket = "x y".into();
        assert_eq!(
            apply_form(&c, &f, Lang::Vi).unwrap_err().field,
            Field::HotkeyRow(HotkeyRole::SaveFull)
        );
        f.save_dir = "".into();
        assert_eq!(
            apply_form(&c, &f, Lang::Vi).unwrap_err().field,
            Field::SaveDir
        );
    }

    #[test]
    fn new_options_are_carried_from_form() {
        let c = base();
        let mut f = form_of(&c);
        f.show_success_notices = false;
        f.keep_selection = true;
        f.include_cursor = true;
        f.hotkeys.upload_full.enabled = true;
        f.hotkeys.region.enabled = false;
        let n = apply_form(&c, &f, Lang::Vi).unwrap();
        assert!(!n.show_success_notices && n.keep_selection && n.include_cursor);
        assert_eq!(n.hotkeys, f.hotkeys);
    }

    #[test]
    fn tab_order_has_four_tabs() {
        let want = [Tab::General, Tab::Hotkeys, Tab::Format, Tab::R2];
        assert_eq!(Tab::ALL.as_slice(), want.as_slice());
        for (i, t) in want.iter().enumerate() {
            assert_eq!(t.index(), i, "{t:?}");
        }
        assert_eq!(Tab::R2.index(), 3);
        // Đi tới: qua đủ bốn tab rồi về đầu.
        let mut t = Tab::General;
        let mut seen = Vec::new();
        for _ in 0..4 {
            seen.push(t);
            t = t.cycle(1);
        }
        assert_eq!(seen, want.to_vec());
        assert_eq!(t, Tab::General);
        // Đi lùi: General, R2, Format, Hotkeys rồi về General.
        let back = Tab::ALL.len() - 1;
        let mut t = Tab::General;
        let mut seen = Vec::new();
        for _ in 0..4 {
            seen.push(t);
            t = t.cycle(back);
        }
        assert_eq!(seen, vec![Tab::General, Tab::R2, Tab::Format, Tab::Hotkeys]);
        assert_eq!(t, Tab::General);
    }

    #[test]
    fn form_reads_and_writes_format() {
        let current = base();
        let mut src = base();
        src.upload_format = UploadFormat::Jpeg;
        src.jpeg_quality = 75;
        let f = form_of(&src);
        assert_eq!(f.upload_format, UploadFormat::Jpeg);
        assert_eq!(f.jpeg_quality, 75);
        let out = apply_form(&current, &f, Lang::Vi).unwrap();
        assert_eq!(out.upload_format, UploadFormat::Jpeg);
        assert_eq!(out.jpeg_quality, 75);
    }

    #[test]
    fn quality_survives_png_selection() {
        let mut current = base();
        current.upload_format = UploadFormat::Jpeg;
        current.jpeg_quality = 90;
        let mut f = form_of(&current);
        f.upload_format = UploadFormat::Png;
        f.jpeg_quality = 70;
        let out = apply_form(&current, &f, Lang::Vi).unwrap();
        assert_eq!(out.upload_format, UploadFormat::Png);
        assert_eq!(out.jpeg_quality, 70);
    }

    #[test]
    fn quality_enabled_only_for_jpeg() {
        assert!(quality_enabled(UploadFormat::Jpeg));
        assert!(!quality_enabled(UploadFormat::Png));
    }

    #[test]
    fn tabs_fit_compares_total_width() {
        assert!(tabs_fit(&[100, 100, 100, 100], 401));
        assert!(tabs_fit(&[100, 100, 100, 100], 400));
        assert!(!tabs_fit(&[100, 100, 100, 101], 400));
        assert!(!tabs_fit(&[300, 200], 499));
        assert!(tabs_fit(&[], 0));
    }

    #[test]
    fn tab_labels_follow_tab_order() {
        let want = [Msg::TabGeneral, Msg::TabHotkeys, Msg::TabFormat, Msg::TabR2];
        for (tab, msg) in Tab::ALL.into_iter().zip(want) {
            assert_eq!(tab.label(), msg);
        }
    }

    #[test]
    fn initial_tab_by_reason() {
        assert_eq!(initial_tab(OpenReason::User), Tab::General);
        assert_eq!(initial_tab(OpenReason::NeedsR2), Tab::R2);
    }

    #[test]
    fn form_errors_name_their_field() {
        let c = base();
        let mut f = form_of(&c);
        f.save_dir = "  ".into();
        assert_eq!(
            apply_form(&c, &f, Lang::Vi).unwrap_err().field,
            Field::SaveDir
        );
        let mut f = form_of(&c);
        f.account_id = "a/b".into();
        assert_eq!(
            apply_form(&c, &f, Lang::Vi).unwrap_err().field,
            Field::AccountId
        );
        let mut f = form_of(&c);
        f.bucket = "my bucket".into();
        assert_eq!(
            apply_form(&c, &f, Lang::Vi).unwrap_err().field,
            Field::Bucket
        );
        let mut f = form_of(&c);
        f.public_base_url = "http://x".into();
        assert_eq!(
            apply_form(&c, &f, Lang::Vi).unwrap_err().field,
            Field::PublicUrl
        );
    }

    #[test]
    fn first_error_wins_in_tab_order() {
        // Sai cả SaveDir lẫn Bucket: báo SaveDir.
        let c = base();
        let mut f = form_of(&c);
        f.save_dir = "".into();
        f.bucket = "x y".into();
        assert_eq!(
            apply_form(&c, &f, Lang::Vi).unwrap_err().field,
            Field::SaveDir
        );
    }

    #[test]
    fn form_error_message_follows_language() {
        let c = base();
        let mut f = form_of(&c);
        f.bucket = "x y".into();
        assert_ne!(
            apply_form(&c, &f, Lang::Vi).unwrap_err().message,
            apply_form(&c, &f, Lang::En).unwrap_err().message
        );
    }

    #[test]
    fn language_is_carried_from_form() {
        let c = base();
        let mut f = form_of(&c);
        f.language = Lang::En;
        assert_eq!(apply_form(&c, &f, Lang::Vi).unwrap().language, Lang::En);
    }

    #[test]
    fn key_routing() {
        const TAB: u32 = 0x09;
        const ESC: u32 = 0x1B;
        const ENTER: u32 = 0x0D;
        // Đang bắt phím: mọi thứ là của chế độ bắt, kể cả Ctrl+Tab, Esc, Enter.
        for (vk, ctrl) in [(TAB, true), (ESC, false), (ENTER, false), (0x41, true)] {
            assert_eq!(key_route(true, vk, ctrl, false, false), KeyRoute::Capture);
        }
        assert_eq!(key_route(false, TAB, true, false, false), KeyRoute::NextTab);
        assert_eq!(key_route(false, TAB, true, true, false), KeyRoute::PrevTab);
        assert_eq!(key_route(false, TAB, false, false, false), KeyRoute::Dialog);
        assert_eq!(key_route(false, ESC, false, false, false), KeyRoute::Cancel);
        assert_eq!(
            key_route(false, ENTER, false, false, false),
            KeyRoute::Accept
        );
        assert_eq!(key_route(false, ESC, false, false, true), KeyRoute::Dialog);
        assert_eq!(
            key_route(false, ENTER, false, false, true),
            KeyRoute::Dialog
        );
        assert_eq!(key_route(false, TAB, true, false, true), KeyRoute::NextTab);
    }
}
