//! Đường dẫn dữ liệu trên macOS. Không panic: thiếu `HOME` thì dùng thư mục tạm.
//!
//! Cấu hình và dữ liệu cục bộ: `~/Library/Application Support/darkshot`; ảnh:
//! `~/Pictures/darkshot`. `DARKSHOT_PROFILE_DIR` ghi đè như lớp Windows (chế độ cách ly).

use darkshot_core::dirs::{self, history_dir_in, last_capture_path_in, steps_dir_in};
pub use darkshot_core::dirs::{config_path_in, profile_override, Known};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Thư mục nhà từ giá trị `HOME`: rỗng, thiếu hoặc không tuyệt đối thì dùng thư mục tạm.
pub fn home_from(v: Option<OsString>) -> PathBuf {
    let _ = v;
    todo!()
}

/// Thư mục nhà của người dùng hiện tại (theo `HOME`).
pub fn home_dir() -> PathBuf {
    home_from(std::env::var_os("HOME"))
}

/// Đường dẫn của một thư mục hệ thống với thư mục nhà cho trước: có thư mục cách ly thì
/// `<dir>/Roaming|Local|Pictures` (không tạo thư mục).
pub fn resolve_with(override_dir: Option<&Path>, known: Known, home: &Path) -> PathBuf {
    let _ = (override_dir, known, home, dirs::sandboxed);
    todo!()
}

/// Như `resolve_with`, với thư mục nhà thật.
pub fn resolve(override_dir: Option<&Path>, known: Known) -> PathBuf {
    resolve_with(override_dir, known, &home_dir())
}

/// `~/Library/Application Support` (tương ứng `%APPDATA%`).
pub fn appdata_dir() -> PathBuf {
    resolve(profile_override().as_deref(), Known::Roaming)
}

/// `~/Library/Application Support` (tương ứng `%LOCALAPPDATA%`; trên Mac trùng `appdata_dir`).
pub fn local_appdata_dir() -> PathBuf {
    resolve(profile_override().as_deref(), Known::Local)
}

/// `~/Pictures`.
pub fn pictures_dir() -> PathBuf {
    resolve(profile_override().as_deref(), Known::Pictures)
}

/// `~/Library/Application Support/darkshot/history`
pub fn history_dir() -> PathBuf {
    history_dir_in(&local_appdata_dir())
}

/// `~/Library/Application Support/darkshot/steps/current`
pub fn steps_dir() -> PathBuf {
    steps_dir_in(&local_appdata_dir())
}

/// `~/Library/Application Support/darkshot/config.json`
pub fn config_path() -> PathBuf {
    config_path_in(&appdata_dir())
}

/// `~/Library/Application Support/darkshot/last_capture.json`
pub fn last_capture_path() -> PathBuf {
    last_capture_path_in(&appdata_dir())
}

/// `~/Pictures/darkshot`
pub fn default_save_dir() -> PathBuf {
    pictures_dir().join("darkshot")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_support(home: &Path) -> PathBuf {
        home.join("Library").join("Application Support")
    }

    #[test]
    fn system_folders_under_home() {
        let home = Path::new("/Users/someone");
        assert_eq!(resolve_with(None, Known::Roaming, home), app_support(home));
        assert_eq!(resolve_with(None, Known::Local, home), app_support(home));
        assert_eq!(
            resolve_with(None, Known::Pictures, home),
            home.join("Pictures")
        );
    }

    #[test]
    fn profile_dir_overrides_home() {
        let sb = std::env::temp_dir().join(format!("darkshot_mac_paths_{}", std::process::id()));
        let home = Path::new("/Users/someone");
        for (k, sub) in [
            (Known::Roaming, "Roaming"),
            (Known::Local, "Local"),
            (Known::Pictures, "Pictures"),
        ] {
            assert_eq!(resolve_with(Some(&sb), k, home), sb.join(sub), "{k:?}");
        }
        assert_eq!(
            config_path_in(&resolve_with(Some(&sb), Known::Roaming, home)),
            sb.join("Roaming").join("darkshot").join("config.json")
        );
        assert_eq!(
            history_dir_in(&resolve_with(Some(&sb), Known::Local, home)),
            sb.join("Local").join("darkshot").join("history")
        );
        assert_eq!(
            steps_dir_in(&resolve_with(Some(&sb), Known::Local, home)),
            sb.join("Local")
                .join("darkshot")
                .join("steps")
                .join("current")
        );
        assert_eq!(
            last_capture_path_in(&resolve_with(Some(&sb), Known::Roaming, home)),
            sb.join("Roaming")
                .join("darkshot")
                .join("last_capture.json")
        );
        // Chỉ tính đường dẫn, không tạo thư mục.
        assert!(!sb.exists());
    }

    #[test]
    fn data_files_under_app_support_darkshot() {
        let home = Path::new("/Users/someone");
        let support = resolve_with(None, Known::Roaming, home);
        assert_eq!(
            config_path_in(&support),
            app_support(home).join("darkshot").join("config.json")
        );
        assert_eq!(
            last_capture_path_in(&support),
            app_support(home).join("darkshot").join("last_capture.json")
        );
        let local = resolve_with(None, Known::Local, home);
        assert_eq!(
            history_dir_in(&local),
            app_support(home).join("darkshot").join("history")
        );
    }

    #[test]
    fn missing_or_bad_home_falls_back_to_temp() {
        let tmp = std::env::temp_dir();
        assert_eq!(home_from(None), tmp);
        assert_eq!(home_from(Some(OsString::new())), tmp);
        assert_eq!(home_from(Some("relative/home".into())), tmp);
        assert_eq!(
            home_from(Some("/Users/someone".into())),
            PathBuf::from("/Users/someone")
        );
    }

    #[test]
    fn public_functions_end_with_darkshot_names() {
        assert!(config_path().ends_with(Path::new("darkshot").join("config.json")));
        assert!(last_capture_path().ends_with(Path::new("darkshot").join("last_capture.json")));
        assert!(history_dir().ends_with(Path::new("darkshot").join("history")));
        assert!(steps_dir().ends_with(Path::new("darkshot").join("steps").join("current")));
        assert!(default_save_dir().ends_with("darkshot"));
        assert!(default_save_dir().is_absolute());
        assert_eq!(appdata_dir(), local_appdata_dir());
    }
}
