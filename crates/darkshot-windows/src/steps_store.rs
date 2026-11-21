//! Phiên chụp từng bước trên đĩa: thư mục `paths::steps_dir()` gồm ảnh `NNN.png` và `session.json`.
//!
//! Thứ tự ghi bảo đảm dữ liệu không mất: ảnh mã hóa và ghi nguyên tử trên luồng nền
//! `darkshot-steps` (hàng đợi), kết quả báo về luồng giao diện bằng `PostMessage`; chỉ khi đó bước
//! mới vào danh sách và `session.json` được ghi nguyên tử (tệp tạm, `sync_all`, đổi tên). Khởi động
//! thì khôi phục; `session.json` hỏng thì cả thư mục được đổi tên và giữ lại.
//!
//! Trạng thái phiên chỉ sống trên luồng giao diện (`STORE`); luồng nền chỉ ghi ảnh.

use crate::paths;
use darkshot_core::i18n::Lang;
use darkshot_core::image::{encode_png, Image};
use darkshot_core::naming::LocalTime;
use darkshot_core::steps::{Step, StepSession, MAX_STEPS};
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub use darkshot_core::steps::Full;

/// Gửi tới cửa sổ chính khi luồng nền ghi xong (hoặc lỗi) một ảnh bước; cửa sổ gọi `drain_saved`.
pub const WM_STEPS_SAVED: u32 = WM_APP + 13;

/// Tên tệp phiên trong thư mục phiên.
pub const SESSION_FILE: &str = "session.json";

/// Thời gian chờ ghi ảnh đang dở khi kết thúc phiên.
const END_FLUSH: Duration = Duration::from_secs(3);

/// Kết quả ghi một ảnh bước, từ luồng nền về luồng giao diện.
struct Saved {
    gen: u64,
    file: String,
    result: Result<(), String>,
}

pub use darkshot_core::notices::StepError;

/// Tệp đánh dấu "phiên đã kết thúc, đang xóa" (ghi trước khi xóa `session.json`): thư mục còn tệp
/// này là phần sót của một lần kết thúc, được xóa nốt chứ không giữ lại như dữ liệu hỏng.
pub const ENDED_FILE: &str = "ended";

/// Kết quả áp các lần ghi ảnh đã xong.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Drained {
    /// Số bước đã thêm vào phiên.
    pub added: usize,
    /// Lỗi ghi ảnh (bước không được thêm) và lỗi ghi `session.json`.
    pub errors: Vec<StepError>,
}

/// Lý do khi luồng ghi nền không chạy được (không tạo được luồng, hoặc việc ghi panic).
fn worker_failed(lang: Lang) -> String {
    darkshot_core::i18n::t(lang, darkshot_core::i18n::Msg::StepsWorkerFailed).to_string()
}

/// Bộ đếm việc chưa xong của hàng ghi: `flush` chờ nó về 0.
#[derive(Default)]
struct Pending {
    n: Mutex<usize>,
    cv: Condvar,
}

impl Pending {
    fn lock(&self) -> MutexGuard<'_, usize> {
        // Chỉ là một số đếm: poison không làm nó sai, dùng tiếp.
        self.n.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn inc(&self) {
        let mut g = self.lock();
        *g = g.saturating_add(1);
    }

    fn dec(&self) {
        let mut g = self.lock();
        *g = g.saturating_sub(1);
        if *g == 0 {
            self.cv.notify_all();
        }
    }

    fn wait_empty(&self, timeout: Duration) -> bool {
        let g = self.lock();
        match self.cv.wait_timeout_while(g, timeout, |n| *n > 0) {
            Ok((g, _)) => *g == 0,
            Err(e) => *e.into_inner().0 == 0,
        }
    }
}

/// Giảm bộ đếm khi rời phạm vi, kể cả khi việc ghi panic.
struct DoneGuard<'a>(&'a Pending);

impl Drop for DoneGuard<'_> {
    fn drop(&mut self) {
        self.0.dec();
    }
}

type Job = Box<dyn FnOnce() + Send>;

/// Hàng ghi ảnh: một luồng `darkshot-steps` sinh lười ở lần đẩy đầu, ghi tuần tự theo thứ tự đẩy.
struct Writer {
    tx: Option<Sender<Job>>,
    pending: Arc<Pending>,
}

impl Writer {
    fn new() -> Writer {
        Writer {
            tx: None,
            pending: Arc::new(Pending::default()),
        }
    }

    fn spawn(&self) -> Option<Sender<Job>> {
        let (tx, rx) = mpsc::channel::<Job>();
        let pending = self.pending.clone();
        std::thread::Builder::new()
            .name("darkshot-steps".into())
            .spawn(move || {
                for job in rx {
                    let _done = DoneGuard(&pending);
                    // Việc tự bắt panic của nó; đây chỉ là lưới an toàn để luồng sống tiếp.
                    let _ = catch_unwind(AssertUnwindSafe(job));
                }
            })
            .ok()
            .map(|_| tx)
    }

    /// Đẩy một việc; `false` khi không có luồng ghi (việc bị bỏ, không chạy trên luồng gọi).
    fn push(&mut self, job: Job) -> bool {
        self.pending.inc();
        let mut job = job;
        // Lần thứ hai chỉ khi luồng ghi đã chết (đầu nhận bị hủy): sinh lại một lần.
        for _ in 0..2 {
            if self.tx.is_none() {
                self.tx = self.spawn();
            }
            let Some(sender) = self.tx.as_ref() else {
                break;
            };
            match sender.send(job) {
                Ok(()) => return true,
                Err(mpsc::SendError(back)) => {
                    job = back;
                    self.tx = None;
                }
            }
        }
        drop(job);
        self.pending.dec();
        false
    }

    fn flush(&self, timeout: Duration) -> bool {
        self.pending.wait_empty(timeout)
    }
}

/// Ghi `bytes` vào `path` nguyên tử: tệp tạm cùng thư mục (`<tên>.tmp`), `sync_all`, đổi tên đè.
/// Không tạo thư mục. Lỗi thì xóa tệp tạm.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    let write = || -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        // Mất điện ngay sau khi đổi tên không được để lại tệp rỗng.
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)
    };
    let r = write();
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r
}

/// `session.json` chắc chắn vắng: chỉ khi hỏi được và câu trả lời là không có (lỗi hỏi thì coi là
/// có, để không xóa nhầm một phiên).
fn json_absent(exists: std::io::Result<bool>) -> bool {
    matches!(exists, Ok(false))
}

/// Xóa tệp `path`; không có thì coi là xong.
fn remove_if_exists(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Xóa thư mục phiên `root`: mọi thứ trừ tệp đánh dấu `ended` trước, đánh dấu sau cùng, rồi thư mục.
/// Lỗi giữa chừng thì đánh dấu còn lại cùng phần sót (để lần sau nhận ra và xóa nốt). Không có thư
/// mục thì coi là xong.
fn remove_session_dir(root: &Path) -> std::io::Result<()> {
    let not_found = |e: &std::io::Error| e.kind() == std::io::ErrorKind::NotFound;
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(e) if not_found(&e) => return Ok(()),
        Err(e) => return Err(e),
    };
    let mut first: Option<std::io::Error> = None;
    for e in entries {
        let r = e.and_then(|e| {
            if e.file_name() == ENDED_FILE {
                return Ok(());
            }
            let p = e.path();
            if e.file_type()?.is_dir() {
                std::fs::remove_dir_all(&p)
            } else {
                std::fs::remove_file(&p)
            }
        });
        if let Err(e) = r {
            if !not_found(&e) {
                first.get_or_insert(e);
            }
        }
    }
    if let Some(e) = first {
        return Err(e);
    }
    for r in [
        std::fs::remove_file(root.join(ENDED_FILE)),
        std::fs::remove_dir(root),
    ] {
        match r {
            Err(e) if !not_found(&e) => return Err(e),
            _ => {}
        }
    }
    Ok(())
}

/// Mã hóa PNG rồi ghi nguyên tử (chạy trên luồng nền).
fn write_png(path: &Path, img: &Image, lang: Lang) -> Result<(), String> {
    let png = encode_png(img).map_err(|e| e.message(lang))?;
    write_atomic(path, &png).map_err(|e| e.to_string())
}

/// Phiên chụp từng bước trong một thư mục. Chỉ dùng trên một luồng (luồng giao diện).
pub struct Store {
    root: PathBuf,
    /// `None`: không có phiên.
    session: Option<StepSession>,
    /// Có thay đổi chưa xuất (thêm, xóa, đổi thứ tự, ghi chú).
    dirty: bool,
    /// Thế hệ phiên: kết quả ghi của phiên đã kết thúc bị bỏ.
    gen: u64,
    /// Tên tệp đã giữ chỗ cho các ảnh đang ghi.
    in_flight: Vec<String>,
    writer: Writer,
    results: Arc<Mutex<Vec<Saved>>>,
    notify: Arc<dyn Fn() + Send + Sync>,
    /// Lần ghi `session.json` gần nhất lỗi: phải ghi lại (lần thay đổi sau, lúc thoát).
    json_stale: bool,
    /// Lỗi chưa báo của các thao tác sửa phiên.
    errors: Vec<StepError>,
}

impl Store {
    /// Kho cho thư mục phiên `root`; `notify` được gọi trên luồng nền sau mỗi lần ghi ảnh.
    /// Không đụng đĩa.
    pub fn new(root: PathBuf, notify: Box<dyn Fn() + Send + Sync>) -> Store {
        Store {
            root,
            session: None,
            dirty: false,
            gen: 0,
            in_flight: Vec::new(),
            writer: Writer::new(),
            results: Arc::new(Mutex::new(Vec::new())),
            notify: Arc::from(notify),
            json_stale: false,
            errors: Vec::new(),
        }
    }

    /// `session.json` đang chậm hơn phiên trong bộ nhớ (lần ghi gần nhất lỗi).
    #[cfg(test)]
    pub fn json_stale(&self) -> bool {
        self.json_stale
    }

    /// Lấy ra các lỗi chưa báo của thao tác sửa phiên.
    pub fn take_errors(&mut self) -> Vec<StepError> {
        std::mem::take(&mut self.errors)
    }

    fn results(&self) -> MutexGuard<'_, Vec<Saved>> {
        self.results.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn active(&self) -> bool {
        self.session.is_some()
    }

    pub fn count(&self) -> usize {
        self.session.as_ref().map_or(0, |s| s.steps.len())
    }

    pub fn steps(&self) -> &[Step] {
        self.session.as_ref().map_or(&[], |s| &s.steps)
    }

    #[cfg_attr(not(test), allow(dead_code))] // Quyết định hỏi dùng `needs_end_confirm`.
    pub fn dirty(&self) -> bool {
        self.dirty
    }

    /// Chờ ảnh đang ghi (tối đa `timeout`) rồi áp kết quả.
    pub fn settle(&mut self, timeout: Duration) -> Drained {
        let _ = self.flush(timeout);
        self.drain()
    }

    /// Kết thúc lúc này cần hỏi: còn bước chưa xuất, hay còn ảnh đang ghi (chưa vào danh sách).
    pub fn needs_end_confirm(&self) -> bool {
        (self.dirty && self.count() > 0) || !self.in_flight.is_empty()
    }

    /// Thư mục phiên là phần sót của một lần kết thúc: có `ended`, không có `session.json`.
    fn is_ended_leftover(&self) -> bool {
        self.root.join(ENDED_FILE).is_file()
            && json_absent(self.root.join(SESSION_FILE).try_exists())
    }

    pub fn mark_exported(&mut self) {
        self.dirty = false;
    }

    /// Đường dẫn ảnh của bước thứ `i` (theo thứ tự hiển thị).
    pub fn image_path(&self, i: usize) -> Option<PathBuf> {
        self.steps().get(i).map(|s| self.root.join(&s.file))
    }

    /// Ghi `session.json` nguyên tử theo phiên hiện tại.
    fn persist(&self) -> std::io::Result<()> {
        let Some(s) = &self.session else {
            return Ok(());
        };
        std::fs::create_dir_all(&self.root)?;
        write_atomic(&self.root.join(SESSION_FILE), s.to_json().as_bytes())
    }

    /// Như `persist`, ghi nhớ kết quả vào `json_stale`; lỗi trả lý do.
    fn save(&mut self) -> Result<(), String> {
        let r = self.persist();
        self.json_stale = r.is_err();
        r.map_err(|e| e.to_string())
    }

    /// Thư mục phiên còn dữ liệu (json hoặc ảnh) không thuộc phiên nào trong bộ nhớ; không đọc
    /// được thư mục cũng coi là còn.
    fn has_leftovers(&self) -> bool {
        let entries = match std::fs::read_dir(&self.root) {
            Ok(e) => e,
            Err(e) => return e.kind() != std::io::ErrorKind::NotFound,
        };
        entries.flatten().any(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name == SESSION_FILE || name.ends_with(".png")
        })
    }

    /// Bắt đầu phiên trống (đang có phiên thì không làm gì): tạo thư mục và `session.json`.
    /// Thư mục còn dữ liệu chưa khôi phục được thì không ghi đè: đổi tên nó thành
    /// `current.corrupt-<now>` để giữ lại; không đổi tên được thì không bắt đầu (trả lỗi).
    pub fn start_at(&mut self, now: LocalTime) -> std::io::Result<()> {
        if self.active() {
            return Ok(());
        }
        if self.is_ended_leftover() {
            // Phần sót của lần kết thúc trước: xóa nốt (không phải dữ liệu cần giữ).
            let _ = remove_session_dir(&self.root);
        }
        if self.has_leftovers() {
            self.keep_corrupt(now)?;
        }
        self.gen += 1;
        self.in_flight.clear();
        self.session = Some(StepSession::default());
        self.dirty = false;
        let mut r = self.persist();
        if r.is_ok() {
            // Đánh dấu `ended` cũ không bao giờ nằm cạnh `session.json` của phiên đang chạy.
            if let Err(e) = remove_if_exists(&self.root.join(ENDED_FILE)) {
                let _ = std::fs::remove_file(self.root.join(SESSION_FILE));
                r = Err(e);
            }
        }
        if r.is_err() {
            // Không ghi được phiên lên đĩa: không có phiên.
            self.session = None;
        }
        self.json_stale = false;
        r
    }

    /// Tên tệp mới không trùng bước nào, ảnh đang ghi nào, hay tệp nào đang có trên đĩa.
    fn reserve_name(&self) -> String {
        let mut probe = StepSession {
            steps: self.steps().to_vec(),
        };
        probe.steps.extend(self.in_flight.iter().map(|f| Step {
            file: f.clone(),
            caption: String::new(),
        }));
        let mut name = probe.next_file_name();
        for _ in 0..999 {
            if !self.root.join(&name).exists() {
                break;
            }
            probe.steps.push(Step {
                file: name,
                caption: String::new(),
            });
            name = probe.next_file_name();
        }
        name
    }

    /// Thêm một bước: giữ tên, đẩy việc mã hóa và ghi ảnh vào hàng nền. Bước chỉ vào danh sách
    /// (và `session.json`) khi `drain` áp kết quả ghi thành công. Không có phiên thì bỏ qua.
    /// Đủ `MAX_STEPS` (kể cả ảnh đang ghi) thì `Err(Full)`.
    pub fn add(&mut self, img: Image) -> Result<(), Full> {
        if !self.active() {
            return Ok(());
        }
        if self.count() + self.in_flight.len() >= MAX_STEPS {
            return Err(Full);
        }
        let file = self.reserve_name();
        self.in_flight.push(file.clone());
        let gen = self.gen;
        if let Err(e) = std::fs::create_dir_all(&self.root) {
            self.results().push(Saved {
                gen,
                file,
                result: Err(e.to_string()),
            });
            (self.notify)();
            return Ok(());
        }
        let path = self.root.join(&file);
        let results = self.results.clone();
        let notify = self.notify.clone();
        let job_file = file.clone();
        let lang = crate::lang::current();
        let job: Job = Box::new(move || {
            let result = catch_unwind(AssertUnwindSafe(|| write_png(&path, &img, lang)))
                .unwrap_or_else(|_| Err(worker_failed(lang)));
            results
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(Saved {
                    gen,
                    file: job_file,
                    result,
                });
            notify();
        });
        if !self.writer.push(job) {
            self.results().push(Saved {
                gen,
                file,
                result: Err(worker_failed(lang)),
            });
            (self.notify)();
        }
        Ok(())
    }

    /// Áp các lần ghi ảnh đã xong: thành công thì thêm bước và ghi `session.json`; lỗi thì không
    /// thêm và trả lý do. Kết quả của phiên đã kết thúc bị bỏ (ảnh của nó bị xóa).
    pub fn drain(&mut self) -> Drained {
        let done: Vec<Saved> = std::mem::take(&mut *self.results());
        let mut out = Drained::default();
        for d in done {
            if d.gen != self.gen || !self.active() {
                let in_use = self.steps().iter().any(|s| s.file == d.file)
                    || self.in_flight.contains(&d.file);
                if d.result.is_ok() && !in_use {
                    let _ = std::fs::remove_file(self.root.join(&d.file));
                }
                continue;
            }
            self.in_flight.retain(|f| *f != d.file);
            match d.result {
                Ok(()) => {
                    let added = self
                        .session
                        .as_mut()
                        .is_some_and(|s| s.add(d.file.clone()).is_ok());
                    if added {
                        out.added += 1;
                        self.dirty = true;
                    } else {
                        let _ = std::fs::remove_file(self.root.join(&d.file));
                    }
                }
                Err(e) => out.errors.push(StepError::Image(e)),
            }
        }
        if out.added > 0 || (self.json_stale && self.active()) {
            if let Err(e) = self.save() {
                out.errors.push(StepError::Session(e));
            }
        }
        out
    }

    /// Lúc thoát: chờ ảnh đang ghi tối đa `timeout`, áp kết quả, ghi lại `session.json` nếu đang
    /// trễ. Trả mọi lỗi chưa báo.
    pub fn finish(&mut self, timeout: Duration) -> Vec<StepError> {
        let _ = self.flush(timeout);
        let mut errors = self.drain().errors;
        errors.extend(self.take_errors());
        errors
    }

    /// Chờ các ảnh đang ghi xong, tối đa `timeout`; `true` khi hàng rỗng.
    pub fn flush(&self, timeout: Duration) -> bool {
        self.writer.flush(timeout)
    }

    /// Khôi phục phiên từ đĩa lúc khởi động (`now` đặt tên thư mục hỏng). `session.json` hỏng thì
    /// đổi tên cả thư mục thành `current.corrupt-<thời gian>` và không có phiên; ảnh thiếu thì bỏ
    /// bước đó. Không có `session.json`, hay không đọc được tệp: không có phiên, không đụng gì.
    pub fn restore_at(&mut self, now: LocalTime) {
        if self.active() {
            return;
        }
        let Ok(bytes) = std::fs::read(self.root.join(SESSION_FILE)) else {
            if self.is_ended_leftover() {
                // Phần sót của lần kết thúc trước (thoát hay tệp bị giữ lúc xóa): xóa nốt.
                let _ = remove_session_dir(&self.root);
            }
            return;
        };
        // Có json: phiên chưa kết thúc xong (dừng giữa ghi đánh dấu và xóa json) thì giữ phiên.
        let _ = std::fs::remove_file(self.root.join(ENDED_FILE));
        let parsed = String::from_utf8(bytes)
            .ok()
            .and_then(|s| StepSession::from_json(&s));
        let Some(mut sess) = parsed else {
            // Không đổi tên được: thư mục giữ nguyên; bắt đầu phiên mới sẽ thử lại.
            let _ = self.keep_corrupt(now);
            return;
        };
        let before = sess.steps.len();
        sess.steps.retain(|s| self.root.join(&s.file).is_file());
        let dropped = sess.steps.len() != before;
        self.gen += 1;
        self.in_flight.clear();
        // Không biết phiên đã xuất chưa: coi là chưa (hỏi lại trước khi xóa).
        self.dirty = !sess.steps.is_empty();
        self.session = Some(sess);
        self.json_stale = false;
        if dropped {
            if let Err(e) = self.save() {
                self.errors.push(StepError::Session(e));
            }
        }
    }

    /// Đổi tên thư mục phiên hỏng (hay chưa khôi phục được) sang tên chưa có; không đổi được thì
    /// để nguyên và trả lỗi.
    fn keep_corrupt(&self, now: LocalTime) -> std::io::Result<()> {
        let base = format!(
            "{}.corrupt-{:04}{:02}{:02}-{:02}{:02}{:02}",
            self.root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "current".to_string()),
            now.year,
            now.month,
            now.day,
            now.hour,
            now.minute,
            now.second
        );
        let mut last = std::io::Error::from(std::io::ErrorKind::AlreadyExists);
        for k in 1..=100 {
            let name = if k == 1 {
                base.clone()
            } else {
                format!("{base}-{k}")
            };
            let target = self.root.with_file_name(name);
            if target.exists() {
                continue;
            }
            match std::fs::rename(&self.root, &target) {
                Ok(()) => return Ok(()),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// Kết thúc phiên: chờ ảnh đang ghi (tối đa 3 s), xóa `session.json` TRƯỚC (không xóa được
    /// thì phiên giữ nguyên và trả lỗi), rồi bỏ phiên và xóa thư mục phiên. Lỗi xóa phần còn lại
    /// được trả nhưng phiên đã kết thúc (không còn json nên không khôi phục; nhờ tệp đánh dấu `ended`,
    /// phần sót được xóa nốt ở lần bắt đầu hay khởi động sau). Không có phiên thì không đụng thư mục (có thể là dữ liệu
    /// chưa khôi phục được).
    pub fn end(&mut self) -> std::io::Result<()> {
        if !self.active() {
            return Ok(());
        }
        let _ = self.flush(END_FLUSH);
        // Đánh dấu trước: nếu xóa dở, phần sót được nhận ra và xóa nốt về sau (không giữ như hỏng).
        let marker = self.root.join(ENDED_FILE);
        let marked = std::fs::write(&marker, b"").is_ok();
        match std::fs::remove_file(self.root.join(SESSION_FILE)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                if marked {
                    if let Err(m) = remove_if_exists(&marker) {
                        // Đánh dấu nằm lại cạnh json: báo cả hai lỗi.
                        return Err(std::io::Error::new(e.kind(), format!("{e}; {m}")));
                    }
                }
                return Err(e);
            }
            _ => {}
        }
        // Kết quả còn lại thuộc phiên cũ: bỏ (ảnh nằm trong thư mục sắp xóa).
        self.results().clear();
        self.gen += 1;
        self.in_flight.clear();
        self.session = None;
        self.dirty = false;
        self.json_stale = false;
        remove_session_dir(&self.root)
    }

    /// Sửa phiên bằng `f`; đổi thì ghi `session.json` và đánh dấu chưa xuất. Ghi lỗi thì thay
    /// đổi vẫn giữ trong bộ nhớ, lỗi được ghi lại để báo và json được ghi lại về sau.
    fn edit(&mut self, f: impl FnOnce(&mut StepSession) -> bool) -> bool {
        let changed = self.session.as_mut().is_some_and(f);
        if changed {
            self.dirty = true;
            if let Err(e) = self.save() {
                self.errors.push(StepError::Session(e));
            }
        }
        changed
    }

    /// Xóa bước `i`: ghi `session.json` trước, rồi xóa tệp ảnh. Không ghi được json thì bước được
    /// trả lại (ảnh không bị xóa), báo lỗi và trả `false`.
    pub fn remove(&mut self, i: usize) -> bool {
        let Some(file) = self.steps().get(i).map(|s| s.file.clone()) else {
            return false;
        };
        let (before, dirty) = (self.session.clone(), self.dirty);
        if !self.session.as_mut().is_some_and(|s| s.remove(i)) {
            return false;
        }
        if let Err(e) = self.save() {
            self.session = before;
            self.dirty = dirty;
            self.errors.push(StepError::Session(e));
            return false;
        }
        self.dirty = true;
        let _ = std::fs::remove_file(self.root.join(file));
        true
    }

    pub fn move_up(&mut self, i: usize) -> bool {
        self.edit(|s| s.move_up(i))
    }

    pub fn move_down(&mut self, i: usize) -> bool {
        self.edit(|s| s.move_down(i))
    }

    /// Đặt ghi chú (cắt `MAX_CAPTION` ký tự); ghi chú không đổi thì không ghi gì (vẫn `true`).
    pub fn set_caption(&mut self, i: usize, text: &str) -> bool {
        let same = self.steps().get(i).map(|s| s.caption == text);
        if same == Some(true) {
            return true;
        }
        self.edit(|s| s.set_caption(i, text))
    }

    #[cfg(test)]
    fn set_session_for_test(&mut self, s: StepSession) {
        self.session = Some(s);
    }
}

// ---- Phiên của tiến trình (luồng giao diện) ----

/// HWND của cửa sổ chính nhận `WM_STEPS_SAVED` (0: chưa có).
static WINDOW: AtomicIsize = AtomicIsize::new(0);

fn ignore_errors(_: &[StepError]) {}

thread_local! {
    static STORE: RefCell<Option<Store>> = const { RefCell::new(None) };
    static LISTENERS: RefCell<Vec<fn()>> = const { RefCell::new(Vec::new()) };
    static ON_ERROR: Cell<fn(&[StepError])> = const { Cell::new(ignore_errors) };
}

/// Hàm báo lỗi của phiên (main: thông báo Error qua khay).
pub fn set_on_error(f: fn(&[StepError])) {
    ON_ERROR.with(|c| c.set(f));
}

/// Báo `errors` (nếu có); chỉ gọi khi đã trả mượn `STORE`.
fn report(errors: &[StepError]) {
    if !errors.is_empty() {
        ON_ERROR.with(|c| c.get())(errors);
    }
}

fn post_saved() {
    let h = WINDOW.load(Ordering::SeqCst);
    if h == 0 {
        return;
    }
    // SAFETY: PostMessageW an toàn từ luồng bất kỳ; cửa sổ đã hủy thì lỗi và ta bỏ qua (kết quả
    // vẫn nằm trong hàng, `flush` lúc thoát áp nó).
    unsafe {
        let _ = PostMessageW(
            Some(HWND(h as *mut c_void)),
            WM_STEPS_SAVED,
            WPARAM(0),
            LPARAM(0),
        );
    }
}

/// Đặt cửa sổ chính nhận `WM_STEPS_SAVED`.
pub fn set_window(hwnd: HWND) {
    WINDOW.store(hwnd.0 as isize, Ordering::SeqCst);
}

fn with<R>(f: impl FnOnce(&mut Store) -> R) -> R {
    STORE.with(|s| {
        let mut b = s.borrow_mut();
        let st = b.get_or_insert_with(|| Store::new(paths::steps_dir(), Box::new(post_saved)));
        f(st)
    })
}

/// Kiểm thử: phiên của luồng này dùng thư mục `root` (tạm) thay cho thư mục thật của người dùng.
#[cfg(test)]
pub fn use_root_for_test(root: PathBuf) {
    STORE.with(|s| *s.borrow_mut() = Some(Store::new(root, Box::new(|| {}))));
}

/// Đăng ký hàm được gọi sau mỗi thay đổi của phiên (bắt đầu, thêm, sửa, kết thúc).
pub fn on_change(f: fn()) {
    LISTENERS.with(|l| l.borrow_mut().push(f));
}

/// Gọi các hàm đã đăng ký; chỉ gọi khi đã trả mượn `STORE`.
fn changed() {
    let all = LISTENERS.with(|l| l.borrow().clone());
    for f in all {
        f();
    }
}

/// Bắt đầu phiên trống (đang có phiên thì giữ nguyên); lỗi được báo qua `set_on_error`.
pub fn start() -> bool {
    let now = paths::local_now();
    let r = with(|s| s.start_at(now));
    changed();
    if let Err(e) = &r {
        report(&[StepError::Start(e.to_string())]);
    }
    r.is_ok()
}

pub fn active() -> bool {
    with(|s| s.active())
}

pub fn count() -> usize {
    with(|s| s.count())
}

/// Thêm ảnh bước (ghi nền); bước hiện ra khi `drain_saved` áp kết quả.
pub fn add(image: Image) -> Result<(), Full> {
    with(|s| s.add(image))
}

/// Áp các lần ghi ảnh đã xong (ở `WM_STEPS_SAVED`); lỗi được báo qua `set_on_error`.
pub fn drain_saved() {
    let d = with(|s| s.drain());
    if d.added > 0 {
        changed();
    }
    report(&d.errors);
}

/// Khôi phục phiên lúc khởi động.
pub fn restore() {
    let now = paths::local_now();
    let errors = with(|s| {
        s.restore_at(now);
        s.take_errors()
    });
    changed();
    report(&errors);
}

/// Kết thúc phiên và xóa thư mục phiên; lỗi được báo qua `set_on_error`.
pub fn end() -> bool {
    let (r, ended) = with(|s| (s.end(), !s.active()));
    changed();
    match r {
        Ok(()) => true,
        // Phiên đã kết thúc, còn sót tệp: xóa nốt ở lần bắt đầu hay khởi động sau.
        Err(e) if ended => {
            report(&[StepError::End(e.to_string())]);
            true
        }
        Err(e) => {
            report(&[StepError::Session(e.to_string())]);
            false
        }
    }
}

/// Lúc thoát: chờ ảnh đang ghi tối đa `timeout`, áp kết quả và ghi lại `session.json` nếu đang
/// trễ (thư mục phiên được giữ). Lỗi được báo qua `set_on_error`.
pub fn flush(timeout: Duration) {
    let errors = with(|s| s.finish(timeout));
    report(&errors);
}

/// Các bước theo thứ tự hiển thị (cho cửa sổ Xem lại).
pub fn steps() -> Vec<Step> {
    with(|s| s.steps().to_vec())
}

/// Đường dẫn ảnh của bước `i` (cho cửa sổ Xem lại).
pub fn image_path(i: usize) -> Option<PathBuf> {
    with(|s| s.image_path(i))
}

#[cfg_attr(not(test), allow(dead_code))] // Quyết định hỏi dùng `needs_end_confirm`.
pub fn dirty() -> bool {
    with(|s| s.dirty())
}

/// Chờ ảnh đang ghi tối đa `timeout` rồi áp kết quả (bước mới vào danh sách); lỗi được báo qua
/// `set_on_error`. Gọi trước khi quyết định có hỏi khi kết thúc phiên hay không.
pub fn settle(timeout: Duration) {
    let d = with(|s| s.settle(timeout));
    if d.added > 0 {
        changed();
    }
    report(&d.errors);
}

/// Kết thúc phiên lúc này có cần hỏi không: còn bước chưa xuất, hay còn ảnh đang ghi.
pub fn needs_end_confirm() -> bool {
    with(|s| s.needs_end_confirm())
}

/// Đã xuất: kết thúc phiên sau đó không cần hỏi lại (tới khi có thay đổi mới).
pub fn mark_exported() {
    with(|s| s.mark_exported());
    changed();
}

fn edit_and_notify(f: impl FnOnce(&mut Store) -> bool) -> bool {
    let (r, errors) = with(|s| (f(s), s.take_errors()));
    if r {
        changed();
    }
    report(&errors);
    r
}

pub fn remove(i: usize) -> bool {
    edit_and_notify(|s| s.remove(i))
}

pub fn move_up(i: usize) -> bool {
    edit_and_notify(|s| s.move_up(i))
}

pub fn move_down(i: usize) -> bool {
    edit_and_notify(|s| s.move_down(i))
}

pub fn set_caption(i: usize, text: &str) -> bool {
    edit_and_notify(|s| s.set_caption(i, text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkshot_core::image::Image;
    use darkshot_core::naming::LocalTime;
    use darkshot_core::steps::{StepSession, MAX_STEPS};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    const WAIT: Duration = Duration::from_secs(20);

    /// Thư mục gốc tạm riêng cho mỗi kiểm thử; `current` bên trong chưa tồn tại.
    fn tmp(name: &str) -> PathBuf {
        let base =
            std::env::temp_dir().join(format!("darkshot_steps_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        base.join("current")
    }

    fn cleanup(root: &Path) {
        if let Some(base) = root.parent() {
            let _ = std::fs::remove_dir_all(base);
        }
    }

    fn img(v: u8) -> Image {
        Image {
            width: 3,
            height: 2,
            bgra: vec![v; 3 * 2 * 4],
        }
    }

    fn store(root: &Path) -> Store {
        Store::new(root.to_path_buf(), Box::new(|| {}))
    }

    fn now() -> LocalTime {
        LocalTime {
            year: 2026,
            month: 11,
            day: 18,
            hour: 14,
            minute: 5,
            second: 59,
        }
    }

    /// Thêm một bước và chờ luồng nền ghi xong, rồi áp kết quả.
    fn add_wait(s: &mut Store, v: u8) {
        s.add(img(v)).unwrap();
        assert!(s.flush(WAIT));
        let d = s.drain();
        assert!(d.errors.is_empty(), "{:?}", d.errors);
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    fn json(root: &Path) -> StepSession {
        let s = std::fs::read_to_string(root.join(SESSION_FILE)).unwrap();
        StepSession::from_json(&s).unwrap()
    }

    fn files(s: &StepSession) -> Vec<String> {
        s.steps.iter().map(|x| x.file.clone()).collect()
    }

    #[test]
    fn add_three_steps_writes_pngs_then_json() {
        let root = tmp("add3");
        let mut s = store(&root);
        assert!(!s.active());
        s.start_at(now()).unwrap();
        assert!(s.active());
        assert_eq!(s.count(), 0);
        assert!(json(&root).steps.is_empty());
        for v in [10, 20, 30] {
            add_wait(&mut s, v);
        }
        assert_eq!(s.count(), 3);
        assert!(s.dirty());
        assert_eq!(
            names(&root),
            ["001.png", "002.png", "003.png", "session.json"]
        );
        assert_eq!(files(&json(&root)), ["001.png", "002.png", "003.png"]);
        let png = std::fs::read(root.join("002.png")).unwrap();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(s.image_path(2), Some(root.join("003.png")));
        assert_eq!(s.image_path(3), None);
        cleanup(&root);
    }

    /// `session.json` chỉ nhận bước khi kết quả ghi PNG đã được áp (sau khi ảnh nằm trên đĩa).
    #[test]
    fn json_updates_only_after_png_result() {
        let root = tmp("order");
        let mut s = store(&root);
        s.start_at(now()).unwrap();
        s.add(img(1)).unwrap();
        assert!(s.flush(WAIT));
        // Ảnh đã ghi, kết quả chưa áp: json và danh sách chưa có bước.
        assert!(root.join("001.png").is_file());
        assert!(json(&root).steps.is_empty());
        assert_eq!(s.count(), 0);
        let d = s.drain();
        assert_eq!((d.added, d.errors.len()), (1, 0));
        assert_eq!(files(&json(&root)), ["001.png"]);
        assert_eq!(s.count(), 1);
        cleanup(&root);
    }

    #[test]
    fn restore_resumes_session_with_captions() {
        let root = tmp("restore");
        {
            let mut s = store(&root);
            s.start_at(now()).unwrap();
            add_wait(&mut s, 1);
            add_wait(&mut s, 2);
            assert!(s.set_caption(1, "Bấm Lưu"));
            s.mark_exported();
        }
        let mut s = store(&root);
        s.restore_at(now());
        assert!(s.active());
        assert_eq!(s.count(), 2);
        assert_eq!(s.steps()[1].caption, "Bấm Lưu");
        // Không biết đã xuất hay chưa sau khi khởi động lại: coi là chưa xuất.
        assert!(s.dirty());
        // Thêm tiếp không trùng tên cũ.
        add_wait(&mut s, 3);
        assert_eq!(files(&json(&root)), ["001.png", "002.png", "003.png"]);
        cleanup(&root);
    }

    #[test]
    fn restore_without_folder_or_json_has_no_session() {
        let root = tmp("none");
        let mut s = store(&root);
        s.restore_at(now());
        assert!(!s.active());
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("001.png"), b"x").unwrap();
        s.restore_at(now());
        assert!(!s.active());
        // Không có json: thư mục giữ nguyên.
        assert!(root.join("001.png").is_file());
        cleanup(&root);
    }

    #[test]
    fn corrupt_json_renames_folder_and_keeps_images() {
        let root = tmp("corrupt");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("001.png"), b"img").unwrap();
        std::fs::write(root.join(SESSION_FILE), b"{rac").unwrap();
        let mut s = store(&root);
        s.restore_at(now());
        assert!(!s.active());
        assert!(!root.exists());
        let kept = root.with_file_name("current.corrupt-20261118-140559");
        assert_eq!(std::fs::read(kept.join("001.png")).unwrap(), b"img");
        assert_eq!(std::fs::read(kept.join(SESSION_FILE)).unwrap(), b"{rac");
        // Lần hỏng thứ hai cùng giây: không đè thư mục đã giữ.
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(SESSION_FILE), b"[]").unwrap();
        s.restore_at(now());
        assert!(!root.exists());
        assert!(kept.join("001.png").is_file());
        let kept2 = root.with_file_name("current.corrupt-20261118-140559-2");
        assert_eq!(std::fs::read(kept2.join(SESSION_FILE)).unwrap(), b"[]");
        // Sau đó bắt đầu phiên mới được bình thường.
        s.start_at(now()).unwrap();
        assert!(s.active() && root.join(SESSION_FILE).is_file());
        cleanup(&root);
    }

    #[test]
    fn missing_png_drops_step_on_restore() {
        let root = tmp("missing");
        std::fs::create_dir_all(&root).unwrap();
        let mut sess = StepSession::default();
        for f in ["001.png", "002.png", "003.png"] {
            sess.add(f.to_string()).unwrap();
        }
        sess.set_caption(2, "ba");
        std::fs::write(root.join(SESSION_FILE), sess.to_json()).unwrap();
        std::fs::write(root.join("001.png"), b"a").unwrap();
        std::fs::write(root.join("003.png"), b"c").unwrap();
        let mut s = store(&root);
        s.restore_at(now());
        assert!(s.active());
        let got: Vec<&str> = s.steps().iter().map(|x| x.file.as_str()).collect();
        assert_eq!(got, ["001.png", "003.png"]);
        assert_eq!(s.steps()[1].caption, "ba");
        // json được ghi lại theo danh sách mới.
        assert_eq!(files(&json(&root)), ["001.png", "003.png"]);
        cleanup(&root);
    }

    #[test]
    fn remove_and_move_update_json() {
        let root = tmp("edit");
        let mut s = store(&root);
        s.start_at(now()).unwrap();
        for v in [1, 2, 3] {
            add_wait(&mut s, v);
        }
        assert!(s.move_up(2));
        assert_eq!(files(&json(&root)), ["001.png", "003.png", "002.png"]);
        assert!(s.move_down(0));
        assert_eq!(files(&json(&root)), ["003.png", "001.png", "002.png"]);
        assert!(!s.move_up(0));
        assert!(!s.move_down(2));
        assert!(s.set_caption(0, "đầu"));
        assert_eq!(json(&root).steps[0].caption, "đầu");
        assert!(s.remove(1));
        assert_eq!(files(&json(&root)), ["003.png", "002.png"]);
        assert!(!root.join("001.png").exists());
        assert!(!s.remove(5));
        assert_eq!(s.count(), 2);
        cleanup(&root);
    }

    #[test]
    fn dirty_tracks_changes_since_export() {
        let root = tmp("dirty");
        let mut s = store(&root);
        s.start_at(now()).unwrap();
        assert!(!s.dirty());
        add_wait(&mut s, 1);
        assert!(s.dirty());
        s.mark_exported();
        assert!(!s.dirty());
        assert!(s.set_caption(0, "x"));
        assert!(s.dirty());
        s.mark_exported();
        assert!(!s.move_up(0));
        assert!(!s.dirty());
        cleanup(&root);
    }

    #[test]
    fn end_removes_folder() {
        let root = tmp("end");
        let mut s = store(&root);
        s.start_at(now()).unwrap();
        add_wait(&mut s, 1);
        s.end().unwrap();
        assert!(!s.active());
        assert_eq!(s.count(), 0);
        assert!(!root.exists());
        // Kết thúc khi không có phiên: vô hại.
        s.end().unwrap();
        cleanup(&root);
    }

    /// Kết thúc ngay sau khi thêm: việc ghi đang dở xong trước khi thư mục bị xóa, không để lại tệp.
    #[test]
    fn end_with_write_in_flight_leaves_nothing() {
        let root = tmp("endflight");
        let mut s = store(&root);
        s.start_at(now()).unwrap();
        s.add(img(9)).unwrap();
        s.end().unwrap();
        assert!(!root.exists());
        let d = s.drain();
        assert_eq!((d.added, d.errors.len()), (0, 0));
        assert_eq!(s.count(), 0);
        cleanup(&root);
    }

    #[test]
    fn full_counts_writes_in_flight() {
        let root = tmp("full");
        let mut s = store(&root);
        s.start_at(now()).unwrap();
        let mut sess = StepSession::default();
        for k in 1..MAX_STEPS {
            sess.add(format!("{k:03}.png")).unwrap();
        }
        s.set_session_for_test(sess);
        assert_eq!(s.count(), MAX_STEPS - 1);
        // Bước thứ 99 đang ghi: bước thứ 100 bị từ chối ngay.
        s.add(img(1)).unwrap();
        assert_eq!(s.add(img(2)), Err(Full));
        assert!(s.flush(WAIT));
        assert_eq!(s.drain().added, 1);
        assert_eq!(s.count(), MAX_STEPS);
        assert_eq!(s.add(img(3)), Err(Full));
        cleanup(&root);
    }

    #[test]
    fn new_names_skip_files_already_on_disk() {
        let root = tmp("orphan");
        let mut s = store(&root);
        s.start_at(now()).unwrap();
        std::fs::write(root.join("001.png"), b"orphan").unwrap();
        add_wait(&mut s, 1);
        add_wait(&mut s, 2);
        let got: Vec<&str> = s.steps().iter().map(|x| x.file.as_str()).collect();
        assert_eq!(got, ["002.png", "003.png"]);
        assert_eq!(std::fs::read(root.join("001.png")).unwrap(), b"orphan");
        cleanup(&root);
    }

    /// Không ghi được ảnh: báo lỗi, không thêm bước, json giữ nguyên.
    #[test]
    fn png_write_failure_reports_error_and_adds_nothing() {
        let root = tmp("fail");
        let mut s = store(&root);
        s.start_at(now()).unwrap();
        // Thư mục phiên bị thay bằng một tệp: không tạo được ảnh.
        std::fs::remove_dir_all(&root).unwrap();
        std::fs::write(&root, b"not a dir").unwrap();
        s.add(img(1)).unwrap();
        assert!(s.flush(WAIT));
        let d = s.drain();
        assert_eq!(d.added, 0);
        assert_eq!(d.errors.len(), 1);
        assert_eq!(s.count(), 0);
        // Hết chỗ giữ tên của lượt hỏng: lượt sau vẫn được tính đủ chỗ.
        std::fs::remove_file(&root).unwrap();
        std::fs::create_dir_all(&root).unwrap();
        add_wait(&mut s, 2);
        assert_eq!(s.count(), 1);
        cleanup(&root);
    }

    #[test]
    fn add_without_session_does_nothing() {
        let root = tmp("inactive");
        let mut s = store(&root);
        assert_eq!(s.add(img(1)), Ok(()));
        assert!(s.flush(WAIT));
        let d = s.drain();
        assert_eq!((d.added, d.errors.len()), (0, 0));
        assert!(!root.exists());
        cleanup(&root);
    }

    /// Không bao giờ để lại tệp tạm (`*.tmp`) sau các lần ghi.
    #[test]
    fn writes_leave_no_temp_files() {
        let root = tmp("notmp");
        let mut s = store(&root);
        s.start_at(now()).unwrap();
        for v in [1, 2] {
            add_wait(&mut s, v);
        }
        s.set_caption(0, "a");
        s.move_down(0);
        s.remove(0);
        assert!(names(&root).iter().all(|n| !n.ends_with(".tmp")));
        assert_eq!(names(&root), ["001.png", "session.json"]);
        cleanup(&root);
    }

    #[test]
    fn notify_called_after_each_write() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let root = tmp("notify");
        let hits = Arc::new(AtomicUsize::new(0));
        let h = hits.clone();
        let mut s = Store::new(
            root.clone(),
            Box::new(move || {
                h.fetch_add(1, Ordering::SeqCst);
            }),
        );
        s.start_at(now()).unwrap();
        add_wait(&mut s, 1);
        add_wait(&mut s, 2);
        assert_eq!(hits.load(Ordering::SeqCst), 2);
        cleanup(&root);
    }

    /// Mở `path` không chia sẻ (đọc, ghi, xóa đều bị chặn) tới khi thả.
    fn lock(path: &Path) -> std::fs::File {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(path)
            .unwrap()
    }

    fn session_errors(e: &[StepError]) -> usize {
        e.iter()
            .filter(|x| matches!(x, StepError::Session(_)))
            .count()
    }

    /// Kết thúc: xóa `session.json` trước; không xóa được thì phiên giữ nguyên, ảnh còn.
    #[test]
    fn end_keeps_session_when_json_cannot_be_removed() {
        let root = tmp("endlock");
        let mut s = store(&root);
        s.start_at(now()).unwrap();
        add_wait(&mut s, 1);
        let held = lock(&root.join(SESSION_FILE));
        assert!(s.end().is_err());
        assert!(s.active());
        assert_eq!(s.count(), 1);
        assert!(root.join("001.png").is_file());
        drop(held);
        s.end().unwrap();
        assert!(!s.active() && !root.exists());
        cleanup(&root);
    }

    /// Ghi json lỗi: báo lỗi, nhớ là chưa ghi; lần thay đổi sau hay lúc thoát ghi lại.
    #[test]
    fn json_write_failure_is_reported_and_retried() {
        let root = tmp("jsonlock");
        let mut s = store(&root);
        s.start_at(now()).unwrap();
        add_wait(&mut s, 1);
        add_wait(&mut s, 2);
        let held = lock(&root.join(SESSION_FILE));
        // Sửa ghi chú: đổi trong bộ nhớ, báo lỗi ghi phiên.
        assert!(s.set_caption(0, "a"));
        assert_eq!(session_errors(&s.take_errors()), 1);
        assert!(s.json_stale());
        // Xóa bước: json không ghi được thì không xóa ảnh, bước còn nguyên.
        assert!(!s.remove(1));
        assert_eq!(session_errors(&s.take_errors()), 1);
        assert_eq!(s.count(), 2);
        assert!(root.join("002.png").is_file());
        // Thêm bước: ảnh ghi được, json thì không: bước có trong bộ nhớ, lỗi phiên được báo.
        s.add(img(3)).unwrap();
        assert!(s.flush(WAIT));
        let d = s.drain();
        assert_eq!(d.added, 1);
        assert_eq!(session_errors(&d.errors), 1);
        drop(held);
        // Lúc thoát: ghi lại json đang trễ.
        let errs = s.finish(WAIT);
        assert!(errs.is_empty(), "{errs:?}");
        assert!(!s.json_stale());
        let j = json(&root);
        assert_eq!(files(&j), ["001.png", "002.png", "003.png"]);
        assert_eq!(j.steps[0].caption, "a");
        assert!(names(&root).iter().all(|n| !n.ends_with(".tmp")));
        cleanup(&root);
    }

    /// Lần thay đổi sau ghi lại json đang trễ.
    #[test]
    fn next_change_rewrites_stale_json() {
        let root = tmp("jsonretry");
        let mut s = store(&root);
        s.start_at(now()).unwrap();
        add_wait(&mut s, 1);
        add_wait(&mut s, 2);
        let held = lock(&root.join(SESSION_FILE));
        assert!(s.set_caption(0, "x"));
        assert_eq!(session_errors(&s.take_errors()), 1);
        drop(held);
        assert!(s.move_down(0));
        assert!(s.take_errors().is_empty());
        assert!(!s.json_stale());
        let j = json(&root);
        assert_eq!(files(&j), ["002.png", "001.png"]);
        assert_eq!(j.steps[1].caption, "x");
        cleanup(&root);
    }

    /// Bắt đầu phiên trên thư mục còn json chưa khôi phục: không ghi đè, đổi tên thư mục để giữ.
    #[test]
    fn start_keeps_unrecovered_folder() {
        let root = tmp("startkeep");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(SESSION_FILE), b"{rac").unwrap();
        std::fs::write(root.join("001.png"), b"img").unwrap();
        let mut s = store(&root);
        s.start_at(now()).unwrap();
        assert!(s.active());
        let kept = root.with_file_name("current.corrupt-20261118-140559");
        assert_eq!(std::fs::read(kept.join(SESSION_FILE)).unwrap(), b"{rac");
        assert_eq!(std::fs::read(kept.join("001.png")).unwrap(), b"img");
        assert!(json(&root).steps.is_empty());
        assert_eq!(names(&root), ["session.json"]);
        // Thư mục chỉ có ảnh (không json) cũng được giữ.
        s.end().unwrap();
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("005.png"), b"five").unwrap();
        s.start_at(now()).unwrap();
        let kept2 = root.with_file_name("current.corrupt-20261118-140559-2");
        assert_eq!(std::fs::read(kept2.join("005.png")).unwrap(), b"five");
        cleanup(&root);
    }

    /// Thư mục chưa khôi phục mà không đổi tên được (tệp đang bị giữ): không bắt đầu, không
    /// ghi đè, `end` không xóa gì.
    #[test]
    fn start_refuses_when_unrecovered_folder_cannot_be_moved() {
        let root = tmp("startlock");
        std::fs::create_dir_all(&root).unwrap();
        let mut sess = StepSession::default();
        sess.add("001.png".to_string()).unwrap();
        let body = sess.to_json();
        std::fs::write(root.join(SESSION_FILE), &body).unwrap();
        std::fs::write(root.join("001.png"), b"img").unwrap();
        let held = lock(&root.join(SESSION_FILE));
        let mut s = store(&root);
        // Đọc lỗi (không phải NotFound): không có phiên, không đụng gì.
        s.restore_at(now());
        assert!(!s.active());
        assert!(s.start_at(now()).is_err());
        assert!(!s.active());
        s.end().unwrap();
        drop(held);
        assert_eq!(
            std::fs::read_to_string(root.join(SESSION_FILE)).unwrap(),
            body
        );
        assert!(root.join("001.png").is_file());
        // Hết bị giữ: khôi phục bình thường.
        s.restore_at(now());
        assert!(s.active());
        assert_eq!(s.count(), 1);
        cleanup(&root);
    }

    /// Json hỏng mà không đổi tên được lúc khởi động: thư mục giữ nguyên, bắt đầu phiên bị từ chối.
    #[test]
    fn corrupt_json_that_cannot_be_moved_blocks_start() {
        let root = tmp("corruptlock");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(SESSION_FILE), b"{rac").unwrap();
        std::fs::write(root.join("001.png"), b"img").unwrap();
        let held = lock(&root.join("001.png"));
        let mut s = store(&root);
        s.restore_at(now());
        assert!(!s.active());
        assert!(root.join(SESSION_FILE).is_file());
        assert!(s.start_at(now()).is_err());
        drop(held);
        assert_eq!(std::fs::read(root.join(SESSION_FILE)).unwrap(), b"{rac");
        cleanup(&root);
    }

    /// Bước vừa thêm còn đang ghi (chưa vào danh sách): kết thúc phải hỏi; `settle` áp kết quả.
    #[test]
    fn write_in_flight_needs_end_confirm_and_settle_applies_it() {
        let root = tmp("settle");
        let mut s = store(&root);
        s.start_at(now()).unwrap();
        assert!(!s.needs_end_confirm());
        s.add(img(1)).unwrap();
        assert_eq!(s.count(), 0);
        assert!(s.needs_end_confirm(), "ảnh đang ghi: chưa được xuất");
        let d = s.settle(WAIT);
        assert_eq!((d.added, d.errors.len()), (1, 0));
        assert_eq!(s.count(), 1);
        assert!(s.needs_end_confirm());
        s.mark_exported();
        assert!(!s.needs_end_confirm());
        cleanup(&root);
    }

    /// Kết thúc mà còn sót tệp (bị giữ): thư mục còn tệp đánh dấu `ended`; lần bắt đầu sau xóa nốt
    /// thay vì giữ lại như thư mục hỏng.
    #[test]
    fn leftover_of_end_is_deleted_not_kept_as_corrupt() {
        let root = tmp("endleft");
        let mut s = store(&root);
        s.start_at(now()).unwrap();
        add_wait(&mut s, 1);
        let held = lock(&root.join("001.png"));
        assert!(s.end().is_err());
        assert!(!s.active());
        assert!(!root.join(SESSION_FILE).exists());
        assert!(
            root.join(ENDED_FILE).is_file(),
            "đánh dấu còn cùng phần sót"
        );
        drop(held);
        s.start_at(now()).unwrap();
        assert!(s.active());
        assert_eq!(names(&root), ["session.json"]);
        let base = root.parent().unwrap();
        assert!(
            names(base).iter().all(|n| !n.contains("corrupt")),
            "{:?}",
            names(base)
        );
        cleanup(&root);
    }

    /// Khởi động thấy phần sót của một lần kết thúc (có `ended`, không có json): xóa nốt.
    #[test]
    fn restore_deletes_leftover_of_end() {
        let root = tmp("restoreleft");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("002.png"), b"x").unwrap();
        std::fs::write(root.join(ENDED_FILE), b"").unwrap();
        let mut s = store(&root);
        s.restore_at(now());
        assert!(!s.active());
        assert!(!root.exists());
        cleanup(&root);
    }

    /// Đánh dấu `ended` cũ không gỡ được (bị giữ): không bắt đầu phiên, không để `session.json` nằm
    /// cạnh nó; gỡ được thì phiên mới chỉ có `session.json`.
    #[test]
    fn start_never_leaves_ended_marker_beside_session_json() {
        let root = tmp("startmarker");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(ENDED_FILE), b"").unwrap();
        let held = lock(&root.join(ENDED_FILE));
        let mut s = store(&root);
        assert!(s.start_at(now()).is_err());
        assert!(!s.active());
        assert!(!root.join(SESSION_FILE).exists());
        drop(held);
        s.start_at(now()).unwrap();
        assert!(s.active());
        assert_eq!(names(&root), ["session.json"]);
        cleanup(&root);
    }

    /// Chỉ coi `session.json` vắng khi hỏi được và nó không có.
    #[test]
    fn json_absent_only_when_known_missing() {
        assert!(json_absent(Ok(false)));
        assert!(!json_absent(Ok(true)));
        assert!(!json_absent(Err(std::io::Error::from(
            std::io::ErrorKind::PermissionDenied
        ))));
    }

    /// Kết thúc không xóa được json: phiên còn, không để lại đánh dấu `ended`.
    #[test]
    fn failed_end_keeps_no_ended_marker() {
        let root = tmp("endkeep");
        let mut s = store(&root);
        s.start_at(now()).unwrap();
        add_wait(&mut s, 1);
        let held = lock(&root.join(SESSION_FILE));
        assert!(s.end().is_err());
        assert!(s.active());
        assert!(!root.join(ENDED_FILE).exists());
        drop(held);
        cleanup(&root);
    }

    fn record_errors(e: &[StepError]) {
        ERRS.with(|v| v.borrow_mut().extend_from_slice(e));
    }

    thread_local! {
        static ERRS: std::cell::RefCell<Vec<StepError>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    /// Kết thúc của tiến trình: phần sót báo bằng lỗi `End` (không phải lỗi lưu phiên), phiên đã hết.
    #[test]
    fn end_reports_leftover_as_end_error() {
        let base =
            std::env::temp_dir().join(format!("darkshot_steps_{}_endrep", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("current");
        use_root_for_test(root.clone());
        set_on_error(record_errors);
        assert!(start());
        add(img(1)).unwrap();
        settle(WAIT);
        assert_eq!(count(), 1);
        let held = lock(&root.join("001.png"));
        assert!(end(), "phiên đã kết thúc dù còn sót tệp");
        assert!(!active());
        let errs = ERRS.with(|v| std::mem::take(&mut *v.borrow_mut()));
        assert_eq!(errs.len(), 1);
        assert!(matches!(errs[0], StepError::End(_)), "{errs:?}");
        drop(held);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn worker_failure_text_is_localized() {
        use darkshot_core::i18n::{t, Msg};
        assert_eq!(worker_failed(Lang::En), t(Lang::En, Msg::StepsWorkerFailed));
        assert_eq!(worker_failed(Lang::Vi), t(Lang::Vi, Msg::StepsWorkerFailed));
    }

    #[test]
    fn message_id_is_unique() {
        use windows::Win32::UI::WindowsAndMessaging::WM_APP;
        assert_eq!(WM_STEPS_SAVED, WM_APP + 13);
    }
}
