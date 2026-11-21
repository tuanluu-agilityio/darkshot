//! Ngôn ngữ hiện hành của lớp macOS (đọc được từ mọi luồng) và ngôn ngữ của hệ thống.
//!
//! Ngôn ngữ hệ thống chỉ dùng khi chưa có tệp cấu hình hoặc tệp hỏng; có `Config` thì dùng
//! ngôn ngữ trong cấu hình (`darkshot_core::app_state::startup` lo việc chọn).

use darkshot_core::i18n::{t, Lang, Msg};
use std::sync::atomic::{AtomicU8, Ordering};

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn current() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::En,
        _ => Lang::Vi,
    }
}

pub fn set(lang: Lang) {
    let v = match lang {
        Lang::Vi => 0,
        Lang::En => 1,
    };
    CURRENT.store(v, Ordering::Relaxed);
}

pub fn tr(m: Msg) -> &'static str {
    t(current(), m)
}

/// Thẻ ngôn ngữ BCP 47 của macOS (`vi`, `vi-VN`, `en-US`, …) sang `Lang`: tiếng Việt khi mã
/// ngôn ngữ chính là `vi` (không phân biệt hoa thường, `-` hay `_`); còn lại là English.
pub fn lang_from_locale(tag: &str) -> Lang {
    let _ = tag;
    Lang::En // STUB
}

/// Ngôn ngữ ưu tiên đầu tiên của người dùng (`NSLocale.preferredLanguages`); danh sách rỗng thì
/// English.
pub fn os_default() -> Lang {
    let first = objc2::rc::autoreleasepool(|_| {
        objc2_foundation::NSLocale::preferredLanguages()
            .firstObject()
            .map(|s| s.to_string())
    });
    first.as_deref().map(lang_from_locale).unwrap_or(Lang::En)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vietnamese_tags_map_to_vi() {
        for tag in ["vi", "vi-VN", "VI", "vi_VN", " vi-VN ", "vi-Latn-VN"] {
            assert_eq!(lang_from_locale(tag), Lang::Vi, "{tag}");
        }
    }

    #[test]
    fn other_tags_map_to_english() {
        for tag in [
            "en",
            "en-US",
            "en-GB",
            "fr-FR",
            "zh-Hans-CN",
            "",
            "v",
            "vie",
            "vi2",
            "-vi",
        ] {
            assert_eq!(lang_from_locale(tag), Lang::En, "{tag}");
        }
    }

    // Kiểm thử duy nhất đụng biến toàn cục; đặt lại Vi ở cuối.
    #[test]
    fn tr_follows_current_language() {
        set(Lang::En);
        assert_eq!(current(), Lang::En);
        assert_eq!(tr(Msg::TrayCapture), "Take screenshot");
        set(Lang::Vi);
        assert_eq!(current(), Lang::Vi);
    }
}
