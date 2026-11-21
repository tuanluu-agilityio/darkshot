//! Trạng thái ứng dụng và luồng tải lên. Phần lớn là hàm thuần để kiểm thử được.

use crate::lang;
use darkshot_core::actions::{run_action, Action, Clipboard, Ctx, Notifier, Outcome};
use darkshot_core::config::UploadFormat;
use darkshot_core::i18n::Lang;
use darkshot_core::image::Image;
use darkshot_core::naming::LocalTime;
use darkshot_core::upload::Uploader;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;

// Trạng thái khởi động và sổ tải lên nằm ở core (dùng chung với macOS); giữ tên cũ cho chỗ gọi.
pub use darkshot_core::app_state::{
    needs_settings_on_start, should_sync_autostart, startup, AppState, PendingUpload,
    PendingUploads,
};

/// Lưu PNG của các lượt chưa xong vào thư mục lưu của từng lượt, theo ngôn ngữ hiện hành.
/// Lõi ở `darkshot_core::app_state::save_pending`.
pub fn save_pending(
    items: Vec<PendingUpload>,
    now: LocalTime,
    on_saved: &dyn Fn(&Outcome, &Image),
) -> usize {
    darkshot_core::app_state::save_pending(items, now, lang::current(), on_saved)
}

/// Móc nhận kết quả của một lượt tải lên cùng ảnh của lượt đó.
pub type OutcomeHook = Box<dyn Fn(&Outcome, &Image) + Send>;

/// Mọi thứ một lượt tải lên cần; đã là bản chụp riêng, không tham chiếu trạng thái luồng chính.
pub struct UploadJob {
    pub save_dir: PathBuf,
    pub expiry: darkshot_core::config::Expiry,
    pub uploader: Option<Arc<dyn Uploader>>,
    pub clipboard: Arc<dyn Clipboard>,
    pub notifier: Arc<dyn Notifier>,
    pub image: Arc<Image>,
    pub clock: fn() -> LocalTime,
    /// Gọi khi `UploadFailedSaved { needs_settings: true }`.
    pub on_needs_settings: Box<dyn FnOnce() + Send>,
    /// Sổ và mã của lượt này; luồng tải lên rút nó khỏi sổ khi kết thúc.
    pub pending: Option<(Arc<PendingUploads>, u64)>,
    /// Ngôn ngữ của thông báo, chốt lúc tạo lượt.
    pub lang: Lang,
    pub upload_format: UploadFormat,
    pub jpeg_quality: u8,
    /// Gọi nhiều nhất một lần với kết quả của lượt, sau khi lượt đã rút khỏi sổ (ghi lịch sử).
    /// Có sổ: chỉ gọi khi luồng này được trao lượt (`PendingUploads::finish`); nếu lúc thoát đã
    /// lấy lượt (và ghi lưu dự phòng) thì không gọi, tránh hai mục cho một ảnh.
    pub on_outcome: Option<OutcomeHook>,
}

/// Chạy một lượt tải lên trên luồng riêng. Lỗi tạo luồng trả về `Err` (không panic).
pub fn spawn_upload(job: UploadJob) -> Result<JoinHandle<()>, String> {
    std::thread::Builder::new()
        .name("darkshot-upload".into())
        .spawn(move || {
            let ctx = Ctx {
                save_dir: &job.save_dir,
                expiry: job.expiry,
                now: (job.clock)(),
                uploader: job.uploader.as_deref(),
                clipboard: job.clipboard.as_ref(),
                notifier: job.notifier.as_ref(),
                lang: job.lang,
                upload_format: job.upload_format,
                jpeg_quality: job.jpeg_quality,
            };
            let outcome = run_action(&ctx, Action::Upload, &job.image);
            // Lượt đã kết thúc (lên kho, đã lưu dự phòng, hoặc đã báo lỗi): rút khỏi sổ và ghi
            // lịch sử, đúng một lần. Móc chạy khi vẫn giữ khóa trao lượt (móc chỉ đẩy việc vào hàng).
            let record = || {
                if let Some(hook) = &job.on_outcome {
                    hook(&outcome, &job.image);
                }
            };
            match &job.pending {
                Some((reg, id)) => {
                    reg.finish(*id, record);
                }
                None => record(),
            }
            if let Outcome::UploadFailedSaved {
                needs_settings: true,
                ..
            } = outcome
            {
                (job.on_needs_settings)();
            }
        })
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkshot_core::actions::NoticeKind;
    use darkshot_core::config::{Expiry, JPEG_QUALITY_DEFAULT};
    use darkshot_core::upload::UploadError;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Mutex;
    use std::time::Duration;

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

    #[derive(Default)]
    struct FakeNotifier {
        notes: Mutex<Vec<(NoticeKind, String, String)>>,
    }
    impl Notifier for FakeNotifier {
        fn notify(&self, kind: NoticeKind, title: &str, body: &str) {
            self.notes
                .lock()
                .unwrap()
                .push((kind, title.into(), body.into()));
        }
    }
    #[derive(Default)]
    struct FakeClipboard;
    impl Clipboard for FakeClipboard {
        fn set_text(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn set_image(&self, _: &Image) -> Result<(), String> {
            Ok(())
        }
    }
    struct SlowUploader(String);
    impl Uploader for SlowUploader {
        fn upload(&self, _key: &str, _body: &[u8], _ct: &str) -> Result<String, UploadError> {
            std::thread::sleep(Duration::from_millis(200));
            Ok(self.0.clone())
        }
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
    fn img() -> Arc<Image> {
        Arc::new(Image {
            width: 2,
            height: 2,
            bgra: vec![255; 16],
        })
    }
    fn job(
        dir: PathBuf,
        up: Option<Arc<dyn Uploader>>,
        n: Arc<FakeNotifier>,
        cb: Box<dyn FnOnce() + Send>,
    ) -> UploadJob {
        UploadJob {
            save_dir: dir,
            expiry: Expiry::D30,
            uploader: up,
            clipboard: Arc::new(FakeClipboard),
            notifier: n,
            image: img(),
            clock: fixed_now,
            on_needs_settings: cb,
            pending: None,
            lang: Lang::Vi,
            upload_format: UploadFormat::Png,
            jpeg_quality: JPEG_QUALITY_DEFAULT,
            on_outcome: None,
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
    fn save_pending_writes_each_png_to_its_dir() {
        let (d1, d2) = (temp_dir(), temp_dir().join("sub"));
        let items = vec![
            PendingUpload {
                image: pixel(1),
                save_dir: d1.clone(),
            },
            PendingUpload {
                image: pixel(2),
                save_dir: d2.clone(),
            },
            PendingUpload {
                image: pixel(3),
                save_dir: d1.clone(),
            },
        ];
        assert_eq!(save_pending(items, fixed_now(), &|_, _| {}), 3);
        assert!(d1.join("darkshot_2026-11-16_01-02-03.png").exists());
        assert!(d1.join("darkshot_2026-11-16_01-02-03_2.png").exists());
        assert!(d2.join("darkshot_2026-11-16_01-02-03.png").exists());
        assert_eq!(std::fs::read_dir(&d1).unwrap().count(), 2);
    }

    #[test]
    fn finished_upload_leaves_the_registry() {
        let reg = Arc::new(PendingUploads::new());
        let other = reg.add(pixel(9), PathBuf::from("C:/other"));
        for up in [
            Some(Arc::new(SlowUploader("https://x/a".into())) as Arc<dyn Uploader>),
            None, // lỗi cấu hình: đã tự lưu dự phòng
        ] {
            let n = Arc::new(FakeNotifier::default());
            let mut j = job(temp_dir(), up, n, Box::new(|| {}));
            let id = reg.add(j.image.clone(), j.save_dir.clone());
            j.pending = Some((reg.clone(), id));
            spawn_upload(j).unwrap().join().unwrap();
            assert!(!reg.remove(id), "lượt đã xong phải tự rút khỏi sổ");
        }
        // Lượt không liên quan vẫn còn.
        assert!(reg.remove(other));
    }

    #[test]
    fn concurrent_uploads_each_notify() {
        let n = Arc::new(FakeNotifier::default());
        let dir = temp_dir();
        let mk = |link: &str| {
            let up: Arc<dyn Uploader> = Arc::new(SlowUploader(link.into()));
            job(dir.clone(), Some(up), n.clone(), Box::new(|| {}))
        };
        let h1 = spawn_upload(mk("https://x/a")).unwrap();
        let h2 = spawn_upload(mk("https://x/b")).unwrap();
        h1.join().unwrap();
        h2.join().unwrap();
        let notes = n.notes.lock().unwrap();
        assert_eq!(notes.len(), 2);
        assert!(notes.iter().all(|(_, t, _)| t == "Đã chép link"));
        let mut links: Vec<_> = notes.iter().map(|(_, _, b)| b.clone()).collect();
        links.sort();
        assert_eq!(links, vec!["https://x/a", "https://x/b"]);
    }

    #[test]
    fn unconfigured_upload_saves_and_requests_settings() {
        let n = Arc::new(FakeNotifier::default());
        let dir = temp_dir();
        let called = Arc::new(AtomicBool::new(false));
        let c2 = called.clone();
        let h = spawn_upload(job(
            dir.clone(),
            None,
            n.clone(),
            Box::new(move || c2.store(true, Ordering::SeqCst)),
        ))
        .unwrap();
        h.join().unwrap();
        assert!(called.load(Ordering::SeqCst));
        assert!(dir.join("darkshot_2026-11-16_01-02-03.png").exists());
    }

    #[test]
    fn upload_success_does_not_request_settings() {
        let n = Arc::new(FakeNotifier::default());
        let called = Arc::new(AtomicBool::new(false));
        let c2 = called.clone();
        let up: Arc<dyn Uploader> = Arc::new(SlowUploader("https://x/a".into()));
        let h = spawn_upload(job(
            temp_dir(),
            Some(up),
            n,
            Box::new(move || c2.store(true, Ordering::SeqCst)),
        ))
        .unwrap();
        h.join().unwrap();
        assert!(!called.load(Ordering::SeqCst));
    }

    type Seen = Arc<Mutex<Vec<(String, u32)>>>;

    fn outcome_tag(o: &Outcome) -> String {
        match o {
            Outcome::Uploaded(link) => format!("uploaded:{link}"),
            Outcome::UploadFailedSaved { .. } => "fallback".into(),
            Outcome::Saved(_) => "saved".into(),
            other => format!("{other:?}"),
        }
    }

    fn recording_hook(seen: Seen) -> OutcomeHook {
        Box::new(move |o: &Outcome, img: &Image| {
            seen.lock().unwrap().push((outcome_tag(o), img.width));
        })
    }

    #[test]
    fn on_outcome_called_once_with_uploaded() {
        let seen: Seen = Arc::default();
        let up: Arc<dyn Uploader> = Arc::new(SlowUploader("https://x/a".into()));
        let reg = Arc::new(PendingUploads::new());
        let mut j = job(
            temp_dir(),
            Some(up),
            Arc::new(FakeNotifier::default()),
            Box::new(|| {}),
        );
        let id = reg.add(j.image.clone(), j.save_dir.clone());
        j.pending = Some((reg.clone(), id));
        let reg2 = reg.clone();
        let seen2 = seen.clone();
        // Móc chạy sau khi lượt đã rút khỏi sổ.
        j.on_outcome = Some(Box::new(move |o: &Outcome, img: &Image| {
            assert!(!reg2.remove(id), "phải rút khỏi sổ trước khi gọi móc");
            seen2.lock().unwrap().push((outcome_tag(o), img.width));
        }));
        spawn_upload(j).unwrap().join().unwrap();
        assert_eq!(
            *seen.lock().unwrap(),
            vec![("uploaded:https://x/a".to_string(), 2)]
        );
    }

    #[test]
    fn on_outcome_called_once_with_upload_fallback() {
        let seen: Seen = Arc::default();
        let dir = temp_dir();
        let mut j = job(
            dir.clone(),
            None,
            Arc::new(FakeNotifier::default()),
            Box::new(|| {}),
        );
        j.on_outcome = Some(recording_hook(seen.clone()));
        spawn_upload(j).unwrap().join().unwrap();
        assert_eq!(*seen.lock().unwrap(), vec![("fallback".to_string(), 2)]);
        assert!(dir.join("darkshot_2026-11-16_01-02-03.png").exists());
    }

    #[test]
    fn save_pending_calls_hook_for_each_saved_image() {
        let d = temp_dir();
        let items = vec![
            PendingUpload {
                image: pixel(1),
                save_dir: d.clone(),
            },
            PendingUpload {
                image: pixel(2),
                save_dir: d.clone(),
            },
        ];
        let seen: Mutex<Vec<(Outcome, u8)>> = Mutex::default();
        let hook = |o: &Outcome, img: &Image| {
            let o = match o {
                Outcome::UploadFailedSaved {
                    path,
                    needs_settings,
                } => Outcome::UploadFailedSaved {
                    path: path.clone(),
                    needs_settings: *needs_settings,
                },
                _ => Outcome::Failed,
            };
            seen.lock().unwrap().push((o, img.bgra[0]));
        };
        assert_eq!(save_pending(items, fixed_now(), &hook), 2);
        let seen = seen.into_inner().unwrap();
        assert_eq!(
            seen,
            vec![
                (
                    Outcome::UploadFailedSaved {
                        path: d.join("darkshot_2026-11-16_01-02-03.png"),
                        needs_settings: false
                    },
                    1
                ),
                (
                    Outcome::UploadFailedSaved {
                        path: d.join("darkshot_2026-11-16_01-02-03_2.png"),
                        needs_settings: false
                    },
                    2
                ),
            ]
        );
    }

    #[test]
    fn save_pending_skips_hook_when_save_fails() {
        // Thư mục lưu là một TỆP: lưu thất bại, móc không được gọi.
        let f = temp_dir().join("file");
        std::fs::write(&f, b"x").unwrap();
        let items = vec![PendingUpload {
            image: pixel(1),
            save_dir: f,
        }];
        let calls = AtomicU32::new(0);
        let hook = |_: &Outcome, _: &Image| {
            calls.fetch_add(1, Ordering::SeqCst);
        };
        assert_eq!(save_pending(items, fixed_now(), &hook), 0);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    struct GatedUploader(Mutex<std::sync::mpsc::Receiver<()>>);
    impl Uploader for GatedUploader {
        fn upload(&self, _key: &str, _body: &[u8], _ct: &str) -> Result<String, UploadError> {
            let _ = self.0.lock().unwrap().recv_timeout(Duration::from_secs(5));
            Ok("https://x/gated".into())
        }
    }

    #[test]
    fn spawn_upload_records_once_when_drained_first() {
        let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
        let up: Arc<dyn Uploader> = Arc::new(GatedUploader(Mutex::new(go_rx)));
        let reg = Arc::new(PendingUploads::new());
        let n = Arc::new(FakeNotifier::default());
        let mut j = job(temp_dir(), Some(up), n.clone(), Box::new(|| {}));
        let id = reg.add(j.image.clone(), j.save_dir.clone());
        j.pending = Some((reg.clone(), id));
        let calls = Arc::new(AtomicU32::new(0));
        let c2 = calls.clone();
        j.on_outcome = Some(Box::new(move |_: &Outcome, _: &Image| {
            c2.fetch_add(1, Ordering::SeqCst);
        }));
        let h = spawn_upload(j).unwrap();
        // Thoát giành lượt trước khi tải lên xong (lúc đó sẽ ghi dự phòng).
        assert_eq!(reg.drain().len(), 1);
        go_tx.send(()).unwrap();
        h.join().unwrap();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "lượt đã trao cho lúc thoát"
        );
        // Thông báo của lượt tải lên vẫn chạy như cũ.
        assert_eq!(n.notes.lock().unwrap().len(), 1);
    }
}
