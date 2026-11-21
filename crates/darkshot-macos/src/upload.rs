//! Tải lên R2 và an toàn khi thoát (Task 108). Không gọi AppKit: kiểm thử được bằng `Uploader`,
//! `Clipboard`, `Notifier` giả.
//!
//! Bất biến "không mất ảnh":
//! - Mỗi lượt vào sổ `PendingUploads` TRƯỚC khi luồng tải lên chạy (`start`); luồng chỉ rút khỏi
//!   sổ (`finish`) sau khi `run_action` đã xong (thành công, hoặc đã lưu dự phòng vào thư mục lưu).
//! - Thoát: `save_on_exit` `drain` sổ rồi ghi PNG từng lượt còn dở vào thư mục lưu của lượt đó
//!   (core: `save_pending`). Chỉ ghi tệp; không mạng, không clipboard, không thông báo.
//! - Mỗi lượt được trao đúng một lần (`finish` hoặc `drain`; thứ tự khóa `handoff` rồi `inner`
//!   nằm trong core), nên một ảnh không bị ghi hai lần bởi hai đường này.
//! - Không tạo được luồng: rút khỏi sổ rồi lưu ngay (trừ khi lúc thoát đã lấy lượt đó).
//!
//! Khóa R2 chỉ nằm trong `R2Config` của cấu hình đã nạp (đã qua Keychain) và đi thẳng vào
//! `R2Uploader`; module này không đọc, không ghi log, không đưa nó vào thông báo hay `Debug`.

use darkshot_core::actions::{run_action, Action, Clipboard, Ctx, NoticeKind, Notifier, Outcome};
use darkshot_core::app_flow::remaining;
use darkshot_core::app_state::{save_pending, PendingUploads};
use darkshot_core::config::{Config, Expiry, UploadFormat};
use darkshot_core::geometry::Rect;
use darkshot_core::i18n::{self, t, Lang, Msg};
use darkshot_core::image::Image;
use darkshot_core::naming::LocalTime;
use darkshot_core::upload::{R2Uploader, Uploader};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Sổ lượt tải lên của cả tiến trình.
pub fn registry() -> Arc<PendingUploads> {
    static REGISTRY: OnceLock<Arc<PendingUploads>> = OnceLock::new();
    REGISTRY
        .get_or_init(|| Arc::new(PendingUploads::new()))
        .clone()
}

/// Phần cấu hình một lượt tải lên cần (không chứa khóa R2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UploadParams {
    pub save_dir: PathBuf,
    pub expiry: Expiry,
    pub upload_format: UploadFormat,
    pub jpeg_quality: u8,
    pub r2_complete: bool,
}

impl UploadParams {
    pub fn from_config(c: &Config) -> UploadParams {
        UploadParams {
            save_dir: c.save_dir.clone(),
            expiry: c.expiry,
            upload_format: c.upload_format,
            jpeg_quality: c.jpeg_quality,
            r2_complete: c.r2.is_complete(),
        }
    }
}

/// Bộ tải lên theo cấu hình: `None` khi R2 chưa đủ (core lưu ảnh và báo cần thiết lập).
pub fn uploader_for(c: &Config) -> Option<Arc<dyn Uploader>> {
    if c.r2.is_complete() {
        Some(Arc::new(R2Uploader::new(&c.r2)))
    } else {
        None
    }
}

/// Việc cần làm thêm sau một lượt, ngoài thông báo của `run_action`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FollowUp {
    None,
    /// Mở Tùy chọn tab R2 (`OpenReason::NeedsR2`).
    OpenOptions,
}

pub fn follow_up(o: &Outcome) -> FollowUp {
    match o {
        Outcome::UploadFailedSaved {
            needs_settings: true,
            ..
        } => FollowUp::OpenOptions,
        _ => FollowUp::None,
    }
}

/// Các thứ một lượt tải lên cần ngoài ảnh.
pub struct Deps {
    pub uploader: Option<Arc<dyn Uploader>>,
    pub clipboard: Arc<dyn Clipboard>,
    pub notifier: Arc<dyn Notifier>,
    pub clock: fn() -> LocalTime,
    pub lang: Lang,
    /// Gọi (trên luồng tải lên) khi cần mở Tùy chọn; phía gọi tự chuyển về luồng chính.
    pub on_needs_settings: Box<dyn FnOnce() + Send>,
}

/// Một lượt đã chốt mọi thứ; đã là bản riêng, không tham chiếu trạng thái luồng chính.
pub struct UploadJob {
    pub params: UploadParams,
    pub deps: Deps,
    pub image: Arc<Image>,
    pub pending: (Arc<PendingUploads>, u64),
}

/// Chạy một lượt trên luồng `darkshot-upload`. Lỗi tạo luồng trả `Err` (không panic).
pub fn spawn_upload(job: UploadJob) -> Result<JoinHandle<()>, String> {
    std::thread::Builder::new()
        .name("darkshot-upload".into())
        .spawn(move || run_job(job))
        .map_err(|e| e.to_string())
}

fn run_job(job: UploadJob) {
    let ctx = Ctx {
        save_dir: &job.params.save_dir,
        expiry: job.params.expiry,
        now: (job.deps.clock)(),
        uploader: job.deps.uploader.as_deref(),
        clipboard: job.deps.clipboard.as_ref(),
        notifier: job.deps.notifier.as_ref(),
        lang: job.deps.lang,
        upload_format: job.params.upload_format,
        jpeg_quality: job.params.jpeg_quality,
    };
    let outcome = run_action(&ctx, Action::Upload, &job.image);
    // Lượt đã kết thúc (lên kho hoặc đã lưu dự phòng hoặc đã báo lỗi): rút khỏi sổ. Nếu lúc thoát
    // đã lấy lượt này (và lưu ảnh) thì `finish` trả false; không làm gì thêm.
    let (reg, id) = &job.pending;
    reg.finish(*id, || {});
    if follow_up(&outcome) == FollowUp::OpenOptions {
        (job.deps.on_needs_settings)();
    }
}

/// Bắt đầu một lượt: ghi vào sổ TRƯỚC, rồi chạy trên luồng riêng. Trả tay cầm luồng (cho test);
/// `None` khi không tạo được luồng (ảnh đã được lưu và báo).
pub fn start(
    reg: &Arc<PendingUploads>,
    params: UploadParams,
    deps: Deps,
    image: Arc<Image>,
) -> Option<JoinHandle<()>> {
    start_with(reg, params, deps, image, spawn_upload)
}

/// Như `start` nhưng nhận hàm tạo luồng (để test đường lỗi).
pub fn start_with(
    reg: &Arc<PendingUploads>,
    params: UploadParams,
    deps: Deps,
    image: Arc<Image>,
    spawn: impl FnOnce(UploadJob) -> Result<JoinHandle<()>, String>,
) -> Option<JoinHandle<()>> {
    let id = reg.add(image.clone(), params.save_dir.clone());
    // Bản giữ lại cho đường lỗi (job bị chuyển đi khi tạo luồng).
    let keep_notifier = deps.notifier.clone();
    let keep_clipboard = deps.clipboard.clone();
    let (keep_clock, keep_lang) = (deps.clock, deps.lang);
    let keep_dir = params.save_dir.clone();
    let keep_image = image.clone();
    let job = UploadJob {
        params,
        deps,
        image,
        pending: (reg.clone(), id),
    };
    match spawn(job) {
        Ok(h) => Some(h),
        Err(e) => {
            // Không luồng nào giữ lượt này. Còn trong sổ thì rút và lưu ngay; đã bị lấy lúc thoát
            // (ảnh đã lưu) thì thôi, tránh ghi hai bản.
            if reg.remove(id) {
                keep_notifier.notify(
                    NoticeKind::Error,
                    t(keep_lang, Msg::UploadStartFailedTitle),
                    &i18n::upload_start_failed_body(keep_lang, &e),
                );
                let ctx = Ctx {
                    save_dir: &keep_dir,
                    expiry: Expiry::Keep,
                    now: keep_clock(),
                    uploader: None,
                    clipboard: keep_clipboard.as_ref(),
                    notifier: keep_notifier.as_ref(),
                    lang: keep_lang,
                    upload_format: UploadFormat::Png,
                    jpeg_quality: darkshot_core::config::JPEG_QUALITY_DEFAULT,
                };
                let _ = run_action(&ctx, Action::Save, &keep_image);
            }
            None
        }
    }
}

/// Ảnh cho một lượt tải lên: cắt theo `rect`; khi `rect` phủ cả ảnh (chụp toàn màn hình) thì dùng
/// luôn ảnh gốc, không sao chép thêm hàng chục MB trên luồng chính.
pub fn image_for_upload(image: Image, rect: Rect) -> Image {
    let whole = rect.x == 0
        && rect.y == 0
        && i64::from(rect.w) == i64::from(image.width)
        && i64::from(rect.h) == i64::from(image.height);
    if whole {
        image
    } else {
        image.crop(rect)
    }
}

/// Chờ tới khi `busy()` hết đúng hoặc hết hạn chót `deadline` (hạn chót chung `EXIT_BUDGET`).
/// Trả `true` nếu đã rảnh. Không bao giờ chờ quá hạn chót.
pub fn wait_idle(deadline: Instant, busy: impl Fn() -> bool) -> bool {
    loop {
        if !busy() {
            return true;
        }
        let left = remaining(deadline, Instant::now());
        if left.is_zero() {
            return false;
        }
        std::thread::sleep(left.min(Duration::from_millis(20)));
    }
}

/// Lúc thoát: lấy mọi lượt còn dở khỏi sổ và lưu PNG vào thư mục lưu của từng lượt. Chỉ ghi tệp.
/// Trả số ảnh đã lưu. Gọi lần hai vô hại (sổ rỗng).
pub fn save_on_exit(reg: &PendingUploads, now: LocalTime, lang: Lang) -> usize {
    save_pending(reg.drain(), now, lang, &|_, _| {})
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkshot_core::upload::UploadError;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// Thư mục tạm riêng của một test; dọn khi hết phạm vi.
    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> TempDir {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let d = std::env::temp_dir().join(format!("darkshot_up108_{}_{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            TempDir(d)
        }
        fn files(&self) -> usize {
            std::fs::read_dir(&self.0)
                .map(|r| r.filter_map(Result::ok).count())
                .unwrap_or(0)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn now() -> LocalTime {
        LocalTime {
            year: 2026,
            month: 11,
            day: 21,
            hour: 10,
            minute: 0,
            second: 0,
        }
    }

    fn img() -> Arc<Image> {
        Arc::new(Image {
            width: 2,
            height: 2,
            bgra: vec![200; 16],
        })
    }

    #[derive(Default)]
    struct FakeNotifier(Mutex<Vec<(NoticeKind, String, String)>>);
    impl Notifier for FakeNotifier {
        fn notify(&self, k: NoticeKind, title: &str, body: &str) {
            self.0.lock().unwrap().push((k, title.into(), body.into()));
        }
    }
    impl FakeNotifier {
        fn kinds(&self) -> Vec<NoticeKind> {
            self.0.lock().unwrap().iter().map(|n| n.0).collect()
        }
    }
    #[derive(Default)]
    struct FakeClipboard {
        text: Mutex<Vec<String>>,
    }
    impl Clipboard for FakeClipboard {
        fn set_text(&self, s: &str) -> Result<(), String> {
            self.text.lock().unwrap().push(s.into());
            Ok(())
        }
        fn set_image(&self, _: &Image) -> Result<(), String> {
            Ok(())
        }
    }

    /// Bộ tải lên giả: trả kết quả định sẵn, ghi lại khóa và content-type.
    struct FakeUploader {
        result: Box<dyn Fn() -> Result<String, UploadError> + Send + Sync>,
        calls: Mutex<Vec<(String, String)>>,
    }
    impl FakeUploader {
        fn new(result: Result<String, UploadError>) -> Arc<FakeUploader> {
            // `UploadError` không `Clone`: dựng lại bằng chuỗi `Debug` không cần; giữ một lần dùng.
            let cell = Mutex::new(Some(result));
            Arc::new(FakeUploader {
                result: Box::new(move || {
                    cell.lock()
                        .unwrap()
                        .take()
                        .unwrap_or(Err(UploadError::Network("again".into())))
                }),
                calls: Mutex::new(Vec::new()),
            })
        }
    }
    impl Uploader for FakeUploader {
        fn upload(&self, key: &str, _b: &[u8], ct: &str) -> Result<String, UploadError> {
            self.calls.lock().unwrap().push((key.into(), ct.into()));
            (self.result)()
        }
    }

    /// Chờ tín hiệu mới trả về (hạn 5 s để test không treo).
    struct GatedUploader(Mutex<std::sync::mpsc::Receiver<()>>);
    impl Uploader for GatedUploader {
        fn upload(&self, _k: &str, _b: &[u8], _c: &str) -> Result<String, UploadError> {
            let _ = self.0.lock().unwrap().recv_timeout(Duration::from_secs(5));
            Ok("https://x/gated".into())
        }
    }

    /// Gọi `drain` ngay trong lúc tải lên (mô phỏng thoát giữa chừng) và nhớ số lượt lấy được.
    struct DrainingUploader {
        reg: Arc<PendingUploads>,
        drained: Mutex<Option<usize>>,
    }
    impl Uploader for DrainingUploader {
        fn upload(&self, _k: &str, _b: &[u8], _c: &str) -> Result<String, UploadError> {
            *self.drained.lock().unwrap() = Some(self.reg.drain().len());
            Err(UploadError::Network("down".into()))
        }
    }

    struct Rig {
        dir: TempDir,
        reg: Arc<PendingUploads>,
        notifier: Arc<FakeNotifier>,
        clipboard: Arc<FakeClipboard>,
        settings: Arc<AtomicU32>,
    }
    impl Rig {
        fn new() -> Rig {
            Rig {
                dir: TempDir::new(),
                reg: Arc::new(PendingUploads::new()),
                notifier: Arc::new(FakeNotifier::default()),
                clipboard: Arc::new(FakeClipboard::default()),
                settings: Arc::new(AtomicU32::new(0)),
            }
        }
        fn params(&self, fmt: UploadFormat) -> UploadParams {
            UploadParams {
                save_dir: self.dir.0.clone(),
                expiry: Expiry::D7,
                upload_format: fmt,
                jpeg_quality: 80,
                r2_complete: true,
            }
        }
        fn deps(&self, up: Option<Arc<dyn Uploader>>) -> Deps {
            let s = self.settings.clone();
            Deps {
                uploader: up,
                clipboard: self.clipboard.clone(),
                notifier: self.notifier.clone(),
                clock: now,
                lang: Lang::En,
                on_needs_settings: Box::new(move || {
                    s.fetch_add(1, Ordering::SeqCst);
                }),
            }
        }
        fn run(&self, up: Option<Arc<dyn Uploader>>, fmt: UploadFormat) {
            start(&self.reg, self.params(fmt), self.deps(up), img())
                .unwrap()
                .join()
                .unwrap();
        }
        fn pending_left(&self) -> usize {
            self.reg.drain().len()
        }
    }

    #[test]
    fn success_puts_the_link_in_the_clipboard_and_leaves_nothing_behind() {
        let r = Rig::new();
        let up = FakeUploader::new(Ok("https://cdn.test/d7/abc.png".into()));
        r.run(Some(up.clone()), UploadFormat::Png);
        assert_eq!(
            *r.clipboard.text.lock().unwrap(),
            vec!["https://cdn.test/d7/abc.png".to_string()]
        );
        assert_eq!(r.notifier.kinds(), vec![NoticeKind::Success]);
        assert_eq!(r.dir.files(), 0, "thành công thì không lưu tệp");
        assert_eq!(r.settings.load(Ordering::SeqCst), 0);
        assert_eq!(r.pending_left(), 0, "lượt đã rút khỏi sổ");
        let calls = up.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].0.starts_with("d7/") && calls[0].0.ends_with(".png"));
        assert_eq!(calls[0].1, "image/png");
    }

    #[test]
    fn jpeg_format_and_expiry_reach_the_object_key() {
        let r = Rig::new();
        let up = FakeUploader::new(Ok("https://cdn.test/x".into()));
        r.run(Some(up.clone()), UploadFormat::Jpeg);
        let calls = up.calls.lock().unwrap();
        assert!(calls[0].0.starts_with("d7/") && calls[0].0.ends_with(".jpg"));
        assert_eq!(calls[0].1, "image/jpeg");
    }

    #[test]
    fn every_upload_error_saves_the_image_and_reports() {
        let errors: [fn() -> UploadError; 6] = [
            || UploadError::Network("net".into()),
            || UploadError::Auth,
            || UploadError::BucketNotFound,
            || UploadError::Http(500),
            || UploadError::PublicUrl("bad".into()),
            || UploadError::ClockSkew,
        ];
        for mk in errors {
            let e = mk();
            let r = Rig::new();
            r.run(Some(FakeUploader::new(Err(mk()))), UploadFormat::Png);
            assert_eq!(r.dir.files(), 1, "{e:?}: ảnh phải được lưu dự phòng");
            assert_eq!(r.notifier.kinds(), vec![NoticeKind::Error], "{e:?}");
            assert!(r.clipboard.text.lock().unwrap().is_empty());
            assert_eq!(r.settings.load(Ordering::SeqCst), 0, "{e:?}");
            assert_eq!(r.pending_left(), 0);
        }
    }

    #[test]
    fn missing_r2_saves_the_image_and_opens_options_once() {
        let r = Rig::new();
        r.run(None, UploadFormat::Png);
        assert_eq!(r.dir.files(), 1);
        assert_eq!(r.notifier.kinds(), vec![NoticeKind::Error]);
        assert_eq!(r.settings.load(Ordering::SeqCst), 1);
        assert_eq!(r.pending_left(), 0);
    }

    #[test]
    fn uploader_reporting_not_configured_also_opens_options() {
        let r = Rig::new();
        r.run(
            Some(FakeUploader::new(Err(UploadError::NotConfigured))),
            UploadFormat::Png,
        );
        assert_eq!(r.dir.files(), 1);
        assert_eq!(r.settings.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn upload_is_registered_before_it_runs() {
        let r = Rig::new();
        let up = Arc::new(DrainingUploader {
            reg: r.reg.clone(),
            drained: Mutex::new(None),
        });
        r.run(Some(up.clone()), UploadFormat::Png);
        assert_eq!(
            *up.drained.lock().unwrap(),
            Some(1),
            "khi bộ tải lên chạy, lượt phải đang nằm trong sổ"
        );
    }

    #[test]
    fn exit_midway_saves_exactly_one_file_and_the_late_thread_adds_none() {
        let r = Rig::new();
        let (go, rx) = std::sync::mpsc::channel::<()>();
        let up: Arc<dyn Uploader> = Arc::new(GatedUploader(Mutex::new(rx)));
        let h = start(&r.reg, r.params(UploadFormat::Png), r.deps(Some(up)), img()).unwrap();
        // Thoát khi tải lên chưa xong: chỉ ghi tệp, không thông báo, không clipboard.
        assert_eq!(save_on_exit(&r.reg, now(), Lang::En), 1);
        assert_eq!(r.dir.files(), 1);
        assert!(r.notifier.kinds().is_empty());
        assert!(r.clipboard.text.lock().unwrap().is_empty());
        // Gọi lại: sổ rỗng, không ghi thêm.
        assert_eq!(save_on_exit(&r.reg, now(), Lang::En), 0);
        go.send(()).unwrap();
        h.join().unwrap();
        assert_eq!(r.dir.files(), 1, "luồng tới muộn không ghi bản thứ hai");
        assert_eq!(r.pending_left(), 0);
    }

    #[test]
    fn exit_during_a_failing_upload_keeps_the_image() {
        // Lượt bị lấy khỏi sổ giữa lúc tải lên rồi tải lên lỗi: lưu dự phòng của core vẫn giữ ảnh.
        let r = Rig::new();
        let up = Arc::new(DrainingUploader {
            reg: r.reg.clone(),
            drained: Mutex::new(None),
        });
        r.run(Some(up), UploadFormat::Png);
        assert_eq!(r.dir.files(), 1);
    }

    #[test]
    fn several_pending_uploads_are_each_saved_once_at_exit() {
        let r = Rig::new();
        let mut gates = Vec::new();
        let mut hs = Vec::new();
        for _ in 0..3 {
            let (go, rx) = std::sync::mpsc::channel::<()>();
            gates.push(go);
            let up: Arc<dyn Uploader> = Arc::new(GatedUploader(Mutex::new(rx)));
            hs.push(start(&r.reg, r.params(UploadFormat::Png), r.deps(Some(up)), img()).unwrap());
        }
        assert_eq!(save_on_exit(&r.reg, now(), Lang::Vi), 3);
        assert_eq!(r.dir.files(), 3, "mỗi ảnh một tệp riêng, không ghi đè");
        for g in gates {
            g.send(()).unwrap();
        }
        for h in hs {
            h.join().unwrap();
        }
        assert_eq!(r.dir.files(), 3);
    }

    #[test]
    fn exit_after_everything_finished_writes_nothing() {
        let r = Rig::new();
        r.run(
            Some(FakeUploader::new(Ok("https://cdn.test/a".into()))),
            UploadFormat::Png,
        );
        assert_eq!(save_on_exit(&r.reg, now(), Lang::En), 0);
        assert_eq!(r.dir.files(), 0);
    }

    #[test]
    fn exit_save_uses_each_jobs_own_directory() {
        let a = Rig::new();
        let b = Rig::new();
        let reg = Arc::new(PendingUploads::new());
        reg.add(img(), a.dir.0.clone());
        reg.add(img(), b.dir.0.clone());
        assert_eq!(save_on_exit(&reg, now(), Lang::En), 2);
        assert_eq!((a.dir.files(), b.dir.files()), (1, 1));
    }

    #[test]
    fn exit_save_failure_is_not_counted_and_does_not_panic() {
        // Thư mục lưu là một tệp: không ghi được; không panic, không đếm.
        let r = Rig::new();
        let bad = r.dir.0.join("not_a_dir");
        std::fs::write(&bad, b"x").unwrap();
        let reg = Arc::new(PendingUploads::new());
        reg.add(img(), bad);
        assert_eq!(save_on_exit(&reg, now(), Lang::En), 0);
    }

    #[test]
    fn spawn_failure_saves_the_image_and_empties_the_registry() {
        let r = Rig::new();
        let up = FakeUploader::new(Ok("https://cdn.test/a".into()));
        let h = start_with(
            &r.reg,
            r.params(UploadFormat::Png),
            r.deps(Some(up.clone())),
            img(),
            |_job| Err("no threads".to_string()),
        );
        assert!(h.is_none());
        assert_eq!(r.dir.files(), 1, "lưu ngay thay vì mất ảnh");
        assert!(up.calls.lock().unwrap().is_empty(), "không tải lên");
        let kinds = r.notifier.kinds();
        assert_eq!(kinds[0], NoticeKind::Error);
        assert_eq!(r.pending_left(), 0);
    }

    #[test]
    fn spawn_failure_after_exit_drained_does_not_save_twice() {
        let r = Rig::new();
        let reg = r.reg.clone();
        let h = start_with(
            &r.reg,
            r.params(UploadFormat::Png),
            r.deps(None),
            img(),
            move |_job| {
                // Thoát giành lượt giữa lúc tạo luồng thất bại.
                assert_eq!(save_on_exit(&reg, now(), Lang::En), 1);
                Err("no threads".to_string())
            },
        );
        assert!(h.is_none());
        assert_eq!(r.dir.files(), 1);
    }

    #[test]
    fn params_follow_the_config_without_the_secret() {
        let mut c = Config::defaults(std::path::Path::new("/tmp/darkshot-test-dir"), Lang::En);
        c.upload_format = UploadFormat::Jpeg;
        c.jpeg_quality = 61;
        c.expiry = Expiry::D1;
        c.save_dir = PathBuf::from("/tmp/darkshot-test-dir");
        let p = UploadParams::from_config(&c);
        assert_eq!(p.upload_format, UploadFormat::Jpeg);
        assert_eq!(p.jpeg_quality, 61);
        assert_eq!(p.expiry, Expiry::D1);
        assert_eq!(p.save_dir, PathBuf::from("/tmp/darkshot-test-dir"));
        assert!(!p.r2_complete);
        assert!(uploader_for(&c).is_none());
        c.r2 = darkshot_core::config::R2Config {
            account_id: "acc".into(),
            access_key_id: "kid".into(),
            secret_access_key: "SENTINEL-SECRET-VALUE".into(),
            bucket: "b".into(),
            public_base_url: "https://cdn.example.com".into(),
        };
        let p = UploadParams::from_config(&c);
        assert!(p.r2_complete);
        assert!(uploader_for(&c).is_some());
        assert!(!format!("{p:?}").contains("SENTINEL"));
    }

    #[test]
    fn follow_up_only_for_needs_settings() {
        let p = PathBuf::from("a.png");
        assert_eq!(
            follow_up(&Outcome::UploadFailedSaved {
                path: p.clone(),
                needs_settings: true
            }),
            FollowUp::OpenOptions
        );
        for o in [
            Outcome::UploadFailedSaved {
                path: p.clone(),
                needs_settings: false,
            },
            Outcome::Uploaded("l".into()),
            Outcome::Saved(p),
            Outcome::Copied,
            Outcome::Cancelled,
            Outcome::Failed,
        ] {
            assert_eq!(follow_up(&o), FollowUp::None, "{o:?}");
        }
    }

    #[test]
    fn whole_image_is_not_copied_and_a_region_is_cropped() {
        let big = Image {
            width: 4,
            height: 4,
            bgra: (0..64).collect(),
        };
        let ptr = big.bgra.as_ptr();
        let whole = image_for_upload(
            big,
            Rect {
                x: 0,
                y: 0,
                w: 4,
                h: 4,
            },
        );
        assert_eq!(whole.bgra.as_ptr(), ptr, "dùng luôn ảnh gốc");
        let part = image_for_upload(
            whole,
            Rect {
                x: 1,
                y: 1,
                w: 2,
                h: 2,
            },
        );
        assert_eq!((part.width, part.height, part.bgra.len()), (2, 2, 16));
        assert!(part.is_valid());
    }

    #[test]
    fn wait_idle_never_outlasts_the_deadline() {
        let t = Instant::now();
        assert!(wait_idle(t + Duration::from_secs(1), || false));
        assert!(t.elapsed() < Duration::from_millis(200));
        let t = Instant::now();
        assert!(!wait_idle(t + Duration::from_millis(60), || true));
        let e = t.elapsed();
        assert!(e >= Duration::from_millis(60) && e < Duration::from_millis(500));
        // Hạn chót đã qua: không ngủ.
        let t = Instant::now();
        assert!(!wait_idle(t, || true));
        assert!(t.elapsed() < Duration::from_millis(100));
    }

    #[test]
    fn registry_is_one_shared_instance() {
        assert!(Arc::ptr_eq(&registry(), &registry()));
    }
}
