//! Lớp phủ chọn vùng macOS (Task 105): mỗi màn hình một cửa sổ phủ ảnh vừa chụp; kéo chọn vùng
//! bằng `Selection` của core (qua mô hình thuần `overlay_input::Model`); thanh Lưu/Chép/Hủy.
//!
//! Vòng đời (chỉ trên luồng chính): `open` dựng cửa sổ và giữ phiên trong `thread_local`; MỌI
//! đường đóng (nút, phím, `Esc`, đổi cấu hình màn hình, ứng dụng mất kích hoạt, lưới an toàn khi
//! không có tương tác) đi qua `finish`, hàm duy nhất gỡ cửa sổ và gọi `on_close` đúng một lần.
//! Đối tượng Objective-C của phiên đã đóng không hủy ngay (có thể ta đang ở trong phương thức của
//! chính chúng) mà chờ tới lượt kế của hàng đợi chính.

use crate::capture::DisplayShot;
use crate::coords::RectF;
use crate::overlay_input::{
    event_to_px, match_screens, Model, Mods, ScreenPx, ViewState, IDLE_CHECK_EVERY,
};
use crate::overlay_view::{make_window, OverlayView, OverlayWindow};
use darkshot_core::actions::Action;
use darkshot_core::geometry::Rect;
use darkshot_core::image::Image;
use dispatch2::{DispatchQueue, DispatchTime};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationDidChangeScreenParametersNotification,
    NSApplicationDidResignActiveNotification, NSCursor, NSScreen,
};
use objc2_core_foundation::{CFData, CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    kCGColorSpaceSRGB, CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage,
    CGImageAlphaInfo, CGImageByteOrderInfo,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObject, NSObjectProtocol};
use std::cell::{Cell, RefCell};
use std::time::Instant;

/// Kết quả của một lần mở lớp phủ.
#[derive(Debug)]
pub enum Outcome {
    /// Hủy (nút Hủy, `Esc`, đổi màn hình, mất kích hoạt, hết giờ) hoặc không có vùng.
    Cancelled,
    /// Hành động trên vùng `rect` (pixel) của ảnh màn hình `image`; bên gọi cắt ảnh ở luồng nền.
    Act {
        action: Action,
        image: Image,
        rect: Rect,
    },
}

/// Không mở được lớp phủ (không có cửa sổ nào được tạo hay còn lại).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenError {
    AlreadyOpen,
    /// Màn hình đã đổi sau khi chụp (ảnh không khớp `NSScreen`).
    ScreensChanged,
    /// Không dựng được `CGImage` từ ảnh chụp.
    Image,
}

/// Những gì view cần để vẽ một màn hình.
pub struct Frame {
    pub image: CFRetained<CGImage>,
    pub w: i32,
    pub h: i32,
    pub scale: f64,
    pub dpi: u32,
    pub state: ViewState,
}

struct Slot {
    window: Retained<OverlayWindow>,
    view: Retained<OverlayView>,
    image: CFRetained<CGImage>,
    /// Khung Cocoa của màn hình (điểm).
    frame: RectF,
    /// Số pixel mỗi điểm của ảnh màn hình này.
    scale: f64,
}

struct Session {
    id: u64,
    model: Model,
    slots: Vec<Slot>,
    /// Ảnh gốc của từng màn hình (cùng thứ tự `slots`), để cắt khi có hành động.
    images: Vec<Image>,
    observer: Retained<OverlayObserver>,
    on_close: fn(Outcome),
}

/// Đối tượng Objective-C của một phiên đã đóng, chờ hủy ở lượt kế của hàng đợi chính.
struct Retired {
    _slots: Vec<Slot>,
    _observer: Retained<OverlayObserver>,
}

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
    static RETIRED: RefCell<Vec<Retired>> = const { RefCell::new(Vec::new()) };
    static NEXT_ID: Cell<u64> = const { Cell::new(1) };
}

define_class!(
    // SAFETY: NSObject không có yêu cầu riêng khi kế thừa; lớp không có ivar, không cài `Drop`.
    // Chỉ dùng trên luồng chính (thông báo của NSApplication được gửi trên luồng chính).
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "DarkshotOverlayObserver"]
    struct OverlayObserver;

    unsafe impl NSObjectProtocol for OverlayObserver {}

    impl OverlayObserver {
        /// Cắm, rút, đổi độ phân giải hay sắp xếp màn hình: ảnh không còn khớp, hủy sạch.
        #[unsafe(method(screensChanged:))]
        fn screens_changed(&self, _note: &NSNotification) {
            cancel();
        }

        /// Chuyển sang ứng dụng khác (`⌘Tab`, Mission Control): không để lại lớp phủ.
        #[unsafe(method(appResigned:))]
        fn app_resigned(&self, _note: &NSNotification) {
            cancel();
        }
    }
);

impl OverlayObserver {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: `init` của NSObject trên đối tượng vừa cấp phát.
        unsafe { msg_send![super(this), init] }
    }
}

fn rect_f(r: CGRect) -> RectF {
    RectF {
        x: r.origin.x,
        y: r.origin.y,
        w: r.size.width,
        h: r.size.height,
    }
}

fn cg_rect(r: RectF) -> CGRect {
    CGRect::new(CGPoint::new(r.x, r.y), CGSize::new(r.w, r.h))
}

/// `CGImage` (sRGB, BGRA 8 bit, bỏ alpha) chép từ ảnh chụp.
fn cg_image(img: &Image) -> Option<CFRetained<CGImage>> {
    if !img.is_valid() {
        return None;
    }
    let (w, h) = (img.width as usize, img.height as usize);
    let data = CFData::from_bytes(&img.bgra);
    let provider = CGDataProvider::with_cf_data(Some(&data))?;
    // SAFETY: `kCGColorSpaceSRGB` là hằng của Core Graphics sống suốt tiến trình.
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    let info =
        CGBitmapInfo(CGImageAlphaInfo::NoneSkipFirst.0 | CGImageByteOrderInfo::Order32Little.0);
    // SAFETY: bộ đệm có đúng `w * 4 * h` byte (ảnh hợp lệ), mỗi điểm 32 bit BGRA; `decode` null
    // là hợp lệ; nhà cung cấp dữ liệu giữ bản chép `CFData` sống cùng ảnh.
    unsafe {
        CGImage::new(
            w,
            h,
            8,
            32,
            w * 4,
            Some(&space),
            info,
            Some(&provider),
            std::ptr::null(),
            false,
            CGColorRenderingIntent::RenderingIntentDefault,
        )
    }
}

/// Lớp phủ đang mở.
pub fn is_open() -> bool {
    SESSION.with(|s| s.try_borrow().map_or(true, |b| b.is_some()))
}

/// Mở lớp phủ trên mọi màn hình với các ảnh vừa chụp; `focus` là chỉ số ảnh của màn hình nhận
/// phím (màn hình chứa con trỏ). `on_close` được gọi đúng một lần khi lớp phủ đóng (chỉ khi mở
/// thành công); lỗi thì không có cửa sổ nào còn lại và bên gọi tự xử lý.
pub fn open(
    mtm: MainThreadMarker,
    shots: Vec<DisplayShot>,
    focus: Option<usize>,
    on_close: fn(Outcome),
) -> Result<(), OpenError> {
    if is_open() {
        return Err(OpenError::AlreadyOpen);
    }
    // Phiên trước chắc chắn đã xong (không ở trong phương thức của nó): hủy luôn đồ cũ.
    // Bất biến: chỉ an toàn vì `on_close` không bao giờ mở lại lớp phủ ngay trong cùng lời gọi
    // (`finish` chạy bên trong phương thức của view/đích thông báo đang nằm trong `RETIRED`);
    // `open` chỉ được gọi từ một lượt riêng của hàng đợi chính (kết quả chụp, `on_captured`).
    RETIRED.with(|r| r.borrow_mut().clear());
    let screens: Vec<RectF> = NSScreen::screens(mtm)
        .to_vec()
        .iter()
        .map(|s| rect_f(s.frame()))
        .collect();
    let shot_frames: Vec<RectF> = shots.iter().map(|s| s.frame_pt).collect();
    let map = match_screens(&shot_frames, &screens).ok_or(OpenError::ScreensChanged)?;
    let mut images = Vec::with_capacity(shots.len());
    for shot in &shots {
        images.push(cg_image(&shot.image).ok_or(OpenError::Image)?);
    }
    let mut slots = Vec::with_capacity(shots.len());
    for (i, ((image, &screen), shot)) in images.into_iter().zip(&map).zip(&shots).enumerate() {
        let frame = screens[screen];
        let (window, view) = make_window(mtm, cg_rect(frame), i);
        slots.push(Slot {
            window,
            view,
            image,
            frame,
            scale: shot.scale,
        });
    }
    let model = Model::new(
        shots
            .iter()
            .map(|s| ScreenPx {
                w: s.image.width as i32,
                h: s.image.height as i32,
                scale: s.scale,
            })
            .collect(),
        Instant::now(),
    );
    let observer = OverlayObserver::new(mtm);
    let center = NSNotificationCenter::defaultCenter();
    let target: &AnyObject = &observer;
    // SAFETY: đích có hai bộ chọn này (nhận một `NSNotification`); tên thông báo là hằng của
    // AppKit. Đích được gỡ trong `finish` trước khi được hủy.
    unsafe {
        center.addObserver_selector_name_object(
            target,
            sel!(screensChanged:),
            Some(NSApplicationDidChangeScreenParametersNotification),
            None,
        );
        center.addObserver_selector_name_object(
            target,
            sel!(appResigned:),
            Some(NSApplicationDidResignActiveNotification),
            None,
        );
    }
    let id = NEXT_ID.with(|n| n.replace(n.get().wrapping_add(1)));
    let views: Vec<(Retained<OverlayWindow>, Retained<OverlayView>)> = slots
        .iter()
        .map(|s| (s.window.clone(), s.view.clone()))
        .collect();
    let session = Session {
        id,
        model,
        slots,
        images: shots.into_iter().map(|s| s.image).collect(),
        observer,
        on_close,
    };
    SESSION.with(|s| *s.borrow_mut() = Some(session));

    // Hiện sau khi đã lưu phiên (view đọc phiên khi vẽ); không giữ mượn `SESSION` lúc gọi AppKit.
    // `finish` ẩn ứng dụng (`hide`) để trả tiêu điểm cho ứng dụng trước đó; cửa sổ của ứng dụng
    // đang ẩn không hiện ra, và `activate()` trên macOS 14+ có thể bị từ chối. Vì vậy bỏ ẩn
    // (không kích hoạt, không đòi hệ thống đồng ý) trước khi hiện lớp phủ.
    let app = NSApplication::sharedApplication(mtm);
    app.unhideWithoutActivation();
    for (w, _) in &views {
        w.orderFrontRegardless();
    }
    let focus = focus.filter(|i| *i < views.len()).unwrap_or(0);
    if let Some((w, v)) = views.get(focus) {
        w.makeKeyAndOrderFront(None);
        w.makeFirstResponder(Some(v));
    }
    // Ứng dụng nền (`accessory`) phải được kích hoạt thì cửa sổ mới nhận phím ngay.
    app.activate();
    NSCursor::crosshairCursor().set();
    // Nếu phiên đã bị đóng ngay trong các lời gọi trên (thông báo đồng bộ), lời gọi hiện cửa sổ
    // sau đó có thể đã hiện lại một cửa sổ đã đóng: ẩn hết để không còn cửa sổ mồ côi.
    let still_open = SESSION.with(|s| {
        s.try_borrow()
            .map_or(true, |b| b.as_ref().is_some_and(|s| s.id == id))
    });
    if still_open {
        schedule_idle(id);
    } else {
        for (w, _) in &views {
            w.orderOut(None);
        }
    }
    Ok(())
}

/// Hẹn kiểm lưới an toàn của phiên `id`.
fn schedule_idle(id: u64) {
    let when = DispatchTime::NOW.time(IDLE_CHECK_EVERY.as_nanos() as i64);
    if DispatchQueue::main()
        .after(when, move || idle_tick(id))
        .is_err()
    {
        eprintln!("darkshot: cannot schedule the overlay idle check");
    }
}

fn idle_tick(id: u64) {
    let state = SESSION.with(|s| {
        s.try_borrow()
            .map(|b| {
                b.as_ref()
                    .filter(|s| s.id == id)
                    .map(|s| s.model.idle_expired(Instant::now()))
            })
            .ok()
    });
    match state {
        // Phiên khác hoặc đã đóng: thôi.
        Some(None) => {}
        Some(Some(true)) => {
            eprintln!("darkshot: no input on the overlay for too long, closing it");
            cancel();
        }
        // Còn hạn, hoặc đang bận (không mượn được): kiểm lại sau.
        Some(Some(false)) | None => schedule_idle(id),
    }
}

/// Áp một sự kiện vào phiên; vẽ lại các màn hình có trạng thái đổi; làm hành động nếu có.
/// Trả `true` khi có hành động (lớp phủ đã đóng).
fn input(f: impl FnOnce(&mut Session, Instant) -> Option<Action>) -> bool {
    let res = SESSION.with(|cell| {
        let mut b = cell.try_borrow_mut().ok()?;
        let s = b.as_mut()?;
        let n = s.slots.len();
        let before: Vec<ViewState> = (0..n).map(|i| s.model.view_state(i)).collect();
        let act = f(s, Instant::now());
        let dirty: Vec<Retained<OverlayView>> = (0..n)
            .filter(|&i| s.model.view_state(i) != before[i])
            .map(|i| s.slots[i].view.clone())
            .collect();
        Some((act, dirty))
    });
    let Some((act, dirty)) = res else {
        return false;
    };
    for v in dirty {
        v.setNeedsDisplay(true);
    }
    match act {
        Some(a) => {
            finish(Some(a));
            true
        }
        None => false,
    }
}

fn px_of(s: &Session, i: usize, global: (f64, f64)) -> Option<darkshot_core::geometry::Point> {
    let slot = s.slots.get(i)?;
    Some(event_to_px(global, slot.frame, slot.scale))
}

pub fn mouse_down(i: usize, global: (f64, f64)) {
    input(|s, now| {
        let p = px_of(s, i, global)?;
        s.model.press(i, p, now);
        None
    });
}

pub fn mouse_dragged(i: usize, global: (f64, f64)) {
    input(|s, now| {
        let p = px_of(s, i, global)?;
        s.model.drag(p, now);
        None
    });
}

pub fn mouse_up(i: usize, global: (f64, f64)) {
    input(|s, now| {
        let p = px_of(s, i, global)?;
        s.model.release(p, now)
    });
}

pub fn mouse_moved(i: usize, global: (f64, f64)) {
    input(|s, now| {
        let p = px_of(s, i, global)?;
        s.model.hover(i, p, now);
        None
    });
}

/// Phím xuống; `true` khi phím được dùng (đã làm hành động).
pub fn key_down(code: u16, chars: Option<&str>, mods: Mods) -> bool {
    input(|s, now| s.model.key(code, chars, mods, now))
}

/// Dữ liệu vẽ của màn hình `i`.
pub fn frame(i: usize) -> Option<Frame> {
    SESSION.with(|cell| {
        let b = cell.try_borrow().ok()?;
        let s = b.as_ref()?;
        let slot = s.slots.get(i)?;
        let b = s.model.bounds(i)?;
        Some(Frame {
            image: slot.image.clone(),
            w: b.w,
            h: b.h,
            scale: slot.scale,
            dpi: s.model.dpi(i),
            state: s.model.view_state(i),
        })
    })
}

/// Hủy lớp phủ (nếu đang mở).
pub fn cancel() {
    finish(None);
}

/// Đường đóng duy nhất: gỡ mọi cửa sổ và đích thông báo, rồi gọi `on_close` một lần.
fn finish(action: Option<Action>) {
    let taken = SESSION.with(|cell| cell.try_borrow_mut().map(|mut b| b.take()));
    let session = match taken {
        Ok(Some(s)) => s,
        Ok(None) => return,
        Err(_) => {
            // Đang có người mượn phiên (không nên xảy ra): thử lại ở lượt kế, không bỏ lớp phủ.
            DispatchQueue::main().exec_async(move || finish(action));
            return;
        }
    };
    let Session {
        model,
        slots,
        images,
        observer,
        on_close,
        ..
    } = session;
    let target: &AnyObject = &observer;
    // SAFETY: gỡ đích đã đăng ký trong `open`; đích còn sống (giữ trong `Retired`).
    unsafe { NSNotificationCenter::defaultCenter().removeObserver(target) };
    for slot in &slots {
        slot.window.orderOut(None);
        slot.window.close();
    }
    RETIRED.with(|r| {
        r.borrow_mut().push(Retired {
            _slots: slots,
            _observer: observer,
        })
    });
    // Hủy ở lượt kế của hàng đợi chính, khi chắc chắn không còn ở trong phương thức của chúng.
    DispatchQueue::main().exec_async(|| RETIRED.with(|r| r.borrow_mut().clear()));
    NSCursor::arrowCursor().set();
    if let Some(mtm) = MainThreadMarker::new() {
        // Trả tiêu điểm cho ứng dụng trước đó (để dán ngay sau khi chép).
        NSApplication::sharedApplication(mtm).hide(None);
    }
    let outcome = match (action, model.active(), model.rect()) {
        (Some(a), Some(i), Some(rect)) if a != Action::Cancel => match images.into_iter().nth(i) {
            Some(image) => Outcome::Act {
                action: a,
                image,
                rect,
            },
            None => Outcome::Cancelled,
        },
        _ => Outcome::Cancelled,
    };
    on_close(outcome);
}
