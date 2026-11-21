//! Trạng thái khởi động và sổ tải lên chưa xong, dùng chung mọi nền tảng. Phần lớn là hàm thuần
//! để kiểm thử được.

use crate::actions::{run_action, Action, Clipboard, Ctx, NoticeKind, Notifier, Outcome};
use crate::config::{Config, ConfigError, Expiry, SecretStore, UploadFormat, JPEG_QUALITY_DEFAULT};
use crate::i18n::{t, Lang, Msg};
use crate::image::Image;
use crate::naming::LocalTime;
use crate::update::Release;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigNotice {
    Corrupt,
    SecretLost,
    Unreadable,
}

impl ConfigNotice {
    pub fn message(self, lang: Lang) -> String {
        let m = match self {
            ConfigNotice::Corrupt => Msg::NoticeConfigCorrupt,
            ConfigNotice::SecretLost => Msg::NoticeSecretLost,
            ConfigNotice::Unreadable => Msg::NoticeConfigUnreadable,
        };
        t(lang, m).to_string()
    }
}

pub struct AppState {
    pub config: Config,
    pub config_notice: Option<ConfigNotice>,
    pub overlay_open: bool,
    /// Bản mới nhất đã tìm thấy (mới hơn bản đang chạy); menu khay đọc giá trị này.
    pub update: Option<Release>,
}

/// Đọc cấu hình lúc khởi động. Không bao giờ ghi file.
pub fn startup(
    path: &Path,
    store: &dyn SecretStore,
    default_save_dir: &Path,
    os_lang: Lang,
) -> AppState {
    let defaults = || Config::defaults(default_save_dir, os_lang);
    let (config, config_notice) = match crate::config::load(path, store, default_save_dir, os_lang)
    {
        Ok(l) if l.secret_lost => (l.config, Some(ConfigNotice::SecretLost)),
        Ok(l) => (l.config, None),
        Err(ConfigError::Corrupt(_)) => (defaults(), Some(ConfigNotice::Corrupt)),
        Err(_) => (defaults(), Some(ConfigNotice::Unreadable)),
    };
    AppState {
        config,
        config_notice,
        overlay_open: false,
        update: None,
    }
}

/// Có đồng bộ khóa Run theo cấu hình không. Cấu hình hỏng hoặc không đọc được thì giá trị
/// `autostart` thật là chưa biết: giữ nguyên mọi giá trị Run (kể cả của bản cũ).
pub fn should_sync_autostart(notice: Option<ConfigNotice>) -> bool {
    match notice {
        None | Some(ConfigNotice::SecretLost) => true,
        Some(ConfigNotice::Corrupt) | Some(ConfigNotice::Unreadable) => false,
    }
}

pub fn needs_settings_on_start(s: &AppState, config_file_exists: bool) -> bool {
    !config_file_exists || s.config_notice.is_some()
}

/// Một lượt tải lên chưa xong: ảnh và thư mục lưu của chính lượt đó.
pub struct PendingUpload {
    pub image: Arc<Image>,
    pub save_dir: PathBuf,
}

/// Sổ các lượt tải lên đang chạy, để lưu ảnh vào máy nếu ứng dụng thoát giữa chừng.
/// Dùng được từ nhiều luồng; `Mutex` bị poison vẫn dùng tiếp, không panic.
///
/// Hai khóa: `inner` giữ danh sách (giữ rất ngắn); `handoff` là khóa trao lượt, giữ suốt
/// `finish` (kể cả khi chạy móc) và suốt `drain`, nên mỗi lượt được trao đúng một lần: hoặc cho
/// luồng tải lên (`finish`), hoặc cho việc lưu lúc thoát (`drain`). Thứ tự khóa: `handoff` rồi
/// `inner`. Móc trong `finish` vẫn gọi được `add`/`remove` (chỉ cần `inner`).
pub struct PendingUploads {
    handoff: Mutex<()>,
    inner: Mutex<PendingInner>,
}

struct PendingInner {
    next_id: u64,
    items: Vec<(u64, PendingUpload)>,
}

impl Default for PendingUploads {
    fn default() -> Self {
        Self::new()
    }
}

impl PendingUploads {
    pub const fn new() -> Self {
        PendingUploads {
            handoff: Mutex::new(()),
            inner: Mutex::new(PendingInner {
                next_id: 0,
                items: Vec::new(),
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, PendingInner> {
        // Sổ chỉ là danh sách; một luồng panic giữa chừng không làm nó sai, nên dùng tiếp.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Ghi một lượt vào sổ; trả mã để rút ra sau.
    pub fn add(&self, image: Arc<Image>, save_dir: PathBuf) -> u64 {
        let mut g = self.lock();
        let id = g.next_id;
        g.next_id = g.next_id.wrapping_add(1);
        g.items.push((id, PendingUpload { image, save_dir }));
        id
    }

    /// Rút một lượt khỏi sổ; `false` nếu nó không còn trong sổ.
    pub fn remove(&self, id: u64) -> bool {
        let mut g = self.lock();
        match g.items.iter().position(|(i, _)| *i == id) {
            Some(pos) => {
                g.items.remove(pos);
                true
            }
            None => false,
        }
    }

    fn lock_handoff(&self) -> std::sync::MutexGuard<'_, ()> {
        self.handoff.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Trao lượt `id` đúng một lần: nếu còn trong sổ thì rút ra và chạy `f` trong khi vẫn giữ khóa
    /// trao lượt, trả `true`; nếu đã bị `drain` (lúc thoát) thì không chạy `f`, trả `false`.
    /// `drain` gọi trong lúc `f` chạy sẽ chờ `f` xong.
    pub fn finish(&self, id: u64, f: impl FnOnce()) -> bool {
        let _turn = self.lock_handoff();
        if !self.remove(id) {
            return false;
        }
        f();
        true
    }

    /// Lấy ra mọi lượt còn trong sổ (theo thứ tự thêm vào), sổ trở thành rỗng.
    /// Chờ mọi `finish` đang chạy móc xong trước khi lấy.
    pub fn drain(&self) -> Vec<PendingUpload> {
        let _turn = self.lock_handoff();
        std::mem::take(&mut self.lock().items)
            .into_iter()
            .map(|(_, p)| p)
            .collect()
    }
}

/// Bỏ qua mọi thông báo: lúc thoát hoặc tắt máy không còn ai để báo.
struct SilentNotifier;
impl Notifier for SilentNotifier {
    fn notify(&self, _: NoticeKind, _: &str, _: &str) {}
}

/// Không đụng clipboard của người dùng khi lưu lúc thoát. Mang ngôn ngữ để báo lỗi đúng tiếng.
struct NoClipboard(Lang);
impl Clipboard for NoClipboard {
    fn set_text(&self, _: &str) -> Result<(), String> {
        Err(t(self.0, Msg::ClipboardUnusedOnExit).into())
    }
    fn set_image(&self, _: &Image) -> Result<(), String> {
        Err(t(self.0, Msg::ClipboardUnusedOnExit).into())
    }
}

/// Lưu PNG của các lượt chưa xong vào thư mục lưu của từng lượt. Chỉ ghi file, không thông báo,
/// không chạm clipboard, không mạng. Trả số ảnh đã lưu được.
/// Mỗi ảnh lưu được thì gọi `on_saved` (đồng bộ) với `UploadFailedSaved` (lưu dự phòng).
/// `lang` là ngôn ngữ hiện hành của lớp giao diện (lớp Windows truyền `lang::current()`).
pub fn save_pending(
    items: Vec<PendingUpload>,
    now: LocalTime,
    lang: Lang,
    on_saved: &dyn Fn(&Outcome, &Image),
) -> usize {
    items
        .iter()
        .filter(|p| {
            let ctx = Ctx {
                save_dir: &p.save_dir,
                expiry: Expiry::Keep,
                now,
                uploader: None,
                clipboard: &NoClipboard(lang),
                notifier: &SilentNotifier,
                lang,
                upload_format: UploadFormat::Png,
                jpeg_quality: JPEG_QUALITY_DEFAULT,
            };
            match run_action(&ctx, Action::Save, &p.image) {
                Outcome::Saved(path) => {
                    on_saved(
                        &Outcome::UploadFailedSaved {
                            path,
                            needs_settings: false,
                        },
                        &p.image,
                    );
                    true
                }
                _ => false,
            }
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::R2Config;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::time::Duration;

    struct XorStore;
    impl SecretStore for XorStore {
        fn protect(&self, plain: &[u8]) -> Result<Vec<u8>, String> {
            Ok(plain.iter().map(|b| b ^ 0x5A).collect())
        }
        fn unprotect(&self, cipher: &[u8]) -> Result<Vec<u8>, String> {
            Ok(cipher.iter().map(|b| b ^ 0x5A).collect())
        }
    }
    struct FailStore;
    impl SecretStore for FailStore {
        fn protect(&self, _: &[u8]) -> Result<Vec<u8>, String> {
            Err("fail".into())
        }
        fn unprotect(&self, _: &[u8]) -> Result<Vec<u8>, String> {
            Err("fail".into())
        }
    }

    static COUNTER: AtomicU32 = AtomicU32::new(0);
    fn temp_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let d = std::env::temp_dir().join(format!("darkshot_app_{}_{t}_{n}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn complete_config() -> Config {
        let mut c = Config::defaults(Path::new("C:/pics"), Lang::Vi);
        c.r2 = R2Config {
            account_id: "acc123".into(),
            access_key_id: "AKIDEXAMPLE".into(),
            secret_access_key: "TOP-SECRET-VALUE".into(),
            bucket: "bucket".into(),
            public_base_url: "https://img.example.com".into(),
        };
        c
    }

    #[test]
    fn first_run_opens_settings() {
        let missing = temp_dir().join("config.json");
        let s = startup(&missing, &XorStore, Path::new("C:/pics"), Lang::Vi);
        assert!(needs_settings_on_start(&s, false) && s.config_notice.is_none());
        assert!(!missing.exists());
    }

    #[test]
    fn corrupt_config_uses_defaults_and_does_not_overwrite() {
        let path = temp_dir().join("config.json");
        std::fs::write(&path, "{not json").unwrap();
        let s = startup(&path, &XorStore, Path::new("C:/pics"), Lang::Vi);
        assert_eq!(s.config, Config::defaults(Path::new("C:/pics"), Lang::Vi));
        assert_eq!(s.config_notice, Some(ConfigNotice::Corrupt));
        assert!(needs_settings_on_start(&s, true));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{not json");
    }

    #[test]
    fn lost_secret_opens_settings_with_notice() {
        let path = temp_dir().join("config.json");
        crate::config::save(&path, &complete_config(), &XorStore).unwrap();
        let s = startup(&path, &FailStore, Path::new("C:/pics"), Lang::Vi);
        assert_eq!(s.config_notice, Some(ConfigNotice::SecretLost));
        assert!(s
            .config_notice
            .unwrap()
            .message(Lang::Vi)
            .contains("Secret Access Key"));
        assert!(needs_settings_on_start(&s, true));
        assert!(s.config.r2.secret_access_key.is_empty());
    }

    #[test]
    fn corrupt_or_missing_config_uses_os_language() {
        let path = temp_dir().join("config.json");
        std::fs::write(&path, "{not json").unwrap();
        let s = startup(&path, &XorStore, Path::new("C:/pics"), Lang::En);
        assert_eq!(s.config.language, Lang::En);
        assert_eq!(s.config_notice, Some(ConfigNotice::Corrupt));

        let missing = temp_dir().join("config.json");
        let s = startup(&missing, &XorStore, Path::new("C:/pics"), Lang::En);
        assert_eq!(s.config.language, Lang::En);
        assert_eq!(s.config_notice, None);

        let path = temp_dir().join("config.json");
        std::fs::write(&path, "{}").unwrap();
        let s = startup(&path, &XorStore, Path::new("C:/pics"), Lang::En);
        assert_eq!(s.config.language, Lang::Vi);
        assert_eq!(s.config_notice, None);
    }

    #[test]
    fn config_notice_messages_differ_by_language() {
        for n in [
            ConfigNotice::Corrupt,
            ConfigNotice::SecretLost,
            ConfigNotice::Unreadable,
        ] {
            assert_ne!(n.message(Lang::Vi), n.message(Lang::En));
        }
    }

    #[test]
    fn autostart_sync_only_when_config_is_known() {
        assert!(should_sync_autostart(None));
        assert!(should_sync_autostart(Some(ConfigNotice::SecretLost)));
        assert!(!should_sync_autostart(Some(ConfigNotice::Corrupt)));
        assert!(!should_sync_autostart(Some(ConfigNotice::Unreadable)));
    }

    #[test]
    fn complete_config_starts_quietly() {
        let path = temp_dir().join("config.json");
        crate::config::save(&path, &complete_config(), &XorStore).unwrap();
        let s = startup(&path, &XorStore, Path::new("C:/pics"), Lang::Vi);
        assert_eq!(s.config, complete_config());
        assert!(!needs_settings_on_start(&s, true));
        assert!(!s.overlay_open);
    }

    fn fixed_now() -> LocalTime {
        LocalTime {
            year: 2026,
            month: 11,
            day: 16,
            hour: 1,
            minute: 2,
            second: 3,
        }
    }
    fn pixel(v: u8) -> Arc<Image> {
        Arc::new(Image {
            width: 1,
            height: 1,
            bgra: vec![v, v, v, 255],
        })
    }

    #[test]
    fn pending_add_remove_drain() {
        let reg = PendingUploads::new();
        let (a, b, c) = (pixel(1), pixel(2), pixel(3));
        let ia = reg.add(a.clone(), PathBuf::from("C:/a"));
        let ib = reg.add(b.clone(), PathBuf::from("C:/b"));
        let ic = reg.add(c.clone(), PathBuf::from("C:/c"));
        assert!(ia != ib && ib != ic && ia != ic);
        assert!(reg.remove(ib));
        // Rút hai lần: lần sau báo không còn.
        assert!(!reg.remove(ib));
        let left = reg.drain();
        assert_eq!(left.len(), 2);
        assert!(Arc::ptr_eq(&left[0].image, &a) && left[0].save_dir == Path::new("C:/a"));
        assert!(Arc::ptr_eq(&left[1].image, &c) && left[1].save_dir == Path::new("C:/c"));
        // Sau drain sổ rỗng; rút một lượt đã bị drain không làm gì.
        assert!(reg.drain().is_empty());
        assert!(!reg.remove(ia));
        // Mã mới không trùng mã cũ.
        let id = reg.add(pixel(4), PathBuf::from("C:/d"));
        assert!(id != ia && id != ib && id != ic);
        assert_eq!(reg.drain().len(), 1);
    }

    #[test]
    fn pending_survives_poisoned_mutex() {
        let reg = Arc::new(PendingUploads::new());
        let id = reg.add(pixel(1), PathBuf::from("C:/a"));
        let r2 = reg.clone();
        let _ = std::thread::spawn(move || {
            let _g = r2.inner.lock().unwrap();
            panic!("poison");
        })
        .join();
        assert!(reg.inner.is_poisoned());
        let id2 = reg.add(pixel(2), PathBuf::from("C:/b"));
        assert!(reg.remove(id));
        assert!(!reg.remove(id));
        let left = reg.drain();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].save_dir, Path::new("C:/b"));
        assert!(!reg.remove(id2));
    }

    #[test]
    fn finish_after_drain_skips_hook() {
        let reg = PendingUploads::new();
        let id = reg.add(pixel(1), PathBuf::from("C:/a"));
        assert_eq!(reg.drain().len(), 1);
        let ran = AtomicBool::new(false);
        assert!(!reg.finish(id, || ran.store(true, Ordering::SeqCst)));
        assert!(!ran.load(Ordering::SeqCst));
    }

    #[test]
    fn drain_after_finish_sees_nothing() {
        let reg = PendingUploads::new();
        let id = reg.add(pixel(1), PathBuf::from("C:/a"));
        let runs = AtomicU32::new(0);
        assert!(reg.finish(id, || {
            runs.fetch_add(1, Ordering::SeqCst);
        }));
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        assert!(reg.drain().is_empty());
        // Lần thứ hai: lượt đã trao, không chạy lại.
        assert!(!reg.finish(id, || {
            runs.fetch_add(1, Ordering::SeqCst);
        }));
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn finish_runs_hook_under_lock() {
        let reg = Arc::new(PendingUploads::new());
        let id = reg.add(pixel(1), PathBuf::from("C:/a"));
        let done = Arc::new(AtomicBool::new(false));
        let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
        let (r2, d2) = (reg.clone(), done.clone());
        let h = std::thread::spawn(move || {
            r2.finish(id, || {
                started_tx.send(()).unwrap();
                std::thread::sleep(Duration::from_millis(100));
                d2.store(true, Ordering::SeqCst);
            })
        });
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        // `f` đang chạy: drain phải chờ nó xong và không thấy lượt.
        let left = reg.drain();
        assert!(
            done.load(Ordering::SeqCst),
            "drain trả về trước khi móc xong"
        );
        assert!(left.is_empty());
        assert!(h.join().unwrap());
    }

    #[test]
    fn save_pending_writes_png_and_reports_each_saved_image() {
        let d = temp_dir();
        let items = vec![
            PendingUpload {
                image: pixel(1),
                save_dir: d.clone(),
            },
            PendingUpload {
                image: pixel(2),
                save_dir: d.join("sub"),
            },
        ];
        let reported = AtomicU32::new(0);
        let hook = |o: &Outcome, _: &Image| {
            assert!(matches!(
                o,
                Outcome::UploadFailedSaved {
                    needs_settings: false,
                    ..
                }
            ));
            reported.fetch_add(1, Ordering::SeqCst);
        };
        assert_eq!(save_pending(items, fixed_now(), Lang::En, &hook), 2);
        assert_eq!(reported.load(Ordering::SeqCst), 2);
        assert!(d.join("darkshot_2026-11-16_01-02-03.png").exists());
        assert!(d
            .join("sub")
            .join("darkshot_2026-11-16_01-02-03.png")
            .exists());
    }
}
