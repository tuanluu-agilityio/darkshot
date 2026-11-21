//! Khóa bí mật R2 lưu trong Keychain của macOS (quyết định 136).
//!
//! Tệp cấu hình chỉ giữ dấu `keychain:v1`; khóa thật nằm trong Keychain (mục mật khẩu chung,
//! service `darkshot`, tài khoản cố định). Logic dấu/kiểm tách khỏi Keychain qua `KeyBackend`
//! để kiểm thử không chạm Keychain thật. Lỗi là chuỗi kỹ thuật cho nhật ký, không bao giờ
//! chứa khóa.

use darkshot_core::config::SecretStore;
use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};

/// `errSecItemNotFound` (Security/SecBase.h); `security-framework` không xuất lại hằng này.
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

/// Dấu ghi vào tệp cấu hình thay cho khóa.
pub const MARKER: &[u8] = b"keychain:v1";
/// Service của mục Keychain của ứng dụng.
pub const SERVICE: &str = "darkshot";
/// Tài khoản cố định của mục Keychain.
pub const ACCOUNT: &str = "r2-secret-access-key";

/// Nơi cất khóa thật (Keychain, hoặc bản giả trong kiểm thử).
pub trait KeyBackend: Send + Sync {
    /// Ghi khóa, ghi đè giá trị cũ.
    fn set(&self, secret: &[u8]) -> Result<(), String>;
    /// Đọc khóa; thiếu mục hoặc bị từ chối thì `Err`.
    fn get(&self) -> Result<Vec<u8>, String>;
}

/// Ghi `plain` vào `backend` rồi trả dấu `keychain:v1` (không bao giờ chứa khóa).
pub fn protect_with(backend: &dyn KeyBackend, plain: &[u8]) -> Result<Vec<u8>, String> {
    if plain.is_empty() {
        return Err("refusing to store an empty secret".into());
    }
    backend.set(plain)?;
    Ok(MARKER.to_vec())
}

/// Kiểm dấu `keychain:v1` rồi đọc khóa từ `backend`.
pub fn unprotect_with(backend: &dyn KeyBackend, cipher: &[u8]) -> Result<Vec<u8>, String> {
    // Không lặp lại `cipher` trong lỗi: dữ liệu lạ có thể là khóa rõ dán nhầm.
    if cipher != MARKER {
        return Err("stored secret is not a keychain reference".into());
    }
    let secret = backend.get()?;
    if secret.is_empty() {
        return Err("keychain item is empty".into());
    }
    Ok(secret)
}

/// `SecretStore` dựa trên Keychain. `Debug` chỉ in service và tài khoản, không có khóa.
#[derive(Debug, Clone)]
pub struct KeychainStore {
    service: String,
    account: &'static str,
}

impl KeychainStore {
    /// Mục Keychain của ứng dụng (service `darkshot`).
    pub fn new() -> Self {
        Self::with_service(SERVICE)
    }

    /// Mục Keychain với service khác (cho kiểm thử: `darkshot-test-<pid>`).
    pub fn with_service(service: &str) -> Self {
        KeychainStore {
            service: service.to_string(),
            account: ACCOUNT,
        }
    }

    /// Xóa mục Keychain; không có mục thì coi như đã xóa.
    pub fn delete(&self) -> Result<(), String> {
        match delete_generic_password(&self.service, self.account) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
            Err(e) => Err(format!("keychain delete failed (OSStatus {})", e.code())),
        }
    }
}

impl Default for KeychainStore {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyBackend for KeychainStore {
    fn set(&self, secret: &[u8]) -> Result<(), String> {
        set_generic_password(&self.service, self.account, secret)
            .map_err(|e| format!("keychain write failed (OSStatus {})", e.code()))
    }

    fn get(&self) -> Result<Vec<u8>, String> {
        get_generic_password(&self.service, self.account).map_err(|e| {
            if e.code() == ERR_SEC_ITEM_NOT_FOUND {
                "keychain item not found".to_string()
            } else {
                format!("keychain read failed (OSStatus {})", e.code())
            }
        })
    }
}

impl SecretStore for KeychainStore {
    fn protect(&self, plain: &[u8]) -> Result<Vec<u8>, String> {
        protect_with(self, plain)
    }

    fn unprotect(&self, cipher: &[u8]) -> Result<Vec<u8>, String> {
        unprotect_with(self, cipher)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Chỉ dùng chuỗi giả; không bao giờ đọc khóa thật hay mục Keychain của ứng dụng.
    const FAKE: &[u8] = b"FAKE-SECRET-0123456789";

    #[derive(Default)]
    struct FakeBackend {
        slot: Mutex<Option<Vec<u8>>>,
        deny: bool,
    }

    impl KeyBackend for FakeBackend {
        fn set(&self, secret: &[u8]) -> Result<(), String> {
            if self.deny {
                return Err("denied".into());
            }
            *self.slot.lock().unwrap() = Some(secret.to_vec());
            Ok(())
        }
        fn get(&self) -> Result<Vec<u8>, String> {
            if self.deny {
                return Err("denied".into());
            }
            self.slot
                .lock()
                .unwrap()
                .clone()
                .ok_or_else(|| "missing".to_string())
        }
    }

    fn contains(hay: &[u8], needle: &[u8]) -> bool {
        hay.windows(needle.len()).any(|w| w == needle)
    }

    #[test]
    fn round_trip_through_backend() {
        let b = FakeBackend::default();
        let c = protect_with(&b, FAKE).unwrap();
        assert_eq!(c, MARKER);
        assert!(!contains(&c, FAKE));
        assert_eq!(unprotect_with(&b, &c).unwrap(), FAKE);
    }

    #[test]
    fn protect_overwrites_previous_secret() {
        let b = FakeBackend::default();
        protect_with(&b, b"FAKE-OLD").unwrap();
        let c = protect_with(&b, b"FAKE-NEW").unwrap();
        assert_eq!(unprotect_with(&b, &c).unwrap(), b"FAKE-NEW");
    }

    #[test]
    fn unprotect_rejects_unknown_marker_and_empty() {
        let b = FakeBackend::default();
        protect_with(&b, FAKE).unwrap();
        assert!(unprotect_with(&b, b"").is_err());
        assert!(unprotect_with(&b, b"keychain:v2").is_err());
        assert!(unprotect_with(&b, b"keychain:v1x").is_err());
        assert!(unprotect_with(&b, b"dpapi-blob").is_err());
    }

    #[test]
    fn missing_item_is_an_error() {
        let b = FakeBackend::default();
        assert!(unprotect_with(&b, MARKER).is_err());
    }

    #[test]
    fn denied_backend_is_an_error() {
        let b = FakeBackend {
            deny: true,
            ..Default::default()
        };
        assert!(protect_with(&b, FAKE).is_err());
        assert!(unprotect_with(&b, MARKER).is_err());
    }

    #[test]
    fn empty_secret_is_rejected() {
        let b = FakeBackend::default();
        assert!(protect_with(&b, b"").is_err());
        // Mục rỗng trong Keychain cũng coi như mất khóa.
        b.set(b"").unwrap();
        assert!(unprotect_with(&b, MARKER).is_err());
    }

    #[test]
    fn errors_never_echo_secret_or_input() {
        let denied = FakeBackend {
            deny: true,
            ..Default::default()
        };
        let e = protect_with(&denied, FAKE).unwrap_err();
        assert!(!contains(e.as_bytes(), FAKE), "{e}");
        // Dữ liệu cũ (ví dụ khóa rõ dán nhầm vào cấu hình) không được lặp lại trong lỗi.
        let e = unprotect_with(&FakeBackend::default(), FAKE).unwrap_err();
        assert!(!contains(e.as_bytes(), FAKE), "{e}");
        let e = unprotect_with(&FakeBackend::default(), MARKER).unwrap_err();
        assert!(!contains(e.as_bytes(), FAKE), "{e}");
    }

    #[test]
    fn keychain_store_debug_has_no_secret_and_uses_fixed_names() {
        let s = KeychainStore::new();
        let d = format!("{s:?}");
        assert!(d.contains(SERVICE) && d.contains(ACCOUNT), "{d}");
        assert_eq!(SERVICE, "darkshot");
        assert_eq!(ACCOUNT, "r2-secret-access-key");
        assert_eq!(MARKER, b"keychain:v1");
    }

    /// Xóa mục thử khi kết thúc, kể cả khi khẳng định thất bại.
    struct Cleanup(KeychainStore);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = self.0.delete();
        }
    }

    #[test]
    #[ignore = "chạm Keychain thật; chạy tay: cargo test -p darkshot-macos -- --ignored --test-threads=1"]
    fn keychain_round_trip_and_cleanup() {
        let service = format!("darkshot-test-{}", std::process::id());
        let store = KeychainStore::with_service(&service);
        let guard = Cleanup(store.clone());
        let c = store.protect(b"FAKE-KEYCHAIN-1").unwrap();
        assert_eq!(c, MARKER);
        assert_eq!(store.unprotect(&c).unwrap(), b"FAKE-KEYCHAIN-1");
        let c = store.protect(b"FAKE-KEYCHAIN-2").unwrap();
        assert_eq!(store.unprotect(&c).unwrap(), b"FAKE-KEYCHAIN-2");
        drop(guard);
        assert!(store.unprotect(MARKER).is_err(), "mục thử phải đã bị xóa");
    }
}
