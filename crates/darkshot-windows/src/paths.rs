//! Đường dẫn và giờ địa phương. Không panic: thiếu thư mục hệ thống thì dùng phương án dự phòng.

use darkshot_core::dirs::{self, history_dir_in, last_capture_path_in, steps_dir_in};
pub use darkshot_core::dirs::{config_path_in, profile_override, Known};
use darkshot_core::naming::LocalTime;
use std::path::{Path, PathBuf};
use windows::core::GUID;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FOLDERID_Pictures, FOLDERID_RoamingAppData, SHGetKnownFolderPath,
    KF_FLAG_DEFAULT,
};

fn known_folder(id: &GUID) -> Option<PathBuf> {
    // SAFETY: SHGetKnownFolderPath trả về chuỗi do COM cấp; ta sao chép rồi giải phóng bằng CoTaskMemFree.
    unsafe {
        let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s.filter(|s| !s.is_empty()).map(PathBuf::from)
    }
}

/// Đường dẫn của một thư mục hệ thống: có thư mục cách ly thì `<dir>\Roaming|Local|Pictures`
/// (không tạo thư mục); không có thì hỏi hệ thống.
pub fn resolve(override_dir: Option<&Path>, known: Known) -> PathBuf {
    if let Some(p) = dirs::sandboxed(override_dir, known) {
        return p;
    }
    match known {
        Known::Roaming => known_folder(&FOLDERID_RoamingAppData)
            .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
            .unwrap_or_else(std::env::temp_dir),
        Known::Local => known_folder(&FOLDERID_LocalAppData)
            .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
            .unwrap_or_else(std::env::temp_dir),
        Known::Pictures => known_folder(&FOLDERID_Pictures)
            .or_else(|| std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("Pictures")))
            .unwrap_or_else(std::env::temp_dir),
    }
}

/// Thư mục gốc `%APPDATA%` (Roaming).
pub fn appdata_dir() -> PathBuf {
    resolve(profile_override().as_deref(), Known::Roaming)
}

/// Thư mục gốc `%LOCALAPPDATA%` (máy cục bộ).
pub fn local_appdata_dir() -> PathBuf {
    resolve(profile_override().as_deref(), Known::Local)
}

/// Thư mục gốc `Pictures` của người dùng.
pub fn pictures_dir() -> PathBuf {
    resolve(profile_override().as_deref(), Known::Pictures)
}

/// `%LOCALAPPDATA%\darkshot\history`
pub fn history_dir() -> PathBuf {
    history_dir_in(&local_appdata_dir())
}

/// `%LOCALAPPDATA%\darkshot\steps\current`: phiên chụp từng bước (theo `DARKSHOT_PROFILE_DIR`).
pub fn steps_dir() -> PathBuf {
    steps_dir_in(&local_appdata_dir())
}

/// `%APPDATA%\darkshot\config.json`
pub fn config_path() -> PathBuf {
    config_path_in(&appdata_dir())
}

/// `%APPDATA%\darkshot\last_capture.json` (theo `DARKSHOT_PROFILE_DIR`).
pub fn last_capture_path() -> PathBuf {
    last_capture_path_in(&appdata_dir())
}

/// `Pictures\darkshot`
pub fn default_save_dir() -> PathBuf {
    pictures_dir().join("darkshot")
}

pub fn local_now() -> LocalTime {
    // SAFETY: GetLocalTime chỉ trả về giá trị.
    let t = unsafe { GetLocalTime() };
    LocalTime {
        year: t.wYear,
        month: t.wMonth as u8,
        day: t.wDay as u8,
        hour: t.wHour as u8,
        minute: t.wMinute as u8,
        second: t.wSecond as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_darkshot() {
        assert!(config_path().ends_with("darkshot/config.json"));
        assert!(default_save_dir().ends_with("darkshot"));
    }

    #[test]
    fn sandbox_resolves_under_profile_dir() {
        let dir = Path::new("D:/sandbox");
        assert_eq!(
            resolve(Some(dir), Known::Roaming),
            PathBuf::from("D:/sandbox").join("Roaming")
        );
        assert_eq!(
            resolve(Some(dir), Known::Local),
            PathBuf::from("D:/sandbox").join("Local")
        );
        assert_eq!(
            resolve(Some(dir), Known::Pictures),
            PathBuf::from("D:/sandbox").join("Pictures")
        );
    }

    #[test]
    fn resolve_does_not_create_directories() {
        let dir = std::env::temp_dir().join(format!("darkshot_paths_{}_x", std::process::id()));
        let _ = resolve(Some(&dir), Known::Local);
        assert!(!dir.exists());
    }

    #[test]
    fn no_override_uses_system_folders() {
        for k in [Known::Roaming, Known::Local, Known::Pictures] {
            let p = resolve(None, k);
            assert!(p.is_absolute(), "{k:?}");
        }
        assert_ne!(resolve(None, Known::Roaming), resolve(None, Known::Local));
    }

    #[test]
    fn history_dir_is_under_local_darkshot() {
        assert_eq!(
            history_dir_in(Path::new("C:/L")),
            PathBuf::from("C:/L").join("darkshot").join("history")
        );
        assert!(history_dir().ends_with("darkshot/history"));
    }

    #[test]
    fn steps_dir_is_under_local_darkshot() {
        assert_eq!(
            steps_dir_in(Path::new("C:/L")),
            PathBuf::from("C:/L")
                .join("darkshot")
                .join("steps")
                .join("current")
        );
        assert!(steps_dir().ends_with("darkshot/steps/current"));
        assert_eq!(
            steps_dir_in(&resolve(Some(Path::new("D:/sb")), Known::Local)),
            PathBuf::from("D:/sb/Local/darkshot/steps/current")
        );
    }

    #[test]
    fn last_capture_sits_next_to_config() {
        let root = Path::new("C:/R");
        let p = last_capture_path_in(root);
        assert_eq!(p.parent(), config_path_in(root).parent());
        assert_eq!(p.file_name().unwrap(), "last_capture.json");
        assert_eq!(
            last_capture_path_in(&resolve(Some(Path::new("D:/sb")), Known::Roaming)),
            PathBuf::from("D:/sb")
                .join("Roaming")
                .join("darkshot")
                .join("last_capture.json")
        );
        assert!(last_capture_path().ends_with("darkshot/last_capture.json"));
    }

    #[test]
    fn local_now_is_plausible() {
        let t = local_now();
        assert!(t.year >= 2026 && (1..=12).contains(&t.month));
    }
}
