//! Ảnh ghim: cửa sổ nổi luôn trên cùng hiện một ảnh chụp, kéo để dời, cuộn để phóng/thu.
//! Mọi ảnh ghim sống trên luồng giao diện chính; sổ `PINS` chỉ giữ HWND.
//!
//! Quy tắc tái nhập: trạng thái `Pin` chỉ được mượn trong `with_pin`, và closure của nó không gọi
//! hàm Win32 nào có thể gửi thông điệp cho cửa sổ (SetWindowPos, DestroyWindow, SetCapture...).

use crate::clipboard::WinClipboard;
use crate::lang;
use crate::theme;
use crate::winutil;
use darkshot_core::actions::{Action, Clipboard};
use darkshot_core::geometry::{Point, Rect};
use darkshot_core::i18n::{t, Lang, Msg};
use darkshot_core::image::Image;
use darkshot_core::pin as core_pin;
use darkshot_core::theme::{ACCENT, DANGER_BUTTON, OVERLAY_BORDER, OVERLAY_LABEL, TEXT};
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use windows::core::{w, BOOL, HSTRING, PCWSTR};
use windows::Win32::Foundation::{
    GetLastError, COLORREF, ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, EndPaint,
    EnumDisplayMonitors, GetMonitorInfoW, InvalidateRect, MonitorFromPoint, MonitorFromRect,
    ScreenToClient, SelectObject, SetBrushOrgEx, SetStretchBltMode, StretchBlt, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, COLORONCOLOR, DIB_RGB_COLORS, DT_CALCRECT, DT_CENTER, DT_NOPREFIX,
    DT_SINGLELINE, DT_VCENTER, HALFTONE, HBITMAP, HDC, HFONT, HGDIOBJ, HMONITOR, MONITORINFO,
    MONITOR_DEFAULTTONEAREST, PAINTSTRUCT, SRCCOPY, STRETCH_BLT_MODE,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_ADD,
    VK_CONTROL, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_NUMPAD0, VK_OEM_MINUS, VK_OEM_PLUS, VK_RIGHT,
    VK_SHIFT, VK_SUBTRACT, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    EndMenu, GetClientRect, GetCursorPos, GetWindowLongPtrW, GetWindowRect, IsWindow, KillTimer,
    LoadCursorW, RegisterClassW, SetCursor, SetForegroundWindow, SetLayeredWindowAttributes,
    SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow, TrackPopupMenu, CS_DBLCLKS,
    GWLP_USERDATA, HMENU, HTCLIENT, IDC_ARROW, IDC_SIZEALL, LWA_ALPHA, MENU_ITEM_FLAGS, MF_CHECKED,
    MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, MINMAXINFO, SET_WINDOW_POS_FLAGS, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_SHOWNA, TPM_RETURNCMD, TPM_RIGHTBUTTON, WA_INACTIVE,
    WINDOWPOS, WM_ACTIVATE, WM_CAPTURECHANGED, WM_CONTEXTMENU, WM_DISPLAYCHANGE, WM_DPICHANGED,
    WM_ERASEBKGND, WM_GETMINMAXINFO, WM_KEYDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCACTIVATE, WM_NCDESTROY, WM_PAINT, WM_SETCURSOR, WM_TIMER,
    WM_WINDOWPOSCHANGED, WNDCLASSW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

const CLASS: PCWSTR = w!("darkshot.pin");
const BADGE_TIMER: usize = 1;
const BADGE_MS: u32 = 1200;
/// Một nấc bánh xe (`WHEEL_DELTA`).
const WHEEL_DELTA: i64 = 120;
/// Cờ phím Ctrl trong từ thấp của wParam của WM_MOUSEWHEEL.
const MK_CONTROL: usize = 0x0008;

const MENU_ZOOMS: [u16; 4] = [50, 100, 150, 200];
const MENU_OPACITIES: [u8; 4] = [100, 80, 60, 40];
const ID_COPY: u32 = 1;
const ID_SAVE: u32 = 2;
const ID_CLOSE: u32 = 3;
const ID_CLOSE_ALL: u32 = 4;
const ID_OCR: u32 = 5;
/// Đầu menu ngữ cảnh: mục hành động trên ảnh gốc, theo thứ tự (ID, chữ, hành động gửi cho sink).
const MENU_ACTIONS: [(u32, Msg, Action); 3] = [
    (ID_COPY, Msg::PinMenuCopy, Action::Copy),
    (ID_SAVE, Msg::PinMenuSave, Action::Save),
    (ID_OCR, Msg::PinMenuOcr, Action::Ocr),
];
const ID_ZOOM_BASE: u32 = 1000;
const ID_OPACITY_BASE: u32 = 2000;

/// Nơi đặt ảnh ghim mới.
pub enum PinPlace {
    /// Ảnh hiện 100% với góc trên trái của ẢNH tại điểm màn hình này (cửa sổ = ảnh nới một viền ra ngoài).
    At(Point),
    /// Căn giữa vùng làm việc của màn hình chứa con trỏ, mức `pin::fit_zoom`.
    CenterOnCursorMonitor,
}

pub use darkshot_core::notices::PinError;

/// Một chỗ chờ cho đúng một giá trị: đặt mới thì thay giá trị cũ, lấy ra đúng một lần.
#[derive(Debug)]
pub struct PendingPin<T>(Option<T>);

impl<T> PendingPin<T> {
    pub const fn new() -> Self {
        Self(None)
    }

    /// Đặt `v`; trả giá trị cũ (nếu có) để nơi gọi bỏ.
    pub fn put(&mut self, v: T) -> Option<T> {
        self.0.replace(v)
    }

    pub fn take(&mut self) -> Option<T> {
        self.0.take()
    }

    #[cfg(test)]
    pub fn is_waiting(&self) -> bool {
        self.0.is_some()
    }

    pub fn clear(&mut self) {
        self.0 = None;
    }
}

impl<T> Default for PendingPin<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Hàm nhận hành động Lưu/Chép từ ảnh ghim.
/// Hành động, bản sao ảnh gốc, khung cửa sổ ảnh ghim (tọa độ màn hình) để cửa sổ mới tránh che.
type ActionSink = fn(Action, Image, Option<Rect>);

thread_local! {
    /// Sổ ảnh ghim của luồng giao diện.
    static PINS: RefCell<Vec<HWND>> = const { RefCell::new(Vec::new()) };
    static REGISTERED: Cell<bool> = const { Cell::new(false) };
    static SINK: Cell<Option<ActionSink>> = const { Cell::new(None) };
    /// Đang trong `with_pin` (chặn mượn lồng nhau nếu có tái nhập ngoài dự kiến).
    static BUSY: Cell<bool> = const { Cell::new(false) };
    /// Một ảnh ghim đang mở menu ngữ cảnh.
    static MENU_OPEN: Cell<bool> = const { Cell::new(false) };
}

/// Trạng thái của một cửa sổ ảnh ghim, sở hữu qua `GWLP_USERDATA`.
struct Pin {
    /// DIB 32 bit từ trên xuống giữ điểm ảnh gốc (bản duy nhất của ảnh trong ảnh ghim).
    bitmap: HBITMAP,
    bits: *const u8,
    len: usize,
    img_w: u32,
    img_h: u32,
    /// Khung ẢNH (không kể viền) trên màn hình, pixel vật lý.
    img: Rect,
    zoom: u16,
    opacity: u8,
    dpi: u32,
    font: Option<HFONT>,
    active: bool,
    hover: bool,
    tracking: bool,
    over_close: bool,
    close_pressed: bool,
    /// Kéo: con trỏ lúc bấm và góc ảnh lúc bấm (màn hình).
    drag: Option<(Point, Point)>,
    wheel: i32,
    badge: Option<String>,
    /// Khung nhãn (tọa độ client) lúc đặt chữ nhãn: chỉ vẽ lại phần này khi nhãn hiện/ẩn.
    badge_box: Option<Rect>,
    /// Đang tự đặt vị trí (WM_WINDOWPOSCHANGED không đồng bộ lại khung ảnh).
    self_move: bool,
}

impl Pin {
    fn border(&self) -> i32 {
        border_px(self.dpi)
    }

    fn client(&self) -> Rect {
        let w = window_rect(self.img, self.border());
        Rect {
            x: 0,
            y: 0,
            w: w.w,
            h: w.h,
        }
    }

    fn close_rect(&self) -> Option<Rect> {
        close_button_visible(self.img.w, self.img.h, self.dpi)
            .then(|| close_button_rect(self.client(), self.dpi))
    }

    /// Khung nhãn chữ `text` trong client, đo bằng phông nhãn trên DC tạm.
    fn measure_badge(&self, text: &str) -> Option<Rect> {
        let wide: Vec<u16> = text.encode_utf16().collect();
        // SAFETY: DC bộ nhớ tạm tạo ở đây được xóa trên mọi đường.
        let size = unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return None;
            }
            let m = theme::draw_text(
                dc,
                &wide,
                &RECT::default(),
                TEXT,
                self.font,
                DT_SINGLELINE | DT_NOPREFIX | DT_CALCRECT,
            );
            let _ = DeleteDC(dc);
            (m.right - m.left, m.bottom - m.top)
        };
        badge_rect(image_rect_of(self.client(), self.border()), size, self.dpi)
    }

    fn corner(&self) -> Point {
        Point {
            x: self.img.x,
            y: self.img.y,
        }
    }

    /// Bản sao ảnh gốc (100%, không phụ thuộc mức phóng hay độ mờ).
    fn image_copy(&self) -> Image {
        // SAFETY: `bits` trỏ vào DIB `len` byte do Pin sở hữu, sống đến khi Pin bị hủy;
        // GDI chỉ đọc DIB này (nguồn của BitBlt/StretchBlt) nên không cần GdiFlush.
        let px = unsafe { std::slice::from_raw_parts(self.bits, self.len) };
        Image {
            width: self.img_w,
            height: self.img_h,
            bgra: px.to_vec(),
        }
    }
}

impl Drop for Pin {
    fn drop(&mut self) {
        // SAFETY: bitmap và phông do Pin tạo, chỉ Pin giữ; không còn chọn vào DC nào (DC vẽ tạm đã trả).
        unsafe {
            let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
            if let Some(f) = self.font.take() {
                let _ = DeleteObject(HGDIOBJ(f.0));
            }
        }
    }
}

fn pin_ptr(hwnd: HWND) -> *mut Pin {
    // SAFETY: chỉ đọc giá trị người dùng của cửa sổ.
    unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Pin }
}

/// Mượn trạng thái của `hwnd`. `None` khi cửa sổ không còn trạng thái hoặc đang mượn dở.
fn with_pin<R>(hwnd: HWND, f: impl FnOnce(&mut Pin) -> R) -> Option<R> {
    if BUSY.with(|b| b.replace(true)) {
        return None;
    }
    let p = pin_ptr(hwnd);
    // SAFETY: con trỏ do `open` đặt từ Box::into_raw, gỡ ra ở WM_NCDESTROY trước khi hủy; chỉ dùng
    // trên luồng giao diện; cờ BUSY bảo đảm không có hai tham chiếu &mut cùng lúc.
    let r = unsafe { p.as_mut() }.map(f);
    BUSY.with(|b| b.set(false));
    r
}

fn too_large() -> PinError {
    PinError::TooLarge
}

/// Tạo một ảnh ghim (sở hữu `image`). Gọi trên luồng giao diện chính.
pub fn open(image: Image, place: PinPlace) -> Result<(), PinError> {
    if !core_pin::can_add(count()) {
        return Err(PinError::Limit);
    }
    if !image.is_valid()
        || i32::try_from(image.width).is_err()
        || i32::try_from(image.height).is_err()
    {
        return Err(PinError::Create("invalid image".into()));
    }
    let (iw, ih) = (image.width, image.height);
    let (zoom, img) = match place {
        PinPlace::At(p) => initial_at(p, iw, ih).ok_or_else(too_large)?,
        PinPlace::CenterOnCursorMonitor => {
            let area = cursor_work_area();
            let z = core_pin::fit_zoom(iw, ih, area);
            if !core_pin::zoom_allowed(iw, ih, z) {
                return Err(too_large());
            }
            (z, core_pin::centered(iw, ih, z, area))
        }
    };
    ensure_class()?;
    let (bitmap, bits) = make_dib(&image)?;
    let len = image.bgra.len();
    drop(image);
    let dpi = dpi_for_rect(img);
    let border = border_px(dpi);
    let win = core_pin::keep_visible(window_rect(img, border), &monitors());
    let pin = Box::new(Pin {
        bitmap,
        bits,
        len,
        img_w: iw,
        img_h: ih,
        img: image_rect_of(win, border),
        zoom,
        opacity: 100,
        dpi,
        font: winutil::system_font(dpi),
        active: false,
        hover: false,
        tracking: false,
        over_close: false,
        close_pressed: false,
        drag: None,
        wheel: 0,
        badge: None,
        badge_box: None,
        self_move: false,
    });
    let caption = HSTRING::from(title(lang::current(), iw, ih));
    // SAFETY: tạo cửa sổ trên luồng giao diện; lỗi thì `pin` (bitmap, phông) tự giải phóng khi trả.
    let hwnd = unsafe {
        let hinst = GetModuleHandleW(None).map_err(|e| PinError::Create(e.to_string()))?;
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            CLASS,
            &caption,
            WS_POPUP,
            win.x,
            win.y,
            win.w,
            win.h,
            None,
            None,
            Some(hinst.into()),
            None,
        )
        .map_err(|e| PinError::Create(e.to_string()))?
    };
    // Cửa sổ layered chỉ hiện sau khi có thuộc tính; đặt ngay khi tạo.
    // SAFETY: hwnd vừa tạo ở luồng này.
    if let Err(e) = unsafe { SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA) } {
        // Chưa gắn trạng thái: hủy cửa sổ, `pin` tự giải phóng khi trả.
        destroy(hwnd);
        return Err(PinError::Create(e.to_string()));
    }
    // Ghi sổ trước khi gắn trạng thái: không ghi được thì hủy cửa sổ, `pin` tự giải phóng khi trả.
    let registered = PINS.with(|v| match v.try_borrow_mut() {
        Ok(mut v) => {
            v.push(hwnd);
            true
        }
        Err(_) => false,
    });
    if !registered {
        destroy(hwnd);
        return Err(PinError::Create("pin registry busy".into()));
    }
    // SAFETY: trao quyền sở hữu Box cho cửa sổ; WM_NCDESTROY lấy lại và hủy đúng một lần.
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(pin) as isize);
    }
    // DPI thật của cửa sổ (màn hình chứa phần lớn cửa sổ) có thể khác dự đoán.
    // SAFETY: hwnd là cửa sổ còn sống của luồng này.
    let real = norm_dpi(unsafe { GetDpiForWindow(hwnd) });
    if real != dpi {
        on_dpi_changed(hwnd, real);
    }
    // SAFETY: như trên; SW_SHOWNA không giành tiêu điểm.
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNA);
    }
    Ok(())
}

/// Menu ngữ cảnh của một ảnh ghim đang mở (vòng lặp modal của `TrackPopupMenu`).
pub fn menu_open() -> bool {
    MENU_OPEN.with(|m| m.get())
}

pub fn count() -> usize {
    PINS.with(|v| v.try_borrow().map(|v| v.len()).unwrap_or(0))
}

/// Khung ảnh (không kể viền, tọa độ màn hình) của mọi ảnh ghim, cho kiểm thử của nơi gọi.
#[cfg(test)]
pub fn image_rects() -> Vec<Rect> {
    handles()
        .into_iter()
        .filter_map(|h| with_pin(h, |p| p.img))
        .collect()
}

/// HWND của mọi ảnh ghim còn sống (bản chép của sổ).
fn handles() -> Vec<HWND> {
    PINS.with(|v| v.try_borrow().map(|v| v.clone()).unwrap_or_default())
}

/// Hủy mọi ảnh ghim (đồng bộ). Gọi được nhiều lần.
pub fn close_all() {
    if MENU_OPEN.with(|m| m.get()) {
        // Đóng menu ngữ cảnh đang mở trước khi hủy cửa sổ giữ nó.
        // SAFETY: EndMenu chỉ tác động menu đang mở của luồng này.
        unsafe {
            let _ = EndMenu();
        }
    }
    // Chép danh sách trước: DestroyWindow gửi WM_NCDESTROY, nơi sổ bị sửa.
    for h in handles() {
        // SAFETY: IsWindow chỉ hỏi tính hợp lệ của handle.
        if unsafe { IsWindow(Some(h)) }.as_bool() {
            destroy(h);
        }
    }
    // Lưới an toàn: bỏ handle chết (WM_NCDESTROY đã xóa trong trường hợp thường).
    PINS.with(|v| {
        if let Ok(mut v) = v.try_borrow_mut() {
            // SAFETY: IsWindow chỉ hỏi tính hợp lệ của handle.
            v.retain(|h| unsafe { IsWindow(Some(*h)) }.as_bool());
        }
    });
}

/// Hàm `main` đăng ký để Lưu/Chép từ ảnh ghim đi đúng đường `run_on_image` (thông báo, Lịch sử), và
/// Nhận dạng chữ mở cửa sổ kết quả.
pub fn set_action_sink(f: ActionSink) {
    SINK.with(|s| s.set(Some(f)));
}

// ---------------------------------------------------------------------------------------------
// Hình học và quy tắc thuần (không cần cửa sổ).

fn norm_dpi(dpi: u32) -> u32 {
    if dpi == 0 {
        96
    } else {
        dpi
    }
}

fn clamp_i32(v: i64) -> i32 {
    v.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

/// Quy đổi DIP sang pixel ở `dpi` (làm tròn gần nhất).
fn dip(v: i32, dpi: u32) -> i32 {
    clamp_i32((v as i64 * norm_dpi(dpi) as i64 + 48) / 96)
}

/// Bề dày viền: 1 DIP, tối thiểu 1 px.
fn border_px(dpi: u32) -> i32 {
    dip(1, dpi).max(1)
}

/// Nét dấu ×: 1,5 DIP làm tròn, tối thiểu 1 px.
fn x_pen_px(dpi: u32) -> i32 {
    clamp_i32((3 * norm_dpi(dpi) as i64 + 96) / 192).max(1)
}

/// Khung cửa sổ = khung ảnh nới `border` px mỗi phía (kẹp vào i32).
fn window_rect(image_rect: Rect, border: i32) -> Rect {
    let b = border as i64;
    Rect {
        x: clamp_i32(image_rect.x as i64 - b),
        y: clamp_i32(image_rect.y as i64 - b),
        w: clamp_i32(image_rect.w as i64 + 2 * b),
        h: clamp_i32(image_rect.h as i64 + 2 * b),
    }
}

/// Khung ảnh trong khung cửa sổ (ngược của `window_rect`).
fn image_rect_of(window: Rect, border: i32) -> Rect {
    let b = border as i64;
    Rect {
        x: clamp_i32(window.x as i64 + b),
        y: clamp_i32(window.y as i64 + b),
        w: clamp_i32(window.w as i64 - 2 * b),
        h: clamp_i32(window.h as i64 - 2 * b),
    }
}

/// Nút × trong vùng client: ô 20 DIP, cách mép trên và phải 4 DIP.
fn close_button_rect(client: Rect, dpi: u32) -> Rect {
    let (s, m) = (dip(20, dpi), dip(4, dpi));
    Rect {
        x: clamp_i32(client.x as i64 + client.w as i64 - m as i64 - s as i64),
        y: clamp_i32(client.y as i64 + m as i64),
        w: s,
        h: s,
    }
}

/// Nút × chỉ hiện khi ảnh hiển thị ≥ 48 DIP mỗi chiều.
fn close_button_visible(display_w: i32, display_h: i32, dpi: u32) -> bool {
    let min = dip(48, dpi);
    display_w >= min && display_h >= min
}

/// Khung nhãn mức/độ mờ giữa đáy `area` (khung ảnh trong client), cách đáy 8 DIP, đệm 8×4 DIP.
/// `None` khi ảnh nhỏ hơn nhãn cộng 8 DIP.
fn badge_rect(area: Rect, text_size: (i32, i32), dpi: u32) -> Option<Rect> {
    let (px, py, m) = (dip(8, dpi) as i64, dip(4, dpi) as i64, dip(8, dpi) as i64);
    let bw = text_size.0.max(0) as i64 + 2 * px;
    let bh = text_size.1.max(0) as i64 + 2 * py;
    if (area.w as i64) < bw + m || (area.h as i64) < bh + m {
        return None;
    }
    Some(Rect {
        x: clamp_i32(area.x as i64 + (area.w as i64 - bw).div_euclid(2)),
        y: clamp_i32(area.y as i64 + area.h as i64 - m - bh),
        w: clamp_i32(bw),
        h: clamp_i32(bh),
    })
}

/// Cách chép ảnh nguồn vào khung ảnh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Blit {
    /// 100%: chép thẳng.
    Copy,
    /// Thu nhỏ: `HALFTONE`.
    Halftone,
    /// Phóng to: `COLORONCOLOR` (điểm ảnh sắc).
    ColorOnColor,
}

fn stretch_mode(zoom: u16) -> Blit {
    match zoom {
        100 => Blit::Copy,
        z if z < 100 => Blit::Halftone,
        _ => Blit::ColorOnColor,
    }
}

/// Cộng `delta` của bánh xe vào phần dư `accum`: số bậc trọn `WHEEL_DELTA` và phần dư mới.
/// Đổi chiều thì bỏ phần dư cũ.
fn wheel_steps(accum: i32, delta: i32) -> (i32, i32) {
    let acc = if (accum > 0 && delta < 0) || (accum < 0 && delta > 0) {
        0
    } else {
        accum as i64
    };
    let total = acc + delta as i64;
    (clamp_i32(total / WHEEL_DELTA), (total % WHEEL_DELTA) as i32)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hit {
    Close,
    Image,
}

/// Điểm client `pt` rơi vào nút × (khi đang hiện, khung `close`) hay ảnh.
fn hit(pt: Point, close: Option<Rect>) -> Hit {
    let inside = |c: Rect| {
        let (x, y) = (pt.x as i64, pt.y as i64);
        x >= c.x as i64
            && x < c.x as i64 + c.w as i64
            && y >= c.y as i64
            && y < c.y as i64 + c.h as i64
    };
    match close {
        Some(c) if inside(c) => Hit::Close,
        _ => Hit::Image,
    }
}

/// Mục Thu phóng của menu: bật khi lõi cho phép mức này (cạnh và diện tích).
fn menu_zoom_enabled(w: u32, h: u32, zoom: u16) -> bool {
    core_pin::zoom_allowed(w, h, zoom)
}

/// Mức và khung ảnh ban đầu của `At(p)`: 100%, hoặc bậc lớn nhất dưới 100% vừa `MAX_SIDE`;
/// `None` khi cả mức nhỏ nhất vẫn quá lớn.
fn initial_at(p: Point, w: u32, h: u32) -> Option<(u16, Rect)> {
    let mut z = 100;
    while !core_pin::zoom_allowed(w, h, z) {
        z = core_pin::zoom_out(z)?;
    }
    let (dw, dh) = core_pin::display_size(w, h, z);
    Some((
        z,
        Rect {
            x: p.x,
            y: p.y,
            w: dw,
            h: dh,
        },
    ))
}

/// Điểm mở menu ngữ cảnh: tọa độ màn hình trong `lp`, hoặc `fallback` khi mở bằng bàn phím (`lp` = -1).
fn context_point(lp: isize, fallback: Point) -> Point {
    if lp as i32 == -1 {
        return fallback;
    }
    Point {
        x: (lp & 0xFFFF) as u16 as i16 as i32,
        y: ((lp >> 16) & 0xFFFF) as u16 as i16 as i32,
    }
}

/// Tiêu đề cửa sổ: "darkshot — Ảnh ghim W × H".
fn title(lang: Lang, w: u32, h: u32) -> String {
    format!("{} {w} \u{00D7} {h}", t(lang, Msg::PinWindowTitle))
}

/// Chữ của nhãn: "125%" hoặc "Độ mờ 60%".
fn badge_text(lang: Lang, zoom: Option<u16>, opacity: u8) -> String {
    match zoom {
        Some(z) => format!("{z}%"),
        None => darkshot_core::i18n::pin_opacity_badge(lang, opacity),
    }
}

// ---------------------------------------------------------------------------------------------
// Hệ thống: lớp cửa sổ, DIB, màn hình.

fn ensure_class() -> Result<(), PinError> {
    if REGISTERED.with(|r| r.get()) {
        return Ok(());
    }
    // SAFETY: đăng ký lớp trên luồng giao diện; lớp đã có (luồng khác đăng ký) là vô hại.
    unsafe {
        let hinst = GetModuleHandleW(None).map_err(|e| PinError::Create(e.to_string()))?;
        let wc = WNDCLASSW {
            style: CS_DBLCLKS,
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            lpszClassName: CLASS,
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            return Err(PinError::Create("RegisterClassW".into()));
        }
    }
    REGISTERED.with(|r| r.set(true));
    Ok(())
}

/// DIB 32 bit từ trên xuống chứa điểm ảnh của `image` (đã kiểm hợp lệ).
fn make_dib(image: &Image) -> Result<(HBITMAP, *const u8), PinError> {
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: image.width as i32,
            biHeight: -(image.height as i32), // âm: từ trên xuống
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut c_void = std::ptr::null_mut();
    // SAFETY: DIB mới có đúng width*height*4 byte == bgra.len() (ảnh hợp lệ); lỗi thì xóa bitmap.
    unsafe {
        let bmp = CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
            .map_err(|e| PinError::Create(e.to_string()))?;
        if bits.is_null() {
            let _ = DeleteObject(HGDIOBJ(bmp.0));
            return Err(PinError::Create("CreateDIBSection".into()));
        }
        std::ptr::copy_nonoverlapping(image.bgra.as_ptr(), bits as *mut u8, image.bgra.len());
        Ok((bmp, bits as *const u8))
    }
}

fn to_rect(r: &RECT) -> Rect {
    Rect {
        x: r.left,
        y: r.top,
        w: clamp_i32(r.right as i64 - r.left as i64),
        h: clamp_i32(r.bottom as i64 - r.top as i64),
    }
}

fn to_win(r: Rect) -> RECT {
    RECT {
        left: r.x,
        top: r.y,
        right: clamp_i32(r.x as i64 + r.w as i64),
        bottom: clamp_i32(r.y as i64 + r.h as i64),
    }
}

unsafe extern "system" fn collect_monitor(m: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `data` trỏ tới Vec<Rect> còn sống trong `monitors`; mi có cbSize đúng.
    unsafe {
        if GetMonitorInfoW(m, &mut mi).as_bool() {
            if let Some(v) = (data.0 as *mut Vec<Rect>).as_mut() {
                v.push(to_rect(&mi.rcMonitor));
            }
        }
    }
    BOOL(1)
}

/// Khung mọi màn hình đang gắn (pixel vật lý).
fn monitors() -> Vec<Rect> {
    let mut out: Vec<Rect> = Vec::new();
    // SAFETY: callback chỉ ghi vào `out`, sống đến hết lời gọi đồng bộ này.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect_monitor),
            LPARAM(&mut out as *mut Vec<Rect> as isize),
        );
    }
    out
}

/// Vùng làm việc của màn hình chứa (hoặc gần) con trỏ.
fn cursor_work_area() -> Rect {
    let c = cursor_pos();
    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: tham số ra là biến cục bộ có cbSize đúng.
    let ok = unsafe {
        let mon = MonitorFromPoint(POINT { x: c.x, y: c.y }, MONITOR_DEFAULTTONEAREST);
        GetMonitorInfoW(mon, &mut mi).as_bool()
    };
    if ok {
        to_rect(&mi.rcWork)
    } else {
        Rect {
            x: 0,
            y: 0,
            w: 1024,
            h: 768,
        }
    }
}

/// DPI của màn hình giao nhiều nhất với `r`.
fn dpi_for_rect(r: Rect) -> u32 {
    let rc = to_win(r);
    let (mut x, mut y) = (0u32, 0u32);
    // SAFETY: tham số là biến cục bộ.
    let ok = unsafe {
        let m = MonitorFromRect(&rc, MONITOR_DEFAULTTONEAREST);
        GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut x, &mut y).is_ok()
    };
    if ok {
        norm_dpi(x)
    } else {
        96
    }
}

// ---------------------------------------------------------------------------------------------
// Thao tác trên cửa sổ.

/// Vẽ lại phần `r` (tọa độ client) của cửa sổ.
fn invalidate_part(hwnd: HWND, r: Option<Rect>) {
    if let Some(r) = r {
        let rc = to_win(r);
        // SAFETY: hwnd là cửa sổ của luồng này; rc là biến cục bộ.
        unsafe {
            let _ = InvalidateRect(Some(hwnd), Some(&rc), false);
        }
    }
}

fn invalidate(hwnd: HWND) {
    // SAFETY: hwnd là cửa sổ của luồng này.
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

fn destroy(hwnd: HWND) {
    // SAFETY: hủy cửa sổ ảnh ghim của luồng này; dọn ở WM_NCDESTROY; người gọi không dùng trạng thái sau đó.
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

fn cursor_pos() -> Point {
    let mut pt = POINT::default();
    // SAFETY: tham số ra là biến cục bộ.
    unsafe {
        let _ = GetCursorPos(&mut pt);
    }
    Point { x: pt.x, y: pt.y }
}

fn lo_word(v: usize) -> u32 {
    (v & 0xFFFF) as u32
}

fn hi_i16(v: usize) -> i16 {
    ((v >> 16) & 0xFFFF) as u16 as i16
}

/// Điểm (có dấu) trong lParam của thông điệp chuột.
fn lparam_pt(lp: LPARAM) -> Point {
    Point {
        x: (lp.0 & 0xFFFF) as u16 as i16 as i32,
        y: ((lp.0 >> 16) & 0xFFFF) as u16 as i16 as i32,
    }
}

/// Đặt khung ảnh `img`: cửa sổ = nới viền; `keep` thì giữ trong màn hình; `resize` thì đổi cả cỡ.
/// Một lời gọi SetWindowPos, không mượn trạng thái trong lúc gọi.
fn place(hwnd: HWND, img: Rect, keep: bool, resize: bool) {
    let Some(border) = with_pin(hwnd, |p| p.border()) else {
        return;
    };
    let mut win = window_rect(img, border);
    if keep {
        win = core_pin::keep_visible(win, &monitors());
    }
    let Some(prev) = with_pin(hwnd, |p| {
        p.img = image_rect_of(win, border);
        std::mem::replace(&mut p.self_move, true)
    }) else {
        return;
    };
    let mut flags: SET_WINDOW_POS_FLAGS = SWP_NOACTIVATE | SWP_NOZORDER;
    if !resize {
        flags |= SWP_NOSIZE;
    }
    // SAFETY: đổi vị trí (và cỡ) chính cửa sổ này.
    unsafe {
        let _ = SetWindowPos(hwnd, None, win.x, win.y, win.w, win.h, flags);
    }
    with_pin(hwnd, |p| p.self_move = prev);
    if resize {
        invalidate(hwnd);
    }
}

fn keep(hwnd: HWND) {
    if let Some(img) = with_pin(hwnd, |p| p.img) {
        place(hwnd, img, true, false);
    }
}

fn show_badge(hwnd: HWND, text: String) {
    let Some((old, new)) = with_pin(hwnd, |p| {
        let new = p.measure_badge(&text);
        p.badge = Some(text);
        (std::mem::replace(&mut p.badge_box, new), new)
    }) else {
        return;
    };
    // SAFETY: hẹn giờ của chính cửa sổ này; đặt lại cùng ID thì thay hẹn giờ cũ.
    unsafe {
        SetTimer(Some(hwnd), BADGE_TIMER, BADGE_MS, None);
    }
    invalidate_part(hwnd, old);
    invalidate_part(hwnd, new);
}

/// Đổi sang mức `target` giữ `anchor` (màn hình) đứng yên; mức không đổi thì chỉ hiện nhãn.
fn zoom_to(hwnd: HWND, target: u16, anchor: Point) {
    let Some(next) = with_pin(hwnd, |p| {
        if target == p.zoom || !menu_zoom_enabled(p.img_w, p.img_h, target) {
            return None;
        }
        let r = core_pin::zoom_about(p.img, anchor, p.img_w, p.img_h, target);
        p.zoom = target;
        Some(r)
    }) else {
        return;
    };
    if let Some(r) = next {
        place(hwnd, r, true, true);
    }
    if let Some(z) = with_pin(hwnd, |p| p.zoom) {
        show_badge(hwnd, badge_text(lang::current(), Some(z), 100));
    }
}

/// Phóng (`steps` > 0) hoặc thu từng bậc quanh `anchor`.
fn zoom_steps(hwnd: HWND, steps: i32, anchor: Point) {
    let Some(target) = with_pin(hwnd, |p| {
        let mut z = p.zoom;
        for _ in 0..steps.unsigned_abs().min(core_pin::ZOOM_STEPS.len() as u32) {
            let n = if steps > 0 {
                core_pin::zoom_in(p.img_w, p.img_h, z)
            } else {
                core_pin::zoom_out(z)
            };
            match n {
                Some(n) => z = n,
                None => break,
            }
        }
        z
    }) else {
        return;
    };
    zoom_to(hwnd, target, anchor);
}

fn center_of(hwnd: HWND) -> Option<Point> {
    with_pin(hwnd, |p| Point {
        x: clamp_i32(p.img.x as i64 + p.img.w as i64 / 2),
        y: clamp_i32(p.img.y as i64 + p.img.h as i64 / 2),
    })
}

fn set_opacity(hwnd: HWND, percent: u8) {
    let percent = percent.clamp(core_pin::OPACITY_MIN, 100);
    if with_pin(hwnd, |p| p.opacity = percent).is_none() {
        return;
    }
    // SAFETY: cửa sổ layered của luồng này.
    unsafe {
        let _ = SetLayeredWindowAttributes(
            hwnd,
            COLORREF(0),
            core_pin::opacity_alpha(percent),
            LWA_ALPHA,
        );
    }
    show_badge(hwnd, badge_text(lang::current(), None, percent));
}

fn opacity_steps(hwnd: HWND, steps: i32) {
    let Some(o) = with_pin(hwnd, |p| {
        let mut o = p.opacity;
        for _ in 0..steps.unsigned_abs().min(10) {
            o = core_pin::opacity_step(o, steps > 0);
        }
        o
    }) else {
        return;
    };
    set_opacity(hwnd, o);
}

fn delete_font(f: Option<HFONT>) {
    if let Some(f) = f {
        // SAFETY: phông do CreateFontIndirectW tạo, không còn chọn vào DC nào.
        unsafe {
            let _ = DeleteObject(HGDIOBJ(f.0));
        }
    }
}

/// DPI mới: giữ góc trên trái và cỡ ẢNH (điểm ảnh); chỉ viền, nút ×, nhãn, phông theo DPI mới.
fn on_dpi_changed(hwnd: HWND, dpi: u32) {
    let font = winutil::system_font(dpi);
    let Some((img, old)) = with_pin(hwnd, |p| {
        p.dpi = dpi;
        (p.img, std::mem::replace(&mut p.font, font))
    }) else {
        delete_font(font);
        return;
    };
    delete_font(old);
    place(hwnd, img, true, true);
    // Phông và viền mới: đo lại khung nhãn đang hiện (cả cửa sổ đã được vẽ lại).
    with_pin(hwnd, |p| {
        let b = p.badge.as_deref().and_then(|t| p.measure_badge(t));
        p.badge_box = b;
    });
}

/// Hành động của mục menu `id` trong `MENU_ACTIONS`.
fn menu_action(id: u32) -> Option<Action> {
    MENU_ACTIONS
        .iter()
        .find(|(i, _, _)| *i == id)
        .map(|(_, _, a)| *a)
}

/// Khung cửa sổ `hwnd` (tọa độ màn hình).
fn screen_rect_of(hwnd: HWND) -> Option<Rect> {
    let mut rc = RECT::default();
    // SAFETY: rc là biến cục bộ; hwnd là ảnh ghim của luồng này.
    unsafe { GetWindowRect(hwnd, &mut rc) }.ok()?;
    Some(Rect {
        x: rc.left,
        y: rc.top,
        w: rc.right - rc.left,
        h: rc.bottom - rc.top,
    })
}

/// Chép, Lưu hay Nhận dạng chữ trên bản sao ảnh gốc: gửi cho sink của `main`.
fn copy_or_save(hwnd: HWND, action: Action) {
    let Some(img) = with_pin(hwnd, |p| p.image_copy()) else {
        return;
    };
    match SINK.with(|s| s.get()) {
        Some(f) => f(action, img, screen_rect_of(hwnd)),
        None => {
            if action == Action::Copy {
                let _ = WinClipboard.set_image(&img);
            }
        }
    }
}

/// Menu sở hữu tạm thời: tự hủy (kèm menu con đã gắn) khi ra khỏi phạm vi.
struct OwnedMenu(Option<HMENU>);

impl Drop for OwnedMenu {
    fn drop(&mut self) {
        if let Some(m) = self.0.take() {
            // SAFETY: menu do CreatePopupMenu tạo, chưa thuộc menu nào khác; DestroyMenu hủy cả menu con.
            unsafe {
                let _ = DestroyMenu(m);
            }
        }
    }
}

fn new_menu() -> OwnedMenu {
    // SAFETY: tạo menu bật lên rỗng.
    OwnedMenu(unsafe { CreatePopupMenu() }.ok())
}

fn append(menu: HMENU, flags: MENU_ITEM_FLAGS, id: usize, text: &str) -> bool {
    let s = HSTRING::from(text);
    // SAFETY: menu còn sống; chuỗi sống suốt lời gọi (hệ thống chép lại).
    unsafe { AppendMenuW(menu, flags, id, &s).is_ok() }
}

/// Gắn `sub` làm menu con của `menu`; thành công thì `menu` sở hữu nó, lỗi thì `sub` tự hủy.
fn append_sub(menu: HMENU, mut sub: OwnedMenu, text: &str) {
    if let Some(s) = sub.0 {
        if append(menu, MF_POPUP | MF_STRING, s.0 as usize, text) {
            sub.0 = None;
        }
    }
}

fn context_menu(hwnd: HWND, lp: LPARAM) {
    let Some((corner, zoom, opacity, w, h)) =
        with_pin(hwnd, |p| (p.corner(), p.zoom, p.opacity, p.img_w, p.img_h))
    else {
        return;
    };
    let pt = context_point(lp.0, corner);
    let lang = lang::current();
    let root = new_menu();
    let Some(menu) = root.0 else {
        return;
    };
    for (id, text, _) in MENU_ACTIONS {
        append(menu, MF_STRING, id as usize, t(lang, text));
    }
    append(menu, MF_SEPARATOR, 0, "");
    let zm = new_menu();
    if let Some(m) = zm.0 {
        for z in MENU_ZOOMS {
            let mut f = MF_STRING;
            if z == zoom {
                f |= MF_CHECKED;
            }
            if !menu_zoom_enabled(w, h, z) {
                f |= MF_GRAYED;
            }
            append(m, f, (ID_ZOOM_BASE + z as u32) as usize, &format!("{z}%"));
        }
    }
    append_sub(menu, zm, t(lang, Msg::PinMenuZoom));
    let om = new_menu();
    if let Some(m) = om.0 {
        for o in MENU_OPACITIES {
            let f = if o == opacity {
                MF_STRING | MF_CHECKED
            } else {
                MF_STRING
            };
            append(
                m,
                f,
                (ID_OPACITY_BASE + o as u32) as usize,
                &format!("{o}%"),
            );
        }
    }
    append_sub(menu, om, t(lang, Msg::PinMenuOpacity));
    append(menu, MF_SEPARATOR, 0, "");
    append(
        menu,
        MF_STRING,
        ID_CLOSE as usize,
        t(lang, Msg::PinMenuClose),
    );
    append(
        menu,
        MF_STRING,
        ID_CLOSE_ALL as usize,
        t(lang, Msg::PinMenuCloseAll),
    );
    MENU_OPEN.with(|m| m.set(true));
    // SAFETY: menu còn sống đến khi `root` bị hủy; hwnd là cửa sổ của luồng này.
    let cmd = unsafe {
        let _ = SetForegroundWindow(hwnd);
        TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            pt.x,
            pt.y,
            None,
            hwnd,
            None,
        )
        .0 as u32
    };
    MENU_OPEN.with(|m| m.set(false));
    drop(root);
    // Cửa sổ có thể đã bị hủy trong vòng lặp của menu (close_all từ nơi khác).
    // SAFETY: IsWindow chỉ hỏi tính hợp lệ của handle.
    if cmd == 0 || !unsafe { IsWindow(Some(hwnd)) }.as_bool() || pin_ptr(hwnd).is_null() {
        return;
    }
    if let Some(action) = menu_action(cmd) {
        copy_or_save(hwnd, action);
        return;
    }
    match cmd {
        ID_CLOSE => destroy(hwnd),
        // Sau khi TrackPopupMenu đã trả về: không hủy cửa sổ đang giữ menu.
        ID_CLOSE_ALL => close_all(),
        c if c > ID_OPACITY_BASE && c <= ID_OPACITY_BASE + 100 => {
            set_opacity(hwnd, (c - ID_OPACITY_BASE) as u8);
        }
        c if c > ID_ZOOM_BASE && c < ID_OPACITY_BASE => {
            if let Some(a) = center_of(hwnd) {
                zoom_to(hwnd, (c - ID_ZOOM_BASE) as u16, a);
            }
        }
        _ => {}
    }
}

fn key_down(vk: u16) -> bool {
    // SAFETY: chỉ đọc trạng thái phím của luồng.
    unsafe { GetKeyState(vk as i32) < 0 }
}

const VK_C: u16 = 0x43;
const VK_S: u16 = 0x53;
const VK_W: u16 = 0x57;
const VK_0: u16 = 0x30;

/// `true` khi phím đã được xử lý.
fn on_key(hwnd: HWND, vk: u16) -> bool {
    let ctrl = key_down(VK_CONTROL.0);
    match vk {
        v if v == VK_ESCAPE.0 => destroy(hwnd),
        VK_W if ctrl => destroy(hwnd),
        VK_C if ctrl => copy_or_save(hwnd, Action::Copy),
        VK_S if ctrl => copy_or_save(hwnd, Action::Save),
        v if v == VK_OEM_PLUS.0 || v == VK_ADD.0 || v == VK_OEM_MINUS.0 || v == VK_SUBTRACT.0 => {
            let up = v == VK_OEM_PLUS.0 || v == VK_ADD.0;
            if let Some(a) = center_of(hwnd) {
                zoom_steps(hwnd, if up { 1 } else { -1 }, a);
            }
        }
        v if v == VK_0 || v == VK_NUMPAD0.0 => {
            if let Some(a) = center_of(hwnd) {
                zoom_to(hwnd, 100, a);
            }
        }
        v if v == VK_LEFT.0 || v == VK_RIGHT.0 || v == VK_UP.0 || v == VK_DOWN.0 => {
            let d: i64 = if key_down(VK_SHIFT.0) { 10 } else { 1 };
            let (dx, dy) = if v == VK_LEFT.0 {
                (-d, 0)
            } else if v == VK_RIGHT.0 {
                (d, 0)
            } else if v == VK_UP.0 {
                (0, -d)
            } else {
                (0, d)
            };
            if let Some(img) = with_pin(hwnd, |p| Rect {
                x: clamp_i32(p.img.x as i64 + dx),
                y: clamp_i32(p.img.y as i64 + dy),
                ..p.img
            }) {
                place(hwnd, img, true, false);
            }
        }
        _ => return false,
    }
    true
}

fn on_mouse_move(hwnd: HWND, lp: LPARAM) {
    if let Some((start, origin)) = with_pin(hwnd, |p| p.drag).flatten() {
        let cur = cursor_pos();
        let Some(img) = with_pin(hwnd, |p| Rect {
            x: clamp_i32(origin.x as i64 + cur.x as i64 - start.x as i64),
            y: clamp_i32(origin.y as i64 + cur.y as i64 - start.y as i64),
            ..p.img
        }) else {
            return;
        };
        // Kéo tự do giữa các màn hình; giữ trong màn hình khi nhả.
        place(hwnd, img, false, false);
        return;
    }
    let pt = lparam_pt(lp);
    let Some((need_track, changed)) = with_pin(hwnd, |p| {
        let close = p.close_rect();
        let over = hit(pt, close) == Hit::Close;
        let changed = !p.hover || over != p.over_close;
        p.hover = true;
        p.over_close = over;
        (
            !std::mem::replace(&mut p.tracking, true),
            close.filter(|_| changed),
        )
    }) else {
        return;
    };
    if need_track {
        let mut tme = TRACKMOUSEEVENT {
            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: hwnd,
            dwHoverTime: 0,
        };
        // SAFETY: tme là biến cục bộ hợp lệ; hwnd là cửa sổ của luồng này.
        unsafe {
            let _ = TrackMouseEvent(&mut tme);
        }
    }
    // Chỉ nút × đổi khi rê chuột.
    invalidate_part(hwnd, changed);
}

/// Kết thúc kéo (nếu đang kéo) rồi giữ cửa sổ trong màn hình.
fn end_drag(hwnd: HWND, release: bool) {
    if !with_pin(hwnd, |p| p.drag.take().is_some()).unwrap_or(false) {
        return;
    }
    if release {
        // SAFETY: trả chuột đã bắt ở WM_LBUTTONDOWN; WM_CAPTURECHANGED lồng sẽ thấy drag = None.
        unsafe {
            let _ = ReleaseCapture();
        }
    }
    keep(hwnd);
}

fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: cặp BeginPaint/EndPaint của chính cửa sổ này.
    let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
    if !hdc.is_invalid() {
        let mut rc = RECT::default();
        // SAFETY: chỉ đọc khung client.
        unsafe {
            let _ = GetClientRect(hwnd, &mut rc);
        }
        let (client, dirty) = (to_rect(&rc), ps.rcPaint);
        // Vẽ không gửi thông điệp nào cho cửa sổ, nên mượn trạng thái suốt lần vẽ là an toàn.
        with_pin(hwnd, |p| {
            theme::buffered(hdc, &dirty, |dc| draw(dc, p, client))
        });
    }
    // SAFETY: như trên.
    unsafe {
        let _ = EndPaint(hwnd, &ps);
    }
}

fn draw(dc: HDC, p: &Pin, client: Rect) {
    let edge = if p.active { ACCENT } else { OVERLAY_BORDER };
    theme::fill(dc, &to_win(client), edge);
    let area = image_rect_of(client, p.border());
    if area.w > 0 && area.h > 0 {
        blit(dc, p, area);
    }
    if p.hover {
        if let Some(c) =
            close_button_visible(area.w, area.h, p.dpi).then(|| close_button_rect(client, p.dpi))
        {
            draw_close(dc, p, c);
        }
    }
    if let Some(text) = &p.badge {
        draw_badge(dc, p, area, text);
    }
}

fn draw_close(dc: HDC, p: &Pin, c: Rect) {
    let bg = if p.over_close {
        DANGER_BUTTON
    } else {
        OVERLAY_LABEL
    };
    let rc = to_win(c);
    theme::fill(dc, &rc, bg);
    let i = dip(6, p.dpi);
    let pen = x_pen_px(p.dpi);
    let (l, t, r, b) = (rc.left + i, rc.top + i, rc.right - i, rc.bottom - i);
    theme::polyline(dc, &[POINT { x: l, y: t }, POINT { x: r, y: b }], TEXT, pen);
    theme::polyline(
        dc,
        &[POINT { x: r - 1, y: t }, POINT { x: l - 1, y: b }],
        TEXT,
        pen,
    );
}

fn draw_badge(dc: HDC, p: &Pin, area: Rect, text: &str) {
    let wide: Vec<u16> = text.encode_utf16().collect();
    let flags = DT_SINGLELINE | DT_NOPREFIX;
    let m = theme::draw_text(
        dc,
        &wide,
        &RECT::default(),
        TEXT,
        p.font,
        flags | DT_CALCRECT,
    );
    let size = (m.right - m.left, m.bottom - m.top);
    if let Some(br) = badge_rect(area, size, p.dpi) {
        let rc = to_win(br);
        theme::round_rect(dc, &rc, dip(4, p.dpi), OVERLAY_LABEL, OVERLAY_LABEL, 0);
        theme::draw_text(dc, &wide, &rc, TEXT, p.font, flags | DT_CENTER | DT_VCENTER);
    }
}

/// Chép ảnh nguồn vào `area` theo mức phóng.
fn blit(dc: HDC, p: &Pin, area: Rect) {
    let (sw, sh) = (p.img_w as i32, p.img_h as i32);
    // SAFETY: DC tạm tạo ở đây được trả bitmap cũ và xóa trên mọi đường; bitmap thuộc Pin.
    unsafe {
        let mem = CreateCompatibleDC(Some(dc));
        if mem.is_invalid() {
            return;
        }
        let old = SelectObject(mem, HGDIOBJ(p.bitmap.0));
        if area.w == sw && area.h == sh {
            let _ = BitBlt(dc, area.x, area.y, sw, sh, Some(mem), 0, 0, SRCCOPY);
        } else {
            let halftone = match stretch_mode(p.zoom) {
                Blit::Halftone => true,
                Blit::ColorOnColor => false,
                Blit::Copy => area.w < sw,
            };
            let prev = SetStretchBltMode(dc, if halftone { HALFTONE } else { COLORONCOLOR });
            if halftone {
                // Tài liệu SetStretchBltMode: đặt lại gốc chổi sau khi chọn HALFTONE.
                let _ = SetBrushOrgEx(dc, 0, 0, None);
            }
            let _ = StretchBlt(
                dc,
                area.x,
                area.y,
                area.w,
                area.h,
                Some(mem),
                0,
                0,
                sw,
                sh,
                SRCCOPY,
            );
            SetStretchBltMode(dc, STRETCH_BLT_MODE(prev));
        }
        SelectObject(mem, old);
        let _ = DeleteDC(mem);
    }
}

fn on_nc_destroy(hwnd: HWND) {
    // Gỡ trạng thái trước khi hủy: từ đây không ai thấy con trỏ nữa.
    // SAFETY: đặt lại giá trị người dùng và hủy hẹn giờ (nếu có) của chính cửa sổ này.
    let ptr = unsafe {
        let ptr = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) as *mut Pin;
        let _ = KillTimer(Some(hwnd), BADGE_TIMER);
        ptr
    };
    PINS.with(|v| {
        if let Ok(mut v) = v.try_borrow_mut() {
            v.retain(|h| *h != hwnd);
        }
    });
    if !ptr.is_null() {
        // SAFETY: con trỏ do Box::into_raw trong `open`, vừa gỡ khỏi cửa sổ nên chỉ hủy một lần.
        drop(unsafe { Box::from_raw(ptr) });
    }
}

fn on_set_cursor(hwnd: HWND) {
    let mut pt = POINT::default();
    // SAFETY: tham số ra là biến cục bộ; hwnd là cửa sổ của luồng này.
    unsafe {
        let _ = GetCursorPos(&mut pt);
        let _ = ScreenToClient(hwnd, &mut pt);
    }
    let over = with_pin(hwnd, |p| {
        hit(Point { x: pt.x, y: pt.y }, p.close_rect()) == Hit::Close
    })
    .unwrap_or(false);
    let id = if over { IDC_ARROW } else { IDC_SIZEALL };
    // SAFETY: con trỏ chuột dùng chung của hệ thống, không phải hủy.
    unsafe {
        if let Ok(c) = LoadCursorW(None, id) {
            SetCursor(Some(c));
        }
    }
}

fn on_lbutton_down(hwnd: HWND, lp: LPARAM) {
    let pt = lparam_pt(lp);
    let cur = cursor_pos();
    let start = with_pin(hwnd, |p| {
        p.close_pressed = hit(pt, p.close_rect()) == Hit::Close;
        if !p.close_pressed {
            p.drag = Some((cur, p.corner()));
        }
        !p.close_pressed
    })
    .unwrap_or(false);
    if start {
        // SAFETY: bắt chuột cho chính cửa sổ này đến khi nhả.
        unsafe {
            SetCapture(hwnd);
        }
    }
}

fn on_lbutton_up(hwnd: HWND, lp: LPARAM) {
    let pt = lparam_pt(lp);
    let close = with_pin(hwnd, |p| {
        std::mem::replace(&mut p.close_pressed, false) && hit(pt, p.close_rect()) == Hit::Close
    })
    .unwrap_or(false);
    end_drag(hwnd, true);
    if close {
        destroy(hwnd);
    }
}

fn on_wheel(hwnd: HWND, wp: WPARAM, lp: LPARAM) {
    let delta = hi_i16(wp.0) as i32;
    let ctrl = wp.0 & MK_CONTROL != 0;
    // Tọa độ của WM_MOUSEWHEEL là tọa độ màn hình.
    let at = lparam_pt(lp);
    let steps = with_pin(hwnd, |p| {
        let (s, rest) = wheel_steps(p.wheel, delta);
        p.wheel = rest;
        s
    })
    .unwrap_or(0);
    if steps == 0 {
        return;
    }
    if ctrl {
        opacity_steps(hwnd, steps);
    } else {
        zoom_steps(hwnd, steps, at);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // SAFETY: chuyển nguyên tham số cho xử lý mặc định.
    let def = || unsafe { DefWindowProcW(hwnd, msg, wp, lp) };
    if msg == WM_NCDESTROY {
        on_nc_destroy(hwnd);
        return def();
    }
    if pin_ptr(hwnd).is_null() {
        return def();
    }
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_ACTIVATE => {
            with_pin(hwnd, |p| p.active = lo_word(wp.0) != WA_INACTIVE);
            invalidate(hwnd);
            def()
        }
        WM_NCACTIVATE => {
            with_pin(hwnd, |p| p.active = wp.0 != 0);
            invalidate(hwnd);
            def()
        }
        WM_GETMINMAXINFO => {
            // Cửa sổ có thể lớn hơn màn hình (phóng tới MAX_SIDE) và nhỏ tới vài pixel.
            // SAFETY: lp trỏ tới MINMAXINFO hợp lệ suốt lời gọi.
            if let Some(mm) = unsafe { (lp.0 as *mut MINMAXINFO).as_mut() } {
                let max = core_pin::MAX_SIDE + 64;
                mm.ptMinTrackSize = POINT { x: 1, y: 1 };
                mm.ptMaxTrackSize = POINT { x: max, y: max };
            }
            LRESULT(0)
        }
        WM_WINDOWPOSCHANGED => {
            // Cửa sổ bị dời từ bên ngoài: đồng bộ khung ảnh theo vị trí thật.
            // SAFETY: lp trỏ tới WINDOWPOS hợp lệ suốt lời gọi.
            if let Some(wpos) = unsafe { (lp.0 as *const WINDOWPOS).as_ref() } {
                if !wpos.flags.contains(SWP_NOMOVE) {
                    let (x, y) = (wpos.x as i64, wpos.y as i64);
                    with_pin(hwnd, |p| {
                        if !p.self_move {
                            let b = p.border() as i64;
                            p.img.x = clamp_i32(x + b);
                            p.img.y = clamp_i32(y + b);
                        }
                    });
                }
            }
            def()
        }
        WM_SETCURSOR if lo_word(lp.0 as usize) == HTCLIENT => {
            on_set_cursor(hwnd);
            LRESULT(1)
        }
        WM_MOUSEMOVE => {
            on_mouse_move(hwnd, lp);
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            let close = with_pin(hwnd, |p| {
                p.hover = false;
                p.over_close = false;
                p.tracking = false;
                p.close_pressed = false;
                p.close_rect()
            })
            .flatten();
            invalidate_part(hwnd, close);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            on_lbutton_down(hwnd, lp);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            on_lbutton_up(hwnd, lp);
            LRESULT(0)
        }
        WM_CAPTURECHANGED => {
            end_drag(hwnd, false);
            LRESULT(0)
        }
        WM_LBUTTONDBLCLK => {
            let pt = lparam_pt(lp);
            let on_close =
                with_pin(hwnd, |p| hit(pt, p.close_rect()) == Hit::Close).unwrap_or(true);
            if !on_close {
                zoom_to(hwnd, 100, cursor_pos());
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            on_wheel(hwnd, wp, lp);
            LRESULT(0)
        }
        // WM_RBUTTONUP, Shift+F10 và phím Menu: DefWindowProc sinh WM_CONTEXTMENU.
        WM_CONTEXTMENU => {
            context_menu(hwnd, lp);
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if on_key(hwnd, wp.0 as u16) {
                LRESULT(0)
            } else {
                def()
            }
        }
        WM_TIMER if wp.0 == BADGE_TIMER => {
            // SAFETY: hẹn giờ của chính cửa sổ này.
            unsafe {
                let _ = KillTimer(Some(hwnd), BADGE_TIMER);
            }
            let gone = with_pin(hwnd, |p| {
                p.badge = None;
                p.badge_box.take()
            })
            .flatten();
            invalidate_part(hwnd, gone);
            LRESULT(0)
        }
        WM_DPICHANGED => {
            // Không dùng khung gợi ý trong lp (sẽ phóng ảnh theo DPI): giữ cỡ điểm ảnh.
            on_dpi_changed(hwnd, norm_dpi(lo_word(wp.0)));
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            keep(hwnd);
            def()
        }
        _ => def(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Menu ngữ cảnh: Sao chép, Lưu ảnh, rồi Nhận dạng chữ; mỗi mục một ID riêng, gửi đúng hành động.
    #[test]
    fn menu_actions_copy_save_then_ocr() {
        assert_eq!(
            MENU_ACTIONS,
            [
                (ID_COPY, Msg::PinMenuCopy, Action::Copy),
                (ID_SAVE, Msg::PinMenuSave, Action::Save),
                (ID_OCR, Msg::PinMenuOcr, Action::Ocr),
            ]
        );
        for id in [ID_COPY, ID_SAVE, ID_OCR, ID_CLOSE, ID_CLOSE_ALL] {
            assert!(id < ID_ZOOM_BASE);
        }
        assert_eq!(menu_action(ID_OCR), Some(Action::Ocr));
        assert_eq!(menu_action(ID_SAVE), Some(Action::Save));
        assert_eq!(menu_action(ID_CLOSE), None);
    }

    fn r(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }
    fn p(x: i32, y: i32) -> Point {
        Point { x, y }
    }

    #[test]
    fn border_and_pen_follow_dpi() {
        assert_eq!(border_px(96), 1);
        assert_eq!(border_px(120), 1);
        assert_eq!(border_px(144), 2);
        assert_eq!(border_px(192), 2);
        assert_eq!(border_px(0), 1);
        assert_eq!(x_pen_px(96), 2);
        assert_eq!(x_pen_px(120), 2);
        assert_eq!(x_pen_px(144), 2);
        assert_eq!(x_pen_px(192), 3);
    }

    #[test]
    fn window_rect_inflates_by_border() {
        assert_eq!(window_rect(r(10, 20, 100, 50), 1), r(9, 19, 102, 52));
        assert_eq!(window_rect(r(10, 20, 100, 50), 2), r(8, 18, 104, 54));
        assert_eq!(
            window_rect(r(-1920, -5, 640, 480), 1),
            r(-1921, -6, 642, 482)
        );
        assert_eq!(image_rect_of(r(8, 18, 104, 54), 2), r(10, 20, 100, 50));
        // Không tràn: kẹp vào i32.
        let w = window_rect(r(i32::MIN, 0, i32::MAX, 5), 2);
        assert_eq!((w.x, w.w), (i32::MIN, i32::MAX));
    }

    #[test]
    fn close_button_rect_per_dpi() {
        let c = r(0, 0, 300, 200);
        assert_eq!(close_button_rect(c, 96), r(276, 4, 20, 20));
        assert_eq!(close_button_rect(c, 120), r(270, 5, 25, 25));
        assert_eq!(close_button_rect(c, 144), r(264, 6, 30, 30));
        assert_eq!(close_button_rect(c, 192), r(252, 8, 40, 40));
    }

    #[test]
    fn close_button_visible_needs_48_dip() {
        assert!(close_button_visible(48, 48, 96));
        assert!(!close_button_visible(47, 48, 96));
        assert!(!close_button_visible(48, 47, 96));
        assert!(close_button_visible(60, 60, 120));
        assert!(!close_button_visible(59, 60, 120));
        assert!(close_button_visible(72, 72, 144));
        assert!(!close_button_visible(72, 71, 144));
        assert!(close_button_visible(96, 96, 192));
        assert!(!close_button_visible(95, 96, 192));
    }

    #[test]
    fn badge_rect_per_dpi_and_hidden_when_small() {
        let a = r(0, 0, 400, 300);
        assert_eq!(badge_rect(a, (40, 16), 96), Some(r(172, 268, 56, 24)));
        assert_eq!(badge_rect(a, (40, 16), 120), Some(r(170, 264, 60, 26)));
        assert_eq!(badge_rect(a, (40, 16), 144), Some(r(168, 260, 64, 28)));
        assert_eq!(badge_rect(a, (40, 16), 192), Some(r(164, 252, 72, 32)));
        // Theo gốc của khung ảnh.
        assert_eq!(
            badge_rect(r(1, 1, 400, 300), (40, 16), 96),
            Some(r(173, 269, 56, 24))
        );
        // Ảnh nhỏ hơn nhãn + 8 DIP: ẩn.
        assert_eq!(badge_rect(r(0, 0, 63, 300), (40, 16), 96), None);
        assert!(badge_rect(r(0, 0, 64, 300), (40, 16), 96).is_some());
        assert_eq!(badge_rect(r(0, 0, 400, 31), (40, 16), 96), None);
        assert_eq!(
            badge_rect(r(0, 0, 400, 32), (40, 16), 96),
            Some(r(172, 0, 56, 24))
        );
    }

    #[test]
    fn stretch_mode_by_zoom() {
        assert_eq!(stretch_mode(100), Blit::Copy);
        assert_eq!(stretch_mode(90), Blit::Halftone);
        assert_eq!(stretch_mode(10), Blit::Halftone);
        assert_eq!(stretch_mode(110), Blit::ColorOnColor);
        assert_eq!(stretch_mode(400), Blit::ColorOnColor);
    }

    #[test]
    fn wheel_steps_accumulate() {
        assert_eq!(wheel_steps(0, 120), (1, 0));
        assert_eq!(wheel_steps(0, -120), (-1, 0));
        assert_eq!(wheel_steps(0, 360), (3, 0));
        // Bánh xe mịn: cộng dồn đến đủ một bậc.
        assert_eq!(wheel_steps(0, 40), (0, 40));
        assert_eq!(wheel_steps(80, 40), (1, 0));
        assert_eq!(wheel_steps(100, 240), (2, 100));
        assert_eq!(wheel_steps(-80, -40), (-1, 0));
        // Đổi chiều: bỏ phần dư cũ.
        assert_eq!(wheel_steps(60, -120), (-1, 0));
        assert_eq!(wheel_steps(-60, 30), (0, 30));
        // Không tràn.
        assert!(wheel_steps(i32::MAX, i32::MAX).1.abs() < 120);
    }

    #[test]
    fn hit_close_or_image() {
        let c = Some(r(0, 0, 20, 20));
        assert_eq!(hit(p(10, 10), c), Hit::Close);
        assert_eq!(hit(p(0, 0), c), Hit::Close);
        assert_eq!(hit(p(20, 10), c), Hit::Image);
        assert_eq!(hit(p(10, 20), c), Hit::Image);
        assert_eq!(hit(p(10, 10), None), Hit::Image);
    }

    #[test]
    fn menu_zoom_limits() {
        assert!(menu_zoom_enabled(1000, 1000, 200));
        assert!(menu_zoom_enabled(8192, 100, 200));
        assert!(!menu_zoom_enabled(8193, 100, 200));
        assert!(!menu_zoom_enabled(100, 8193, 200));
        assert!(menu_zoom_enabled(100, 100, 400));
        // Giới hạn diện tích của lõi khi phóng trên 100%.
        assert!(menu_zoom_enabled(3840, 2160, 200));
        assert!(!menu_zoom_enabled(4096, 4096, 200));
        assert!(menu_zoom_enabled(8000, 6000, 100));
    }

    #[test]
    fn initial_at_keeps_100_or_fits_max_side() {
        assert_eq!(
            initial_at(p(-1920, 5), 640, 480),
            Some((100, r(-1920, 5, 640, 480)))
        );
        assert_eq!(
            initial_at(p(0, 0), 20000, 100),
            Some((75, r(0, 0, 15000, 75)))
        );
        // Mức nhỏ nhất (10%) vẫn vượt MAX_SIDE: không ghim được.
        assert_eq!(initial_at(p(0, 0), 200_000, 10), None);
    }

    #[test]
    fn context_point_mouse_or_keyboard() {
        let fb = p(7, 9);
        assert_eq!(context_point(-1, fb), fb);
        let lp = (((50u16 as u32) << 16) | (-100i16 as u16 as u32)) as i32 as isize;
        assert_eq!(context_point(lp, fb), p(-100, 50));
        let lp = (((-300i16 as u16 as u32) << 16) | 200u32) as i32 as isize;
        assert_eq!(context_point(lp, fb), p(200, -300));
    }

    #[test]
    fn title_and_badge_text() {
        assert_eq!(
            title(Lang::En, 640, 480),
            "darkshot — Pinned image 640 × 480"
        );
        assert_eq!(title(Lang::Vi, 1, 2), "darkshot — Ảnh ghim 1 × 2");
        assert_eq!(badge_text(Lang::En, Some(125), 100), "125%");
        assert_eq!(badge_text(Lang::En, None, 60), "Opacity 60%");
        assert_eq!(badge_text(Lang::Vi, None, 60), "Độ mờ 60%");
        for l in Lang::ALL {
            for o in [10u8, 55, 100] {
                assert_eq!(
                    badge_text(l, None, o),
                    darkshot_core::i18n::pin_opacity_badge(l, o)
                );
            }
        }
    }

    #[test]
    fn pending_pin_put_replace_take_once() {
        let mut q: PendingPin<u32> = PendingPin::new();
        assert!(!q.is_waiting());
        assert_eq!(q.take(), None);
        assert_eq!(q.put(1), None);
        assert!(q.is_waiting());
        // Ảnh mới thay ảnh cũ; ảnh cũ trả về để bỏ.
        assert_eq!(q.put(2), Some(1));
        assert_eq!(q.take(), Some(2));
        // Lấy ra đúng một lần.
        assert_eq!(q.take(), None);
        assert!(!q.is_waiting());
        q.put(3);
        q.clear();
        assert_eq!(q.take(), None);
    }

    #[test]
    fn too_large_has_own_variant() {
        assert!(matches!(too_large(), PinError::TooLarge));
    }

    #[test]
    fn menu_open_flag() {
        assert!(!menu_open());
        MENU_OPEN.with(|m| m.set(true));
        assert!(menu_open());
        MENU_OPEN.with(|m| m.set(false));
    }

    #[test]
    fn dip_rounds() {
        assert_eq!(dip(20, 96), 20);
        assert_eq!(dip(20, 120), 25);
        assert_eq!(dip(4, 144), 6);
        assert_eq!(dip(48, 192), 96);
        assert_eq!(dip(1, 0), 1);
    }
}

/// Kiểm thử mở cửa sổ thật. Chạy tay từng cái:
/// `cargo test -p darkshot-windows pin::live -- --ignored --test-threads=1`.
#[cfg(test)]
mod live {
    use super::*;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::Foundation::{POINT, RECT};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::System::Threading::{
        GetCurrentProcess, GetGuiResources, GR_GDIOBJECTS, GR_USEROBJECTS,
    };
    use windows::Win32::UI::HiDpi::{
        SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, EndMenu, GetClientRect, GetWindowRect, IsWindow, KillTimer, PeekMessageW,
        SendMessageW, SetTimer, TranslateMessage, MSG, PM_REMOVE, WM_CONTEXTMENU, WM_MOUSEWHEEL,
    };

    fn send(h: HWND, msg: u32, wp: usize, lp: isize) {
        // SAFETY: gửi thông điệp cho cửa sổ ảnh ghim còn sống của luồng này.
        unsafe {
            SendMessageW(h, msg, Some(WPARAM(wp)), Some(LPARAM(lp)));
        }
    }

    unsafe extern "system" fn end_menu_timer(_: HWND, _: u32, id: usize, _: u32) {
        // SAFETY: hẹn giờ luồng do kiểm thử đặt; đóng menu đang mở của luồng này.
        unsafe {
            let _ = KillTimer(None, id);
            let _ = EndMenu();
        }
    }

    unsafe extern "system" fn close_all_timer(_: HWND, _: u32, id: usize, _: u32) {
        // SAFETY: hẹn giờ luồng do kiểm thử đặt.
        unsafe {
            let _ = KillTimer(None, id);
        }
        close_all();
    }

    /// Mở menu ngữ cảnh bằng bàn phím (lp = -1); hẹn giờ `proc` chạy trong vòng lặp của menu.
    fn context_menu_with(h: HWND, proc: unsafe extern "system" fn(HWND, u32, usize, u32)) {
        // SAFETY: hẹn giờ luồng (không gắn cửa sổ); vòng lặp của menu phân phối WM_TIMER.
        unsafe {
            SetTimer(None, 0, 100, Some(proc));
        }
        send(h, WM_CONTEXTMENU, h.0 as usize, -1);
    }

    fn p(x: i32, y: i32) -> Point {
        Point { x, y }
    }

    fn per_monitor() {
        // SAFETY: như ứng dụng thật (manifest PerMonitorV2), áp cho luồng kiểm thử.
        unsafe {
            SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
    }

    fn pump() {
        let mut msg = MSG::default();
        // SAFETY: bơm thông điệp của luồng kiểm thử.
        unsafe {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    fn image(w: u32, h: u32) -> Image {
        let mut bgra = Vec::with_capacity((w * h * 4) as usize);
        for i in 0..w * h {
            bgra.extend_from_slice(&[(i % 251) as u8, (i % 13) as u8 * 19, 0x80, 0xFF]);
        }
        Image {
            width: w,
            height: h,
            bgra,
        }
    }

    fn counts() -> (u32, u32) {
        // SAFETY: chỉ đọc bộ đếm của tiến trình hiện tại.
        unsafe {
            (
                GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS),
                GetGuiResources(GetCurrentProcess(), GR_USEROBJECTS),
            )
        }
    }

    fn rect_of(h: windows::Win32::Foundation::HWND) -> RECT {
        let mut rc = RECT::default();
        // SAFETY: h là cửa sổ còn sống của luồng này.
        unsafe {
            let _ = GetWindowRect(h, &mut rc);
        }
        rc
    }

    #[test]
    #[ignore]
    fn pin_round_trip_without_gdi_leaks() {
        per_monitor();
        // Vòng đầu là vòng khởi động: lớp cửa sổ và chổi dùng chung (sống suốt tiến trình) được tạo lần đầu.
        let mut base = (0, 0);
        for i in 0..51 {
            if i == 1 {
                // Sau vòng khởi động.
                base = counts();
            }
            let place = if i % 2 == 0 {
                PinPlace::At(p(200 + i, 200))
            } else {
                PinPlace::CenterOnCursorMonitor
            };
            open(image(320, 240), place).unwrap();
            pump();
            // Phóng một bậc: đổi cỡ cửa sổ, hiện nhãn (phông, hẹn giờ) rồi vẽ lại.
            if let Some(h) = handles().first() {
                // SAFETY: gửi thông điệp cho cửa sổ ảnh ghim còn sống của luồng này.
                unsafe {
                    SendMessageW(
                        *h,
                        WM_MOUSEWHEEL,
                        Some(windows::Win32::Foundation::WPARAM(120usize << 16)),
                        Some(windows::Win32::Foundation::LPARAM(0)),
                    );
                }
            }
            pump();
            if let Some(h) = handles().first().copied() {
                // Ctrl + cuộn: độ mờ.
                send(
                    h,
                    WM_MOUSEWHEEL,
                    (-120i16 as u16 as usize) << 16 | 0x0008,
                    0,
                );
                pump();
                // Menu ngữ cảnh mở rồi đóng bằng EndMenu.
                context_menu_with(h, end_menu_timer);
                pump();
            }
            close_all();
            pump();
        }
        let (gdi0, user0) = base;
        let (gdi1, user1) = counts();
        eprintln!("GDI {gdi0} -> {gdi1}, USER {user0} -> {user1}");
        assert!(gdi1 <= gdi0 + 2, "GDI {gdi0} -> {gdi1}");
        assert!(user1 <= user0 + 2, "USER {user0} -> {user1}");
        assert_eq!(count(), 0);
    }

    #[test]
    #[ignore]
    fn pin_close_all_destroys_every_window() {
        per_monitor();
        for i in 0..5 {
            open(image(100, 80), PinPlace::At(p(100 + i * 40, 100 + i * 40))).unwrap();
        }
        pump();
        let hs = handles();
        assert_eq!(hs.len(), 5);
        assert_eq!(count(), 5);
        close_all();
        assert_eq!(count(), 0);
        for h in hs {
            // SAFETY: IsWindow chỉ hỏi tính hợp lệ của handle.
            assert!(!unsafe { IsWindow(Some(h)) }.as_bool());
        }
        // Gọi lại vô hại.
        close_all();
        assert_eq!(count(), 0);
    }

    #[test]
    #[ignore]
    fn pin_limit() {
        per_monitor();
        for i in 0..core_pin::MAX_PINS as i32 {
            open(image(40, 30), PinPlace::At(p(50 + i * 10, 50 + i * 10))).unwrap();
        }
        assert_eq!(count(), core_pin::MAX_PINS);
        assert!(matches!(
            open(image(40, 30), PinPlace::At(p(10, 10))),
            Err(PinError::Limit)
        ));
        assert_eq!(count(), core_pin::MAX_PINS);
        close_all();
        assert_eq!(count(), 0);
    }

    #[test]
    #[ignore]
    fn pin_close_all_during_context_menu() {
        per_monitor();
        for i in 0..3 {
            open(image(120, 90), PinPlace::At(p(150 + i * 30, 150))).unwrap();
        }
        pump();
        let hs = handles();
        // close_all chạy trong vòng lặp của menu đang mở trên ảnh ghim đầu.
        context_menu_with(hs[0], close_all_timer);
        pump();
        assert_eq!(count(), 0);
        for h in hs {
            // SAFETY: IsWindow chỉ hỏi tính hợp lệ của handle.
            assert!(!unsafe { IsWindow(Some(h)) }.as_bool());
        }
    }

    #[test]
    #[ignore]
    fn pin_rejects_invalid_image() {
        per_monitor();
        let bad = Image {
            width: 0,
            height: 10,
            bgra: Vec::new(),
        };
        assert!(matches!(
            open(bad, PinPlace::At(p(10, 10))),
            Err(PinError::Create(_))
        ));
        let bad = Image {
            width: 10,
            height: 10,
            bgra: vec![0; 12],
        };
        assert!(matches!(
            open(bad, PinPlace::CenterOnCursorMonitor),
            Err(PinError::Create(_))
        ));
        // Ở 10% vẫn vượt MAX_SIDE.
        for place in [PinPlace::At(p(10, 10)), PinPlace::CenterOnCursorMonitor] {
            assert!(matches!(
                open(image(200_000, 10), place),
                Err(PinError::TooLarge)
            ));
        }
        assert_eq!(count(), 0);
    }

    #[test]
    #[ignore]
    fn pin_zoom_keeps_anchor() {
        per_monitor();
        let (iw, ih) = (200u32, 200u32);
        open(image(iw, ih), PinPlace::At(p(300, 300))).unwrap();
        pump();
        let h = handles()[0];
        let (ax, ay) = (350i32, 380i32);
        // Điểm ảnh dưới chuột trước khi phóng (100%).
        let before = rect_of(h);
        let b0 = ((before.right - before.left) - iw as i32) / 2;
        let (u0, v0) = (ax - (before.left + b0), ay - (before.top + b0));
        let wp = (120usize) << 16;
        let lp = (((ay as u32 & 0xFFFF) << 16) | (ax as u32 & 0xFFFF)) as isize;
        // SAFETY: gửi thông điệp cho cửa sổ ảnh ghim còn sống của luồng này.
        unsafe {
            SendMessageW(
                h,
                WM_MOUSEWHEEL,
                Some(windows::Win32::Foundation::WPARAM(wp)),
                Some(windows::Win32::Foundation::LPARAM(lp)),
            );
        }
        pump();
        let after = rect_of(h);
        let (dw, _) = core_pin::display_size(iw, ih, 110);
        let b1 = ((after.right - after.left) - dw) / 2;
        assert_eq!(after.right - after.left, dw + 2 * b1);
        let (ix, iy) = (after.left + b1, after.top + b1);
        // Điểm ảnh dưới chuột sau khi phóng (110%).
        let u1 = ((ax - ix) as f64 * 100.0 / 110.0).round() as i32;
        let v1 = ((ay - iy) as f64 * 100.0 / 110.0).round() as i32;
        assert!((u1 - u0).abs() <= 1, "x {u0} -> {u1}");
        assert!((v1 - v0).abs() <= 1, "y {v0} -> {v1}");
        close_all();
    }

    #[test]
    #[ignore]
    fn pin_at_places_image_exactly() {
        per_monitor();
        let at = p(240, 160);
        let (iw, ih) = (123u32, 77u32);
        open(image(iw, ih), PinPlace::At(at)).unwrap();
        pump();
        let h = handles()[0];
        let mut rc = RECT::default();
        // SAFETY: h là cửa sổ còn sống của luồng này; rc là biến cục bộ.
        unsafe {
            let _ = GetClientRect(h, &mut rc);
        }
        let b = ((rc.right - rc.left) - iw as i32) / 2;
        assert!(b >= 1);
        assert_eq!(rc.bottom - rc.top, ih as i32 + 2 * b);
        let mut corner = POINT { x: b, y: b };
        // SAFETY: như trên.
        unsafe {
            let _ = ClientToScreen(h, &mut corner);
        }
        assert_eq!((corner.x, corner.y), (at.x, at.y));
        close_all();
    }
}
