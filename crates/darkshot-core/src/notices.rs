//! Nội dung và loại thông báo dùng chung mọi nền tảng: phím tắt bị chiếm, lỗi phiên chụp từng
//! bước, lỗi ghim ảnh. Chữ lấy từ bảng `i18n` theo ngôn ngữ truyền vào.

use crate::actions::NoticeKind;
use crate::config::{HotkeyRole, Hotkeys};
use crate::hotkey_keys::taken_combos;
use crate::i18n::{self, Lang, Msg};

/// Lỗi của phiên chụp từng bước, kèm lý do (chữ của hệ thống hoặc chữ đã dịch).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepError {
    /// Không ghi được ảnh bước: bước không được thêm.
    Image(String),
    /// Không ghi được `session.json`: phiên trong bộ nhớ vẫn đúng, json được ghi lại ở lần thay
    /// đổi sau hoặc lúc thoát; hoặc không kết thúc được phiên.
    Session(String),
    /// Không bắt đầu được phiên.
    Start(String),
    /// Phiên đã kết thúc nhưng không xóa hết tệp của nó (phần sót được xóa nốt về sau).
    End(String),
}

/// Lỗi khi ghim ảnh.
#[derive(Debug)]
pub enum PinError {
    Limit,
    /// Ảnh vượt `MAX_SIDE` ngay ở mức thu nhỏ nhất.
    TooLarge,
    Create(String),
}

/// Tiêu đề và nội dung của thông báo gộp "Phím tắt chưa hoạt động"; `None` khi không có dòng lỗi.
pub fn hotkeys_taken_notice(
    lang: Lang,
    keys: &Hotkeys,
    failed: &[HotkeyRole],
) -> Option<(&'static str, String)> {
    if failed.is_empty() {
        return None;
    }
    Some((
        i18n::t(lang, Msg::HotkeysNotWorkingTitle),
        i18n::hotkeys_taken_body(lang, &taken_combos(keys, failed)),
    ))
}

/// Tiêu đề và nội dung thông báo cho một lỗi của phiên chụp từng bước.
pub fn step_error_notice(lang: Lang, e: &StepError) -> (&'static str, String) {
    let (title, why) = match e {
        StepError::Image(w) => (Msg::SaveFailedTitle, w),
        StepError::Session(w) => (Msg::StepsSessionSaveFailedTitle, w),
        StepError::End(w) => (Msg::StepsEndFailedTitle, w),
        StepError::Start(w) => (Msg::StepsStartFailedTitle, w),
    };
    (i18n::t(lang, title), i18n::reason(lang, why))
}

/// Thông báo khi không ghim được: đủ ảnh thì Info, ảnh quá lớn hay lỗi tạo cửa sổ thì Error. Lý do của
/// `PinError::Create` (tiếng Anh, chữ của hệ thống) không bao giờ hiện cho người dùng.
pub fn pin_failure_notice(lang: Lang, e: &PinError) -> (NoticeKind, &'static str, String) {
    match e {
        PinError::Limit => (
            NoticeKind::Info,
            i18n::t(lang, Msg::PinLimitTitle),
            i18n::pin_limit_body(lang, crate::pin::MAX_PINS),
        ),
        PinError::TooLarge => (
            NoticeKind::Error,
            i18n::t(lang, Msg::PinFailedTitle),
            i18n::t(lang, Msg::PinTooLargeBody).to_string(),
        ),
        PinError::Create(_) => (
            NoticeKind::Error,
            i18n::t(lang, Msg::PinFailedTitle),
            i18n::t(lang, Msg::PinFailedBody).to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_error_titles() {
        let (t, b) = step_error_notice(Lang::En, &StepError::Start("x".into()));
        assert_eq!(t, "Couldn't start step capture");
        assert_eq!(b, "Reason: x");
        let (t, _) = step_error_notice(Lang::En, &StepError::Session("x".into()));
        assert_eq!(t, i18n::t(Lang::En, Msg::StepsSessionSaveFailedTitle));
        let (t, _) = step_error_notice(Lang::En, &StepError::Image("x".into()));
        assert_eq!(t, i18n::t(Lang::En, Msg::SaveFailedTitle));
        let (t, b) = step_error_notice(Lang::En, &StepError::End("x".into()));
        assert_eq!(t, i18n::t(Lang::En, Msg::StepsEndFailedTitle));
        assert_eq!(b, "Reason: x");
    }

    #[test]
    fn hotkeys_taken_notice_only_when_rows_failed() {
        use crate::config::HotkeyRole::*;
        let keys = Hotkeys::default();
        assert_eq!(hotkeys_taken_notice(Lang::Vi, &keys, &[]), None);
        let (title, body) = hotkeys_taken_notice(Lang::Vi, &keys, &[UploadFull, Region]).unwrap();
        assert_eq!(title, i18n::t(Lang::Vi, Msg::HotkeysNotWorkingTitle));
        assert_eq!(
            body,
            i18n::hotkeys_taken_body(
                Lang::Vi,
                &["PrintScreen".to_string(), "Ctrl+PrintScreen".to_string()]
            )
        );
    }

    /// Lỗi ghim: đủ ảnh thì Info với số tối đa; lỗi tạo thì Error với thân i18n, không lộ lý do.
    #[test]
    fn pin_failure_notices() {
        use crate::pin::MAX_PINS;
        for lang in Lang::ALL {
            let (kind, title, body) = pin_failure_notice(lang, &PinError::Limit);
            assert_eq!(kind, NoticeKind::Info);
            assert_eq!(title, i18n::t(lang, Msg::PinLimitTitle));
            assert_eq!(body, i18n::pin_limit_body(lang, MAX_PINS));
            let err = PinError::Create("CreateWindowExW failed 0x80070008".into());
            let (kind, title, body) = pin_failure_notice(lang, &err);
            assert_eq!(kind, NoticeKind::Error);
            assert_eq!(title, i18n::t(lang, Msg::PinFailedTitle));
            assert_eq!(body, i18n::t(lang, Msg::PinFailedBody));
            assert!(!body.contains("CreateWindowExW"));
            let (kind, title, body) = pin_failure_notice(lang, &PinError::TooLarge);
            assert_eq!(kind, NoticeKind::Error);
            assert_eq!(title, i18n::t(lang, Msg::PinFailedTitle));
            assert_eq!(body, i18n::t(lang, Msg::PinTooLargeBody));
            assert_ne!(body, i18n::t(lang, Msg::PinFailedBody));
        }
    }
}
