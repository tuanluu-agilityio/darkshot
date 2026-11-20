//! Đường dẫn dữ liệu thuần (không hỏi hệ điều hành): thư mục cách ly và tên tệp dưới thư mục gốc.

use std::path::{Path, PathBuf};

/// Biến môi trường của chế độ cách ly (chạy thử, không phải tính năng cho người dùng).
pub const PROFILE_ENV: &str = "DARKSHOT_PROFILE_DIR";

/// Thư mục hệ thống mà chế độ cách ly thay thế.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Known {
    /// Dữ liệu người dùng đi theo tài khoản (`%APPDATA%` trên Windows).
    Roaming,
    /// Dữ liệu cục bộ của máy (`%LOCALAPPDATA%` trên Windows).
    Local,
    /// Thư mục ảnh của người dùng.
    Pictures,
}

impl Known {
    fn sub(self) -> &'static str {
        match self {
            Known::Roaming => "Roaming",
            Known::Local => "Local",
            Known::Pictures => "Pictures",
        }
    }
}

/// Giá trị biến môi trường thành thư mục cách ly; rỗng hoặc thiếu thì không cách ly.
pub fn override_from(v: Option<std::ffi::OsString>) -> Option<PathBuf> {
    v.filter(|s| !s.is_empty()).map(PathBuf::from)
}

/// Thư mục cách ly từ `DARKSHOT_PROFILE_DIR` (không rỗng), nếu có.
pub fn profile_override() -> Option<PathBuf> {
    override_from(std::env::var_os(PROFILE_ENV))
}

/// Có thư mục cách ly thì `<dir>/Roaming|Local|Pictures` (không tạo thư mục); không thì `None`
/// và nền tảng tự hỏi hệ điều hành.
pub fn sandboxed(override_dir: Option<&Path>, known: Known) -> Option<PathBuf> {
    override_dir.map(|dir| dir.join(known.sub()))
}

/// `<local>/darkshot/history` với thư mục gốc truyền vào.
pub fn history_dir_in(local: &Path) -> PathBuf {
    local.join("darkshot").join("history")
}

/// `<local>/darkshot/steps/current` với thư mục gốc truyền vào.
pub fn steps_dir_in(local: &Path) -> PathBuf {
    local.join("darkshot").join("steps").join("current")
}

/// `<appdata>/darkshot/config.json` với thư mục gốc truyền vào.
pub fn config_path_in(appdata: &Path) -> PathBuf {
    appdata.join("darkshot").join("config.json")
}

/// `<appdata>/darkshot/last_capture.json` (cùng thư mục với `config.json`).
pub fn last_capture_path_in(appdata: &Path) -> PathBuf {
    appdata.join("darkshot").join("last_capture.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    const KNOWN: [Known; 3] = [Known::Roaming, Known::Local, Known::Pictures];

    #[test]
    fn no_override_is_not_sandboxed() {
        for k in KNOWN {
            assert_eq!(sandboxed(None, k), None, "{k:?}");
        }
    }

    #[test]
    fn sandbox_resolves_under_profile_dir() {
        let dir = Path::new("sandbox");
        assert_eq!(
            sandboxed(Some(dir), Known::Roaming),
            Some(dir.join("Roaming"))
        );
        assert_eq!(sandboxed(Some(dir), Known::Local), Some(dir.join("Local")));
        assert_eq!(
            sandboxed(Some(dir), Known::Pictures),
            Some(dir.join("Pictures"))
        );
    }

    #[test]
    fn sandboxed_does_not_create_directories() {
        let dir = std::env::temp_dir().join(format!("darkshot_dirs_{}_x", std::process::id()));
        let _ = sandboxed(Some(&dir), Known::Local);
        assert!(!dir.exists());
    }

    #[test]
    fn profile_value_must_be_non_empty() {
        assert_eq!(override_from(None), None);
        assert_eq!(override_from(Some(std::ffi::OsString::new())), None);
        assert_eq!(override_from(Some("sb".into())), Some(PathBuf::from("sb")));
    }

    #[test]
    fn history_dir_is_under_local_darkshot() {
        let root = Path::new("L");
        assert_eq!(history_dir_in(root), root.join("darkshot").join("history"));
    }

    #[test]
    fn steps_dir_is_under_local_darkshot() {
        let root = Path::new("L");
        assert_eq!(
            steps_dir_in(root),
            root.join("darkshot").join("steps").join("current")
        );
        let sb = sandboxed(Some(Path::new("sb")), Known::Local).unwrap();
        assert_eq!(
            steps_dir_in(&sb),
            Path::new("sb")
                .join("Local")
                .join("darkshot")
                .join("steps")
                .join("current")
        );
    }

    #[test]
    fn last_capture_sits_next_to_config() {
        let root = Path::new("R");
        let p = last_capture_path_in(root);
        assert_eq!(p.parent(), config_path_in(root).parent());
        assert_eq!(p.file_name().unwrap(), "last_capture.json");
        assert_eq!(config_path_in(root).file_name().unwrap(), "config.json");
        let sb = sandboxed(Some(Path::new("sb")), Known::Roaming).unwrap();
        assert_eq!(
            last_capture_path_in(&sb),
            Path::new("sb")
                .join("Roaming")
                .join("darkshot")
                .join("last_capture.json")
        );
    }

    #[test]
    fn paths_use_native_components_not_backslashes() {
        let p = config_path_in(Path::new("R"));
        assert!(p.ends_with(Path::new("darkshot").join("config.json")));
        assert_eq!(p.components().count(), 3);
    }
}
