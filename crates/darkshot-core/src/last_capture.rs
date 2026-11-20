//! Lần chụp vùng trước: vùng (tọa độ màn hình) và hành động đã chọn, để "Chụp lại vùng trước".

use crate::actions::Action;
use crate::geometry::{to_screen_coords, Point, Rect, MIN_SIZE};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LastCapture {
    /// Tọa độ MÀN HÌNH.
    pub rect: Rect,
    pub action: Action,
}

pub const FILE_VERSION: u32 = 1;

fn action_code(a: Action) -> Option<&'static str> {
    match a {
        Action::Save => Some("save"),
        Action::Copy => Some("copy"),
        Action::Upload => Some("upload"),
        Action::Pin => Some("pin"),
        Action::Cancel | Action::Ocr | Action::AddStep => None,
    }
}

fn action_from_code(s: &str) -> Option<Action> {
    match s {
        "save" => Some(Action::Save),
        "copy" => Some(Action::Copy),
        "upload" => Some(Action::Upload),
        "pin" => Some(Action::Pin),
        _ => None,
    }
}

pub fn to_json(l: &LastCapture) -> String {
    serde_json::json!({
        "version": FILE_VERSION,
        "x": l.rect.x,
        "y": l.rect.y,
        "w": l.rect.w,
        "h": l.rect.h,
        "action": action_code(l.action).unwrap_or("save"),
    })
    .to_string()
}

/// None khi JSON hỏng, version khác 1, w/h < MIN_SIZE, action lạ hoặc "cancel".
pub fn from_json(s: &str) -> Option<LastCapture> {
    let v: serde_json::Value = serde_json::from_str(s).ok()?;
    if v.get("version")?.as_u64()? != u64::from(FILE_VERSION) {
        return None;
    }
    let num = |k: &str| -> Option<i32> { i32::try_from(v.get(k)?.as_i64()?).ok() };
    let rect = Rect {
        x: num("x")?,
        y: num("y")?,
        w: num("w")?,
        h: num("h")?,
    };
    if rect.w < MIN_SIZE || rect.h < MIN_SIZE {
        return None;
    }
    let action = action_from_code(v.get("action")?.as_str()?)?;
    Some(LastCapture { rect, action })
}

/// Lần chụp trước sau một lần chụp vùng kết thúc bằng `action` với vùng `rect` (tọa độ màn hình):
/// Cancel, Ocr và AddStep giữ `prev` (OCR không phải lần chụp để lặp lại).
pub fn remember(prev: Option<LastCapture>, action: Action, rect: Rect) -> Option<LastCapture> {
    if matches!(action, Action::Cancel | Action::Ocr | Action::AddStep) {
        return prev;
    }
    Some(LastCapture { rect, action })
}

/// Vùng cắt (tọa độ ẢNH) để lặp lại; None khi vùng không giao màn hình nào hoặc phần còn trong
/// ảnh < MIN_SIZE. `monitors_screen` ở tọa độ màn hình.
pub fn repeat_rect(
    last: &LastCapture,
    origin: Point,
    img_w: i32,
    img_h: i32,
    monitors_screen: &[Rect],
) -> Option<Rect> {
    let (ox, oy) = (i64::from(origin.x), i64::from(origin.y));
    let r = last.rect;
    // Tọa độ ảnh, i64 để không tràn.
    let (x0, y0) = (i64::from(r.x) - ox, i64::from(r.y) - oy);
    let (x1, y1) = (x0 + i64::from(r.w), y0 + i64::from(r.h));
    let (cx0, cy0) = (x0.max(0), y0.max(0));
    let (cx1, cy1) = (x1.min(i64::from(img_w)), y1.min(i64::from(img_h)));
    if cx1 - cx0 < i64::from(MIN_SIZE) || cy1 - cy0 < i64::from(MIN_SIZE) {
        return None;
    }
    let touches = monitors_screen.iter().any(|m| {
        let mx0 = i64::from(m.x) - ox;
        let my0 = i64::from(m.y) - oy;
        let (mx1, my1) = (mx0 + i64::from(m.w), my0 + i64::from(m.h));
        cx0.max(mx0) < cx1.min(mx1) && cy0.max(my0) < cy1.min(my1)
    });
    if !touches {
        return None;
    }
    Some(Rect {
        x: i32::try_from(cx0).ok()?,
        y: i32::try_from(cy0).ok()?,
        w: i32::try_from(cx1 - cx0).ok()?,
        h: i32::try_from(cy1 - cy0).ok()?,
    })
}

/// Khung nhớ (tọa độ màn hình) sau một lượt chụp vùng kết thúc bằng `action` với khung `picked`:
/// tải lên, lưu, chép thì nhớ khung mới; hủy thì giữ khung cũ.
pub fn remember_after(prev: Option<Rect>, action: Action, picked: Rect) -> Option<Rect> {
    match action {
        Action::Upload | Action::Save | Action::Copy | Action::Pin => Some(picked),
        Action::Cancel | Action::Ocr | Action::AddStep => prev,
    }
}

/// Việc của một lượt "Chụp lại vùng trước" sau khi đã chụp màn hình.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepeatPlan {
    /// Chưa có lần chụp trước: mở lớp phủ như phím chụp vùng.
    Overlay,
    /// Vùng trước không còn trên màn hình: thông báo rồi mở lớp phủ trên cùng ảnh chụp.
    OverlayWithNotice,
    /// Cắt vùng (tọa độ ảnh) rồi chạy hành động.
    Run(Rect, Action),
}

/// Kế hoạch lặp lại `last` trên ảnh chụp có gốc `origin` (tọa độ màn hình), cỡ `img_w`×`img_h`,
/// các màn hình `monitors` ở tọa độ ẢNH (như `Screenshot.monitors`).
pub fn repeat_plan(
    last: Option<LastCapture>,
    origin: Point,
    img_w: i32,
    img_h: i32,
    monitors: &[Rect],
) -> RepeatPlan {
    let Some(last) = last else {
        return RepeatPlan::Overlay;
    };
    // `repeat_rect` nhận màn hình theo tọa độ màn hình và tự trừ gốc đúng một lần.
    let screens: Vec<Rect> = monitors
        .iter()
        .map(|m| to_screen_coords(*m, origin))
        .collect();
    match repeat_rect(&last, origin, img_w, img_h, &screens) {
        Some(r) => RepeatPlan::Run(r, last.action),
        None => RepeatPlan::OverlayWithNotice,
    }
}

/// Lần chụp trước mới sau một lượt chụp vùng kết thúc bằng `action` với vùng `screen` (tọa độ
/// màn hình), và có phải ghi tệp không: chỉ khi giá trị mới khác giá trị đã ghi thành công lên đĩa
/// (`on_disk`), để lần ghi lỗi trước được thử lại; Hủy không bao giờ ghi.
pub fn last_capture_after(
    prev: Option<LastCapture>,
    on_disk: Option<LastCapture>,
    action: Action,
    screen: Rect,
) -> (Option<LastCapture>, bool) {
    let next = remember(prev, action, screen);
    let write = action != Action::Cancel && next.is_some() && next != on_disk;
    (next, write)
}

/// Tệp lần chụp trước lớn hơn mức này là hỏng (tệp thật chỉ vài chục byte).
const LAST_CAPTURE_MAX_BYTES: u64 = 4096;

/// Đọc tệp lần chụp trước; thiếu, quá lớn hay hỏng thì `None` (im lặng).
pub fn read_last_capture(path: &Path) -> Option<LastCapture> {
    use std::io::Read;
    let f = std::fs::File::open(path).ok()?;
    let mut s = String::new();
    // Đọc tối đa một byte quá giới hạn để nhận ra tệp quá lớn mà không đọc hết.
    f.take(LAST_CAPTURE_MAX_BYTES + 1)
        .read_to_string(&mut s)
        .ok()?;
    if s.len() as u64 > LAST_CAPTURE_MAX_BYTES {
        return None;
    }
    from_json(&s)
}

/// Ghi tệp lần chụp trước nguyên tử: tệp tạm cùng thư mục, `sync_all`, đổi tên; tạo thư mục nếu
/// thiếu. Lỗi thì xóa tệp tạm và trả lỗi (bên gọi nuốt).
pub fn write_last_capture(path: &Path, l: &LastCapture) -> std::io::Result<()> {
    use std::io::Write;
    let dir = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".tmp");
    let tmp = dir.join(tmp_name);
    let write_tmp = || -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(to_json(l).as_bytes())?;
        f.sync_all()
    };
    if let Err(e) = write_tmp() {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rc(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }
    fn lc(rect: Rect, action: Action) -> LastCapture {
        LastCapture { rect, action }
    }
    fn p(x: i32, y: i32) -> Point {
        Point { x, y }
    }

    #[test]
    fn json_round_trip_all_actions() {
        for a in [Action::Save, Action::Copy, Action::Upload, Action::Pin] {
            let l = lc(rc(10, 20, 300, 200), a);
            assert_eq!(from_json(&to_json(&l)), Some(l));
        }
    }

    #[test]
    fn json_round_trip_negative_coords() {
        let l = lc(rc(-1500, -40, 300, 200), Action::Copy);
        assert_eq!(from_json(&to_json(&l)), Some(l));
    }

    #[test]
    fn from_json_rejects_bad_input() {
        let ok = r#"{"version":1,"x":1,"y":2,"w":30,"h":40,"action":"save"}"#;
        assert!(from_json(ok).is_some());
        for bad in [
            "",
            "rác",
            "{",
            "[]",
            "null",
            r#"{"version":2,"x":1,"y":2,"w":30,"h":40,"action":"save"}"#,
            r#"{"version":1,"x":1,"y":2,"w":2,"h":40,"action":"save"}"#,
            r#"{"version":1,"x":1,"y":2,"w":30,"h":2,"action":"save"}"#,
            r#"{"version":1,"x":1,"y":2,"w":30,"h":40,"action":"cancel"}"#,
            r#"{"version":1,"x":1,"y":2,"w":30,"h":40,"action":"lạ"}"#,
            r#"{"version":1,"x":1,"y":2,"w":30,"h":40}"#,
            r#"{"version":1,"x":1,"y":2,"w":30,"action":"save"}"#,
            r#"{"x":1,"y":2,"w":30,"h":40,"action":"save"}"#,
            r#"{"version":1,"x":99999999999,"y":2,"w":30,"h":40,"action":"save"}"#,
            r#"{"version":1,"x":"1","y":2,"w":30,"h":40,"action":"save"}"#,
        ] {
            assert_eq!(from_json(bad), None, "{bad}");
        }
    }

    #[test]
    fn remember_cancel_keeps_prev() {
        let r = rc(0, 0, 50, 50);
        assert_eq!(remember(None, Action::Cancel, r), None);
        let prev = lc(rc(5, 5, 10, 10), Action::Pin);
        assert_eq!(remember(Some(prev), Action::Cancel, r), Some(prev));
    }

    #[test]
    fn remember_ocr_keeps_prev() {
        let r = rc(1, 2, 30, 40);
        let prev = lc(rc(5, 5, 10, 10), Action::Pin);
        assert_eq!(remember(None, Action::Ocr, r), None);
        assert_eq!(remember(Some(prev), Action::Ocr, r), Some(prev));
    }

    #[test]
    fn remember_add_step_keeps_prev() {
        let r = rc(1, 2, 30, 40);
        let prev = lc(rc(5, 5, 10, 10), Action::Pin);
        assert_eq!(remember(None, Action::AddStep, r), None);
        assert_eq!(remember(Some(prev), Action::AddStep, r), Some(prev));
    }

    #[test]
    fn remember_replaces_prev() {
        let r = rc(0, 0, 50, 50);
        let prev = lc(rc(5, 5, 10, 10), Action::Pin);
        assert_eq!(
            remember(Some(prev), Action::Save, r),
            Some(lc(r, Action::Save))
        );
        assert_eq!(
            remember(None, Action::Upload, r),
            Some(lc(r, Action::Upload))
        );
    }

    #[test]
    fn repeat_rect_main_monitor() {
        let mons = [rc(0, 0, 1920, 1080)];
        let l = lc(rc(100, 100, 300, 200), Action::Save);
        assert_eq!(
            repeat_rect(&l, p(0, 0), 1920, 1080, &mons),
            Some(rc(100, 100, 300, 200))
        );
    }

    #[test]
    fn repeat_rect_negative_origin_subtracts_once() {
        let mons = [rc(-1920, 0, 1920, 1080), rc(0, 0, 1920, 1080)];
        let l = lc(rc(-1000, 100, 300, 200), Action::Save);
        assert_eq!(
            repeat_rect(&l, p(-1920, 0), 3840, 1080, &mons),
            Some(rc(920, 100, 300, 200))
        );
    }

    #[test]
    fn repeat_rect_removed_monitor_is_none() {
        // màn hình trái đã rút: ảnh chỉ còn màn hình chính
        let mons = [rc(0, 0, 1920, 1080)];
        let l = lc(rc(-1000, 100, 300, 200), Action::Save);
        assert_eq!(repeat_rect(&l, p(0, 0), 1920, 1080, &mons), None);
    }

    #[test]
    fn repeat_rect_clips_right_edge() {
        let mons = [rc(0, 0, 1920, 1080)];
        let l = lc(rc(1800, 100, 300, 200), Action::Save);
        assert_eq!(
            repeat_rect(&l, p(0, 0), 1920, 1080, &mons),
            Some(rc(1800, 100, 120, 200))
        );
        // còn đúng MIN_SIZE thì giữ
        let l = lc(rc(1917, 100, 300, 200), Action::Save);
        assert_eq!(
            repeat_rect(&l, p(0, 0), 1920, 1080, &mons),
            Some(rc(1917, 100, 3, 200))
        );
        // còn < MIN_SIZE thì bỏ
        let l = lc(rc(1918, 100, 300, 200), Action::Save);
        assert_eq!(repeat_rect(&l, p(0, 0), 1920, 1080, &mons), None);
    }

    #[test]
    fn repeat_rect_extreme_values_do_not_panic() {
        let mons = [rc(0, 0, 1920, 1080)];
        let l = lc(rc(i32::MIN, i32::MAX, i32::MAX, i32::MAX), Action::Save);
        assert_eq!(
            repeat_rect(&l, p(i32::MAX, i32::MIN), 1920, 1080, &mons),
            None
        );
        let l = lc(rc(i32::MAX, i32::MAX, i32::MAX, i32::MAX), Action::Save);
        assert_eq!(
            repeat_rect(&l, p(i32::MIN, i32::MIN), i32::MAX, i32::MAX, &mons),
            None
        );
    }

    fn temp_dir_for(name: &str) -> std::path::PathBuf {
        let d =
            std::env::temp_dir().join(format!("darkshot_lastcap_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn names_in(dir: &Path) -> Vec<std::ffi::OsString> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect()
    }

    #[test]
    fn repeat_plan_without_last_opens_overlay() {
        let mons = [rc(0, 0, 1920, 1080)];
        assert_eq!(
            repeat_plan(None, Point { x: 0, y: 0 }, 1920, 1080, &mons),
            RepeatPlan::Overlay
        );
    }

    #[test]
    fn repeat_plan_gone_region_notifies_then_overlay() {
        // Vùng nằm trên màn hình trái đã rút.
        let mons = [rc(0, 0, 1920, 1080)];
        let last = lc(rc(-1000, 100, 300, 200), Action::Save);
        assert_eq!(
            repeat_plan(Some(last), Point { x: 0, y: 0 }, 1920, 1080, &mons),
            RepeatPlan::OverlayWithNotice
        );
        // Còn dưới MIN_SIZE trong ảnh.
        let last = lc(rc(1918, 100, 300, 200), Action::Copy);
        assert_eq!(
            repeat_plan(Some(last), Point { x: 0, y: 0 }, 1920, 1080, &mons),
            RepeatPlan::OverlayWithNotice
        );
    }

    #[test]
    fn repeat_plan_valid_region_runs_same_action() {
        let mons = [rc(0, 0, 1920, 1080)];
        for a in [Action::Save, Action::Copy, Action::Upload, Action::Pin] {
            let last = lc(rc(100, 100, 300, 200), a);
            assert_eq!(
                repeat_plan(Some(last), Point { x: 0, y: 0 }, 1920, 1080, &mons),
                RepeatPlan::Run(rc(100, 100, 300, 200), a)
            );
        }
        // Một phần ra ngoài ảnh: cắt phần còn trong ảnh.
        let last = lc(rc(1800, 100, 300, 200), Action::Save);
        assert_eq!(
            repeat_plan(Some(last), Point { x: 0, y: 0 }, 1920, 1080, &mons),
            RepeatPlan::Run(rc(1800, 100, 120, 200), Action::Save)
        );
    }

    #[test]
    fn repeat_plan_negative_origin_subtracts_once() {
        // Màn hình trái ở x âm; `monitors` ở tọa độ ảnh như `Screenshot.monitors`.
        let origin = Point { x: -1920, y: -200 };
        let mons = [rc(0, 0, 1920, 1080), rc(1920, 200, 1920, 1080)];
        let last = lc(rc(-1000, 100, 300, 200), Action::Pin);
        assert_eq!(
            repeat_plan(Some(last), origin, 3840, 1280, &mons),
            RepeatPlan::Run(rc(920, 300, 300, 200), Action::Pin)
        );
        // Vùng trên màn hình chính (tọa độ màn hình dương).
        let last = lc(rc(10, 20, 300, 200), Action::Save);
        assert_eq!(
            repeat_plan(Some(last), origin, 3840, 1280, &mons),
            RepeatPlan::Run(rc(1930, 220, 300, 200), Action::Save)
        );
    }

    #[test]
    fn last_capture_after_writes_only_on_change() {
        let r = rc(1, 2, 30, 40);
        let prev = lc(r, Action::Save);
        // Hủy: giữ nguyên, không ghi.
        assert_eq!(
            last_capture_after(None, None, Action::Cancel, r),
            (None, false)
        );
        assert_eq!(
            last_capture_after(Some(prev), Some(prev), Action::Cancel, rc(9, 9, 50, 50)),
            (Some(prev), false)
        );
        // Nhận dạng chữ: như Hủy, giữ nguyên lần chụp trước, không ghi.
        assert_eq!(
            last_capture_after(Some(prev), Some(prev), Action::Ocr, rc(9, 9, 50, 50)),
            (Some(prev), false)
        );
        assert_eq!(
            last_capture_after(None, None, Action::Ocr, r),
            (None, false)
        );
        // Giống hệt giá trị đã ghi: không ghi lại.
        assert_eq!(
            last_capture_after(Some(prev), Some(prev), Action::Save, r),
            (Some(prev), false)
        );
        // Đổi vùng hoặc hành động: ghi.
        assert_eq!(
            last_capture_after(Some(prev), Some(prev), Action::Pin, r),
            (Some(lc(r, Action::Pin)), true)
        );
        assert_eq!(
            last_capture_after(None, None, Action::Copy, rc(5, 5, 60, 60)),
            (Some(lc(rc(5, 5, 60, 60), Action::Copy)), true)
        );
    }

    #[test]
    fn last_capture_after_retries_failed_write() {
        let r = rc(1, 2, 30, 40);
        let cur = lc(r, Action::Save);
        // Lần ghi trước lỗi: trên đĩa chưa có gì (hoặc giá trị cũ), chụp lại cùng vùng phải ghi.
        assert_eq!(
            last_capture_after(Some(cur), None, Action::Save, r),
            (Some(cur), true)
        );
        let old = lc(rc(9, 9, 50, 50), Action::Copy);
        assert_eq!(
            last_capture_after(Some(cur), Some(old), Action::Save, r),
            (Some(cur), true)
        );
        // Hủy vẫn không bao giờ ghi, kể cả khi trên đĩa đang cũ.
        assert_eq!(
            last_capture_after(Some(cur), None, Action::Cancel, r),
            (Some(cur), false)
        );
        assert_eq!(
            last_capture_after(Some(cur), Some(old), Action::Cancel, r),
            (Some(cur), false)
        );
    }

    #[test]
    fn last_capture_file_round_trip_creates_dir() {
        let root = temp_dir_for("roundtrip");
        let path = root.join("darkshot").join("last_capture.json");
        assert!(!root.exists());
        assert_eq!(read_last_capture(&path), None);
        let l = lc(rc(-1500, -40, 300, 200), Action::Upload);
        write_last_capture(&path, &l).unwrap();
        assert_eq!(read_last_capture(&path), Some(l));
        // Ghi đè tệp đã có.
        let l2 = lc(rc(7, 8, 90, 100), Action::Pin);
        write_last_capture(&path, &l2).unwrap();
        assert_eq!(read_last_capture(&path), Some(l2));
        // Không để lại tệp tạm.
        assert_eq!(
            names_in(path.parent().unwrap()),
            vec![std::ffi::OsString::from("last_capture.json")]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn last_capture_file_corrupt_is_none_and_kept() {
        let root = temp_dir_for("corrupt");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("last_capture.json");
        for bad in [
            "",
            "{",
            "null",
            r#"{"version":2,"x":1,"y":2,"w":30,"h":40,"action":"save"}"#,
        ] {
            std::fs::write(&path, bad).unwrap();
            assert_eq!(read_last_capture(&path), None, "{bad}");
            // Không xóa tệp hỏng.
            assert!(path.exists());
        }
        // Tệp quá lớn: bỏ qua.
        std::fs::write(&path, " ".repeat(64 * 1024)).unwrap();
        assert_eq!(read_last_capture(&path), None);
        // Đường dẫn là thư mục: None, không panic.
        assert_eq!(read_last_capture(&root), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn last_capture_write_failure_leaves_no_temp() {
        let root = temp_dir_for("fail");
        // Đích là một thư mục không rỗng: đổi tên thất bại, tệp tạm phải bị xóa.
        let path = root.join("last_capture.json");
        std::fs::create_dir_all(path.join("sub")).unwrap();
        let l = lc(rc(1, 2, 30, 40), Action::Save);
        assert!(write_last_capture(&path, &l).is_err());
        assert_eq!(
            names_in(&root),
            vec![std::ffi::OsString::from("last_capture.json")]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn selection_is_remembered_only_when_used() {
        let old = Some(Rect {
            x: 1,
            y: 2,
            w: 3,
            h: 4,
        });
        let new = Rect {
            x: 10,
            y: 20,
            w: 30,
            h: 40,
        };
        for a in [Action::Upload, Action::Save, Action::Copy, Action::Pin] {
            assert_eq!(remember_after(old, a, new), Some(new));
            assert_eq!(remember_after(None, a, new), Some(new));
        }
        assert_eq!(remember_after(old, Action::Cancel, new), old);
        assert_eq!(remember_after(None, Action::Cancel, new), None);
    }
}
