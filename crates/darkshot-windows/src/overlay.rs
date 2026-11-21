//! Lớp phủ toàn màn hình để chọn vùng và đánh dấu.
//!
//! Cửa sổ `WS_POPUP` phủ đúng màn hình ảo, nên tọa độ client trùng tọa độ ảnh.
//! Mọi trạng thái kéo thả nằm trong `geometry::Selection`; hình đánh dấu trong
//! `annotate::Annotations`. Ở đây chỉ vẽ và nhận sự kiện.

use crate::annotate_draw::{caret_rect, draw_shapes, shape_bounds};
use crate::capture::Screenshot;
use crate::gdiplus::{GdiPlus, Gp};
use crate::lang;
use darkshot_core::actions::Action;
use darkshot_core::annotate::{
    dip_to_px, press_mode, sanitize_paste, Annotations, Applied, Change, PressMode, Pt, Shape,
    SizeLevel, TextEdit, Tool, DEFAULT_COLOR, HIT_TOL_DIP, MIN_EXTENT_DIP, PALETTE,
};
use darkshot_core::color::{
    lum_at, lum_y, picker_layout, spectrum_hs, spectrum_point, CustomColors, Hls, PickerLayout,
    CUSTOM_SLOTS,
};
use darkshot_core::color_picker::{
    at, field_value, lum_image, picker_hit, size_dot, size_for_key, spectrum_image, FieldEdit,
    LumKey, PickColor, PickerDrag, PickerHit, FIELD_COUNT,
};
use darkshot_core::edit::{
    frame, grip_at, handles, restyle, topmost_hit, transform, Gesture, Grip,
};
use darkshot_core::geometry::{
    monitor_containing, Handle, Hit, Point, Rect, Selection, SideLayout,
};
use darkshot_core::i18n::{self, Msg};
use darkshot_core::image::Image;
use darkshot_core::overlay_layout::{
    bar_item_at_with, bar_item_rect_with, bar_layout_with, bars_for_with, blit_in_bounds,
    clamp_into, clamp_x, inflate, intersect, is_shape, palette_rect_for, scaled, selection_dirty,
    shape_cell_at, shape_cell_rect, shape_tip, shapes_rect_for, should_release, side_index,
    side_item_at, side_item_rect, side_layout_for, side_on_right, size_cell_at, size_cell_rect,
    size_rect_for, size_tip, text_preview_bounds, to_pt, union, BarItem, SideItem, CROSS_ARM,
    CROSS_GAP, CROSS_THICK, GAP, HANDLE_HIT, HANDLE_SIZE, ICON, LUM_ARROW, MARK, SHAPE_TOOLS,
    SIDE_ITEMS, SWATCH, TEXT_DASH, TEXT_DASH_GAP, TEXT_EMPTY_W, TEXT_FRAME_PAD,
};
use darkshot_core::overlay_stroke::{begin_stroke, end_stroke, extend_stroke, Stroke};
use darkshot_core::snap;
use darkshot_core::theme::{
    Rgb, ACCENT, ACCENT_BUTTON, ACCENT_BUTTON_PRESSED, BUTTON as BUTTON_FILL, BUTTON_BORDER,
    BUTTON_PRESSED, INPUT, INPUT_BORDER, OVERLAY_BORDER, OVERLAY_HOVER, OVERLAY_ICON,
    OVERLAY_LABEL, OVERLAY_TIP, SURFACE, TEXT, TEXT_DISABLED, TEXT_MUTED, WHITE,
};
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateDIBSection,
    CreateFontIndirectW, CreatePen, CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint,
    FillRect, GetStockObject, GetTextExtentPoint32W, IntersectClipRect, InvalidateRect,
    MonitorFromPoint, Rectangle, RoundRect, SelectClipRgn, SelectObject, SetBkMode, SetTextColor,
    StretchDIBits, UpdateWindow, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    DRAW_TEXT_FORMAT, DT_CENTER, DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_VCENTER,
    FW_BOLD, HBITMAP, HDC, HFONT, HGDIOBJ, LOGFONTW, MONITOR_DEFAULTTONEAREST, NULL_BRUSH, OPAQUE,
    PAINTSTRUCT, PS_SOLID, SRCCOPY, TRANSPARENT,
};
use windows::Win32::Graphics::GdiPlus::{
    DashCapFlat, FillModeAlternate, GdipAddPathArc, GdipAddPathLine, GdipClosePathFigure,
    GdipCreatePath, GdipCreatePen1, GdipCreateSolidFill, GdipDeleteBrush, GdipDeletePath,
    GdipDeletePen, GdipDrawPath, GdipFillEllipse, GdipFillPolygon, GdipSetPenLineCap197819,
    GdipSetPenLineJoin, GdipStartPathFigure, GpBrush, GpPath, GpSolidFill, LineCapRound,
    LineJoinRound, PointF, UnitPixel,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, SystemParametersInfoForDpi, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::Ime::{
    ImmGetCompositionStringW, ImmGetContext, ImmNotifyIME, ImmReleaseContext,
    ImmSetCompositionFontW, ImmSetCompositionWindow, CFS_DEFAULT, CFS_POINT, COMPOSITIONFORM,
    CPS_CANCEL, GCS_COMPSTR, NI_COMPOSITIONSTR,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetCapture, GetKeyState, RegisterHotKey, ReleaseCapture, SetCapture, SetFocus,
    UnregisterHotKey, MOD_NOREPEAT, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END,
    VK_ESCAPE, VK_HOME, VK_LEFT, VK_MENU, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetCaretBlinkTime, GetClientRect, GetCursorPos, GetForegroundWindow, GetMessageW,
    GetWindowRect, GetWindowThreadProcessId, KillTimer, LoadCursorW, PostQuitMessage,
    RegisterClassExW, SetCursor, SetForegroundWindow, SetTimer, SetWindowPos, ShowWindow,
    TranslateMessage, HTCLIENT, IDC_ARROW, IDC_CROSS, IDC_IBEAM, IDC_SIZEALL, IDC_SIZENESW,
    IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE, MSG, NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS,
    SWP_NOACTIVATE, SWP_NOZORDER, SW_SHOW, WM_ACTIVATE, WM_CAPTURECHANGED, WM_CHAR, WM_CLOSE,
    WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_HOTKEY, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_SETCURSOR, WM_TIMER, WNDCLASSEXW, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};

const CLASS: PCWSTR = w!("darkshot-overlay");

/// ID phím tắt Esc tạm (khác ID phím chụp của cửa sổ chính).
const ESC_HOTKEY_ID: i32 = 0x0E5C;
const FOCUS_TIMER_ID: usize = 1;
const FOCUS_RETRY_MS: u32 = 100;
const MAX_FOCUS_RETRIES: u32 = 5;
/// Bộ hẹn giờ nháy con trỏ của ô chữ.
const CARET_TIMER_ID: usize = 2;

// Màu lấy từ `darkshot_core::theme`; biểu tượng GDI+ dùng ARGB (0xFF + RGB).
const ICON_ARGB: u32 = argb(OVERLAY_ICON);
const ICON_HOVER_ARGB: u32 = argb(ACCENT);
const ICON_DISABLED_ARGB: u32 = argb(TEXT_DISABLED);

const fn argb(c: Rgb) -> u32 {
    0xFF00_0000 | ((c.0 as u32) << 16) | ((c.1 as u32) << 8) | c.2 as u32
}

fn cr(c: Rgb) -> COLORREF {
    crate::theme::colorref(c)
}

thread_local! {
    /// Màu nét nhớ qua các lần mở lớp phủ trong tiến trình.
    static LAST_COLOR: Cell<Rgb> = const { Cell::new(DEFAULT_COLOR) };
    /// Mức cỡ nhớ qua các lần mở lớp phủ trong tiến trình.
    static LAST_SIZE: Cell<SizeLevel> = const { Cell::new(SizeLevel::Medium) };
    /// Hình nhớ của nút Hình (một trong `SHAPE_TOOLS`) qua các lần mở lớp phủ trong tiến trình.
    static LAST_SHAPE: Cell<Tool> = const { Cell::new(Tool::Arrow) };
}

/// Kết quả chọn vùng: hành động, vùng (tọa độ ảnh chụp) và các hình đã vẽ (tọa độ ảnh chụp).
#[derive(Clone, Debug, PartialEq)]
pub struct Picked {
    pub action: Action,
    pub rect: Rect,
    pub shapes: Vec<Shape>,
}

/// Biểu tượng vẽ bằng `Gp::side_icon`: công cụ, Hoàn tác, Làm lại.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Glyph {
    Tool(Tool),
    Undo,
    Redo,
}

/// Phím bổ trợ giả cho kiểm thử gửi thông điệp (bit 0: Ctrl, bit 1: Shift).
#[cfg(test)]
static TEST_MODS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Phím `vk` đang được giữ (theo trạng thái bàn phím của luồng này).
fn key_held(vk: VIRTUAL_KEY) -> bool {
    #[cfg(test)]
    {
        let m = TEST_MODS.load(std::sync::atomic::Ordering::SeqCst);
        if (vk == VK_CONTROL && m & 1 != 0) || (vk == VK_SHIFT && m & 2 != 0) {
            return true;
        }
    }
    // SAFETY: đọc trạng thái phím của luồng này.
    unsafe { GetKeyState(i32::from(vk.0)) < 0 }
}

/// `WM_KEYDOWN` là lần lặp của phím đang giữ (bit 30 của `lParam`: phím đã xuống trước đó).
fn key_repeat(lp: LPARAM) -> bool {
    lp.0 & (1 << 30) != 0
}

/// Áp một thay đổi của sổ hình (`a`, đã áp vào `shapes`) lên danh sách hộp bao `bounds` (cùng độ
/// dài, thứ tự với `shapes`): Inserted chèn hộp mới tại vị trí, Removed bỏ hộp tại vị trí, Replaced
/// tính lại hộp tại vị trí; các vòng số đổi `n` được tính lại. Trả các vùng cần làm mới (hộp cũ và
/// mới). Lệch độ dài (không nên xảy ra) thì dựng lại cả danh sách và làm mới mọi hộp.
fn apply_bounds(bounds: &mut Vec<Rect>, shapes: &[Shape], a: &Applied) -> Vec<Rect> {
    let mut dirty = Vec::new();
    match &a.change {
        Change::Inserted { index } => {
            if let (Some(s), true) = (shapes.get(*index), *index <= bounds.len()) {
                let r = shape_bounds(s);
                bounds.insert(*index, r);
                dirty.push(r);
            }
        }
        Change::Removed { index, shape } => {
            if *index < bounds.len() {
                dirty.push(bounds.remove(*index));
            } else {
                dirty.push(shape_bounds(shape));
            }
        }
        Change::Replaced { index, old } => {
            if let Some(s) = shapes.get(*index) {
                let r = shape_bounds(s);
                match bounds.get_mut(*index) {
                    Some(slot) => dirty.push(std::mem::replace(slot, r)),
                    None => dirty.push(shape_bounds(old)),
                }
                dirty.push(r);
            }
        }
    }
    for i in &a.renumbered {
        if let (Some(s), Some(slot)) = (shapes.get(*i), bounds.get_mut(*i)) {
            let r = shape_bounds(s);
            dirty.push(std::mem::replace(slot, r));
            dirty.push(r);
        }
    }
    if bounds.len() != shapes.len() {
        *bounds = shapes.iter().map(shape_bounds).collect();
        dirty.extend(bounds.iter().copied());
    }
    dirty
}

const NO_RECT: Rect = Rect {
    x: 0,
    y: 0,
    w: 0,
    h: 0,
};

/// Ô chữ đang gõ cùng hình học đã đo (đo lại mỗi lần ô đổi, không đo khi vẽ hay khi nháy).
struct TextBox {
    edit: TextEdit,
    /// Điểm bấm tạo ô (tọa độ ảnh chụp).
    at: Point,
    /// DPI của màn hình chứa `at`.
    dpi: u32,
    /// Cỡ chữ (px).
    size: f32,
    /// Con trỏ nháy (tọa độ ảnh chụp).
    caret: Rect,
    /// Khung nét đứt (tọa độ ảnh chụp).
    frame: Rect,
}

impl TextBox {
    fn new(at: Point, dpi: u32, level: SizeLevel, color: Rgb) -> TextBox {
        let size = dip_to_px(level.text_dip(), dpi);
        let mut b = TextBox {
            edit: TextEdit::new(Self::origin(at), size, color),
            at,
            dpi,
            size,
            caret: NO_RECT,
            frame: NO_RECT,
        };
        b.measure();
        b
    }

    /// Đo lại con trỏ và khung. Khung gồm phần mực của chữ và con trỏ (phần mực không gồm khoảng
    /// trắng cuối dòng), nới `TEXT_FRAME_PAD`; ô chỉ có khoảng trắng thì tính từ điểm đặt ô, cao
    /// một dòng, rộng tối thiểu `TEXT_EMPTY_W` (không gọi `shape_bounds` cho ô rỗng).
    fn measure(&mut self) {
        self.caret = caret_rect(&self.edit);
        let base = match self.edit.to_shape() {
            Some(shape) => union(Some(shape_bounds(&shape)), Some(self.caret)),
            None => union(
                Some(Rect {
                    x: self.at.x,
                    y: self.at.y,
                    w: scaled(TEXT_EMPTY_W, self.dpi),
                    h: self.caret.h,
                }),
                Some(self.caret),
            ),
        }
        .unwrap_or(self.caret);
        self.frame = inflate(base, scaled(TEXT_FRAME_PAD, self.dpi));
    }

    fn origin(at: Point) -> Pt {
        Pt {
            x: at.x as f32,
            y: at.y as f32,
        }
    }

    /// Đổi cỡ chữ theo mức `level` (giữ chữ, màu `color`, vị trí con trỏ) rồi đo lại.
    fn resize(&mut self, level: SizeLevel, color: Rgb) {
        self.size = dip_to_px(level.text_dip(), self.dpi);
        let text = self.edit.text();
        let caret = self.edit.caret();
        let mut edit = TextEdit::new(Self::origin(self.at), self.size, color);
        edit.insert(&text);
        // Sau khi chèn, con trỏ ở cuối chữ: lùi về chỗ cũ.
        for _ in caret..edit.caret() {
            edit.left();
        }
        self.edit = edit;
        self.measure();
    }

    /// Vùng màn hình ô chiếm (chữ, khung, con trỏ).
    fn dirty(&self) -> Rect {
        union(Some(inflate(self.frame, 1)), Some(self.caret)).unwrap_or(self.frame)
    }
}

/// Bộ gõ đang có chuỗi soạn dở trên cửa sổ `hwnd`.
fn ime_composing(hwnd: HWND) -> bool {
    // SAFETY: ngữ cảnh IME của cửa sổ luồng này, trả lại ngay bằng ImmReleaseContext.
    unsafe {
        let himc = ImmGetContext(hwnd);
        if himc.is_invalid() {
            return false;
        }
        let n = ImmGetCompositionStringW(himc, GCS_COMPSTR, None, 0);
        let _ = ImmReleaseContext(hwnd, himc);
        n > 0
    }
}

/// Kết thúc chuỗi soạn dở của bộ gõ trên `hwnd` khi ô chữ đóng. `keep`: trả chuỗi đang soạn để
/// chèn thẳng vào ô cũ (hoàn tất); không thì bỏ. Chuỗi soạn luôn được xóa bằng `CPS_CANCEL`, nên bộ
/// gõ không gửi thêm WM_CHAR sau đó (sẽ rơi vào ô mới hoặc mất). Đặt lại cửa sổ soạn về
/// `CFS_DEFAULT` để khi không có ô chữ bộ gõ hành xử như trước.
fn ime_end_composition(hwnd: HWND, keep: bool) -> Option<String> {
    let form = COMPOSITIONFORM {
        dwStyle: CFS_DEFAULT,
        ..Default::default()
    };
    // SAFETY: ngữ cảnh IME của cửa sổ luồng này, trả lại ở cuối khối trên mọi đường đi; bộ đệm
    // đọc chuỗi soạn đủ `bytes` byte theo kích thước bộ gõ vừa báo.
    unsafe {
        let himc = ImmGetContext(hwnd);
        if himc.is_invalid() {
            return None;
        }
        let bytes = ImmGetCompositionStringW(himc, GCS_COMPSTR, None, 0);
        let mut pending = None;
        if bytes > 0 {
            if keep {
                let mut buf = vec![0u16; (bytes as usize).div_ceil(2)];
                let got = ImmGetCompositionStringW(
                    himc,
                    GCS_COMPSTR,
                    Some(buf.as_mut_ptr() as *mut c_void),
                    (buf.len() * 2) as u32,
                );
                if got > 0 {
                    let n = (got as usize / 2).min(buf.len());
                    pending = buf.get(..n).map(String::from_utf16_lossy);
                }
            }
            let _ = ImmNotifyIME(himc, NI_COMPOSITIONSTR, CPS_CANCEL, 0);
        }
        let _ = ImmSetCompositionWindow(himc, &form);
        let _ = ImmReleaseContext(hwnd, himc);
        pending.filter(|s| !s.is_empty())
    }
}

/// Đặt cửa sổ soạn của bộ gõ tại `caret` (tọa độ client) với phông cỡ gần chữ đánh dấu.
fn place_ime(hwnd: HWND, caret: Rect, size_px: f32) {
    let form = COMPOSITIONFORM {
        dwStyle: CFS_POINT,
        ptCurrentPos: POINT {
            x: caret.x,
            y: caret.y,
        },
        ..Default::default()
    };
    let mut lf = LOGFONTW {
        lfHeight: -(size_px.round() as i32).max(1),
        lfWeight: FW_BOLD.0 as i32,
        ..Default::default()
    };
    for (slot, u) in lf.lfFaceName.iter_mut().zip(wide("Segoe UI")) {
        *slot = u;
    }
    // SAFETY: ngữ cảnh IME của cửa sổ luồng này, trả lại ngay; hai cấu trúc sống suốt lời gọi.
    unsafe {
        let himc = ImmGetContext(hwnd);
        if himc.is_invalid() {
            return;
        }
        let _ = ImmSetCompositionFontW(himc, &lf);
        let _ = ImmSetCompositionWindow(himc, &form);
        let _ = ImmReleaseContext(hwnd, himc);
    }
}

fn to_rect(r: Rect) -> RECT {
    RECT {
        left: r.x,
        top: r.y,
        right: r.right(),
        bottom: r.bottom(),
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn point_from_lparam(lp: LPARAM) -> Point {
    Point {
        x: (lp.0 & 0xFFFF) as u16 as i16 as i32,
        y: ((lp.0 >> 16) & 0xFFFF) as u16 as i16 as i32,
    }
}

fn cursor_for_hit(hit: Hit) -> PCWSTR {
    match hit {
        Hit::Outside => IDC_CROSS,
        Hit::Inside => IDC_SIZEALL,
        Hit::Handle(Handle::N | Handle::S) => IDC_SIZENS,
        Hit::Handle(Handle::E | Handle::W) => IDC_SIZEWE,
        Hit::Handle(Handle::NW | Handle::SE) => IDC_SIZENWSE,
        Hit::Handle(Handle::NE | Handle::SW) => IDC_SIZENESW,
    }
}

/// Con trỏ trên tay nắm `g` của hình đang chọn (đầu mút và di chuyển: `IDC_SIZEALL`).
fn cursor_for_grip(g: Grip) -> PCWSTR {
    match g {
        Grip::Box(h) => cursor_for_hit(Hit::Handle(h)),
        Grip::Move | Grip::End(_) => IDC_SIZEALL,
    }
}

/// Hộp bao xem trước của hình `shape` trong cử chỉ `g`.
fn preview_bounds(g: &Gesture, shape: &Shape) -> Rect {
    text_preview_bounds(&g.original, g.original_bounds, shape)
        .unwrap_or_else(|| shape_bounds(shape))
}

fn set_cursor(id: PCWSTR) {
    // SAFETY: con trỏ chuẩn của hệ thống, không cần giải phóng.
    unsafe {
        if let Ok(c) = LoadCursorW(None, id) {
            SetCursor(Some(c));
        }
    }
}

/// Việc phải làm sau khi đã trả mượn trạng thái (các API này có thể gửi thông điệp đồng bộ).
#[derive(Default)]
struct Effects {
    capture: bool,
    release: bool,
}

struct State {
    hwnd: HWND,
    /// Vùng đã yêu cầu vẽ lại (chỉ kiểm thử, vì không có cửa sổ thật).
    #[cfg(test)]
    inval_log: std::cell::RefCell<Vec<Rect>>,
    w: i32,
    h: i32,
    /// Ảnh gốc (`Screenshot::image.bgra`), sống suốt `pick_region`; chỉ đọc.
    bright: *const u8,
    bright_len: usize,
    /// Ảnh chụp gốc (`Screenshot::image`), sống suốt `pick_region`; chỉ đọc (màu che của Làm mờ).
    image: *const Image,
    dark: Vec<u8>,
    /// Bộ đệm vẽ cố định cỡ màn hình ảo; `None` thì mỗi lần vẽ tạo bitmap tạm.
    back: Option<BackBuffer>,
    /// Số lần đã thử lại giành tiêu điểm.
    focus_retries: u32,
    monitors: Vec<Rect>,
    dpis: Vec<u32>,
    sel: Selection,
    /// Lần kéo hiện tại là kéo khung mới (ẩn thanh công cụ).
    new_drag: bool,
    drag_cursor: PCWSTR,
    pressed: Option<BarItem>,
    hover: Option<BarItem>,
    /// Công cụ vẽ đang chọn; `None` thì lớp phủ chạy như khi chưa có công cụ vẽ.
    tool: Option<Tool>,
    /// Các hình đã chốt (tọa độ ảnh chụp).
    marks: Annotations,
    /// Hộp bao (tọa độ ảnh chụp) của từng hình trong `marks`, cùng thứ tự; tính lúc chốt.
    mark_bounds: Vec<Rect>,
    /// Ô chữ đang gõ.
    text: Option<TextBox>,
    /// Con trỏ nháy của ô chữ đang hiện.
    caret_on: bool,
    /// Nửa đầu của cặp UTF-16 (ký tự ngoài BMP) chờ nửa sau qua WM_CHAR.
    high_surrogate: Option<u16>,
    /// Phím tắt Esc toàn cục đang được lớp phủ đăng ký.
    esc_registered: bool,
    /// Màu cho hình vẽ sau đó.
    color: Rgb,
    /// Mức cỡ cho hình vẽ sau đó.
    size: SizeLevel,
    /// Bảng Cỡ đang mở.
    sizes_open: bool,
    /// Ô của bảng Cỡ đang rê chuột, đang được nhấn (tác dụng khi nhả trên cùng ô).
    size_hover: Option<SizeLevel>,
    size_pressed: Option<SizeLevel>,
    /// Hình nhớ của nút Hình (một trong `SHAPE_TOOLS`).
    shape_tool: Tool,
    /// Bảng Hình đang mở.
    shapes_open: bool,
    /// Ô của bảng Hình đang rê chuột, đang được nhấn (tác dụng khi nhả trên cùng ô).
    shape_hover: Option<Tool>,
    shape_pressed: Option<Tool>,
    /// Nét đang kéo (chưa chốt vào `marks`).
    stroke: Option<Stroke>,
    /// Vị trí hình đang chọn trong `marks` (chỉ khi công cụ Chọn bật).
    selected: Option<usize>,
    /// Cử chỉ kéo hình đang chọn (di chuyển, đổi cỡ).
    gesture: Option<Gesture>,
    /// Hình xem trước của cử chỉ (vẽ thay `marks[gesture.index]`), hộp bao của nó và điểm chuột
    /// cuối (đã kẹp vào vùng chọn; để nhấn/nhả Shift cập nhật ngay).
    preview: Option<Shape>,
    preview_bounds: Rect,
    gesture_at: Pt,
    /// Tâm tay nắm đang nắm trừ điểm bấm (0 khi di chuyển): điểm đưa vào `transform` là điểm chuột
    /// cộng độ lệch này, để cú nhích đầu không làm cạnh/góc nhảy tới điểm bấm.
    grip_off: Pt,
    /// Chuột đã dịch khỏi điểm bấm trong cử chỉ hiện tại (chưa dịch thì thả không ghi sổ).
    gesture_moved: bool,
    side_hover: Option<SideItem>,
    side_pressed: Option<SideItem>,
    /// Bảng màu đang mở.
    palette: bool,
    /// Bảng màu ở dạng đủ; nhớ trong lần chụp này.
    expanded: bool,
    /// 16 màu tự chọn; `pick_region` chép ra cho bên gọi khi đóng.
    custom: CustomColors,
    /// Có phiên chụp từng bước: thanh ngang thêm ô Thêm bước, Ctrl+N thêm bước.
    steps: bool,
    /// Màu hiện tại của phần tự chọn (dạng đủ).
    pick: PickColor,
    /// Ô tự chọn đích của "Thêm vào màu tự chọn".
    target: Option<usize>,
    /// Ô số đang sửa.
    field: Option<FieldEdit>,
    /// Phần bảng đang được nhấn (tác dụng khi nhả trên cùng phần).
    picker_pressed: Option<PickerHit>,
    picker_drag: Option<PickerDrag>,
    /// Ảnh ô sắc độ đã dựng, theo cỡ ô (mỗi DPI một cỡ).
    spectra: Vec<((i32, i32), Vec<u8>)>,
    /// Ảnh thanh độ sáng đã dựng: (Hue, Sat, rộng, cao) và ảnh.
    lum_img: Option<(LumKey, Vec<u8>)>,
    /// WM_CHAR cần nuốt (Enter đã được bảng màu dùng ở WM_KEYDOWN).
    eat_char: Option<u16>,
    result: Option<Option<Picked>>,
    fonts: Vec<(u32, HFONT)>,
    /// DC bộ nhớ dùng để đo chữ.
    measure: HDC,
    gdiplus: bool,
    /// Ứng viên gợi ý (cửa sổ rồi màn hình, tọa độ ảnh); rỗng khi ô gợi ý tắt.
    cands: Vec<Rect>,
    /// Ô gợi ý cửa sổ bật (`Screenshot::windows` là `Some`): chọn chữ của viên nhắc.
    snap_windows: bool,
    /// Ứng viên đang xem trước dưới con trỏ (chỉ khi `snap_active`, hoặc trong lúc bấm còn giữ
    /// `snap_press` và chuột chưa rời điểm bấm).
    snap_hover: Option<Rect>,
    /// Điểm bấm của lần bấm hiện tại khi gợi ý đang chạy; bỏ khi chuột rời điểm bấm từ
    /// `MIN_SIZE` (thành kéo vùng). Nhả khi còn giữ thì chọn ứng viên tại điểm bấm.
    snap_press: Option<Point>,
    /// Vị trí chuột cuối cùng (tọa độ ảnh); lúc mở là vị trí con trỏ lúc chụp.
    mouse: Point,
}

impl Drop for State {
    fn drop(&mut self) {
        // SAFETY: font và DC do State tạo, xóa đúng một lần; không còn được chọn vào DC nào.
        unsafe {
            for (_, f) in self.fonts.drain(..) {
                let _ = DeleteObject(f.into());
            }
            if !self.measure.is_invalid() {
                let _ = DeleteDC(self.measure);
            }
        }
    }
}

/// Vị trí các thanh cho vùng chọn hiện tại.
struct Bars {
    sel: Rect,
    /// Màn hình chứa góc dưới phải vùng chọn.
    mon: Rect,
    dpi: u32,
    /// Thanh ngang.
    bar: Rect,
    /// Thanh công cụ vẽ (dọc).
    side: Rect,
    /// Bố cục thanh dọc (tương đối `side`).
    sl: SideLayout,
}

/// DC bộ nhớ có DIB 32 bit cố định được chọn sẵn; giải phóng khi drop.
struct BackBuffer {
    dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
}

impl BackBuffer {
    fn new(w: i32, h: i32) -> Option<BackBuffer> {
        let bmi = dib_info(w, h);
        let mut bits: *mut c_void = std::ptr::null_mut();
        // SAFETY: DC và DIB vừa tạo; mọi đường lỗi giải phóng phần đã tạo.
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return None;
            }
            let Ok(bmp) = CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
            else {
                let _ = DeleteDC(dc);
                return None;
            };
            let old = SelectObject(dc, bmp.into());
            Some(BackBuffer { dc, bmp, old })
        }
    }
}

impl Drop for BackBuffer {
    fn drop(&mut self) {
        // SAFETY: bỏ chọn rồi xóa bitmap và DC do BackBuffer tạo, mỗi thứ đúng một lần.
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.bmp.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

/// BITMAPINFO cho ảnh BGRA 32 bit từ trên xuống.
fn dib_info(w: i32, h: i32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    }
}

thread_local! {
    static OVERLAY: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Chạy `f` với trạng thái lớp phủ; `None` nếu chưa có hoặc đang được mượn (gọi lồng).
fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    OVERLAY.with(|o| {
        let mut b = o.try_borrow_mut().ok()?;
        b.as_mut().map(f)
    })
}

impl State {
    fn monitor_dpi(&self, m: Rect) -> u32 {
        self.monitors
            .iter()
            .position(|x| *x == m)
            .and_then(|i| self.dpis.get(i).copied())
            .unwrap_or(96)
    }

    /// Ảnh chụp gốc của lớp phủ.
    fn shot_image(&self) -> &Image {
        // SAFETY: `image` trỏ tới `Screenshot::image`, sống suốt pick_region (State bị hủy trước
        // khi pick_region trả về) và không bị ai ghi trong lúc này.
        unsafe { &*self.image }
    }

    fn dpi_at(&self, p: Point) -> u32 {
        self.monitor_dpi(monitor_containing(p, &self.monitors))
    }

    fn max_dpi(&self) -> u32 {
        self.dpis.iter().copied().max().unwrap_or(96)
    }

    fn hit_radius(&self, p: Point) -> i32 {
        scaled(HANDLE_HIT, self.dpi_at(p))
    }

    /// Vị trí hai thanh; `None` khi chưa có vùng chọn hoặc đang kéo khung mới.
    fn bars(&self) -> Option<Bars> {
        let sel = self.sel.rect()?;
        if self.sel.is_dragging() && self.new_drag {
            return None;
        }
        let corner = Point {
            x: sel.right() - 1,
            y: sel.bottom() - 1,
        };
        let mon = monitor_containing(corner, &self.monitors);
        let dpi = self.monitor_dpi(mon);
        let (bar, side) = bars_for_with(sel, mon, dpi, self.steps);
        Some(Bars {
            sel,
            mon,
            dpi,
            bar,
            side,
            sl: side_layout_for(mon, dpi),
        })
    }

    fn toolbar(&self) -> Option<(Rect, u32)> {
        self.bars().map(|b| (b.bar, b.dpi))
    }

    fn side_bar(&self) -> Option<(Rect, u32)> {
        self.bars().map(|b| (b.side, b.dpi))
    }

    fn side_item_at(&self, p: Point) -> Option<SideItem> {
        let b = self.bars()?;
        side_item_at(b.side, &b.sl, p)
    }

    /// Bảng Cỡ khi đang mở.
    fn size_rect(&self) -> Option<(Rect, u32)> {
        if !self.sizes_open {
            return None;
        }
        let b = self.bars()?;
        Some((size_rect_for(b.side, b.sel, b.mon, b.dpi), b.dpi))
    }

    /// Ô mức cỡ dưới `p`; `None` khi bảng Cỡ đóng hoặc `p` không trên ô nào.
    fn size_cell_at(&self, p: Point) -> Option<SizeLevel> {
        let (fly, dpi) = self.size_rect()?;
        size_cell_at(fly, dpi, p)
    }

    /// Bảng Hình khi đang mở.
    fn shapes_rect(&self) -> Option<(Rect, u32)> {
        if !self.shapes_open {
            return None;
        }
        let b = self.bars()?;
        Some((shapes_rect_for(b.side, b.sel, b.mon, b.dpi), b.dpi))
    }

    /// Ô công cụ dưới `p`; `None` khi bảng Hình đóng hoặc `p` không trên ô nào.
    fn shape_cell_at(&self, p: Point) -> Option<Tool> {
        let (fly, dpi) = self.shapes_rect()?;
        shape_cell_at(fly, dpi, p)
    }

    /// Mục thanh ngang dưới `p`.
    fn bar_item_at(&self, p: Point) -> Option<BarItem> {
        let (bar, dpi) = self.toolbar()?;
        bar_item_at_with(bar, p, dpi, self.steps)
    }

    /// Bảng màu khi đang mở.
    fn palette_rect(&self) -> Option<(Rect, u32)> {
        if !self.palette {
            return None;
        }
        let b = self.bars()?;
        Some((
            palette_rect_for(b.side, b.sel, b.mon, b.dpi, self.expanded),
            b.dpi,
        ))
    }

    /// Phần bảng màu dưới `p`; `None` khi bảng đóng hoặc `p` ngoài bảng.
    fn picker_hit_at(&self, p: Point) -> Option<PickerHit> {
        let (pal, dpi) = self.palette_rect()?;
        picker_hit(&picker_layout(dpi, self.expanded), dpi, pal, p)
    }

    /// Làm mới cả bảng màu (chỉ bảng).
    fn invalidate_picker(&self) {
        if let Some((r, _)) = self.palette_rect() {
            self.invalidate(&[inflate(r, 1)]);
        }
    }

    /// Dựng (nếu chưa có) ảnh ô sắc độ cho cỡ ô của `l` và ảnh thanh độ sáng cho Hue/Sat hiện tại.
    fn ensure_picker_images(&mut self, l: &PickerLayout) {
        let size = (l.spectrum.w, l.spectrum.h);
        if size.0 > 0 && size.1 > 0 && !self.spectra.iter().any(|(k, _)| *k == size) {
            self.spectra.push((size, spectrum_image(size.0, size.1)));
        }
        let key = (self.pick.hls.h, self.pick.hls.s, l.lum_bar.w, l.lum_bar.h);
        if key.2 > 0 && key.3 > 0 && self.lum_img.as_ref().map(|(k, _)| *k) != Some(key) {
            self.lum_img = Some((key, lum_image(key.0, key.1, key.2, key.3)));
        }
    }

    /// Đang kéo (vùng chọn hoặc nét vẽ).
    fn dragging(&self) -> bool {
        self.sel.is_dragging()
            || self.stroke.is_some()
            || self.picker_drag.is_some()
            || self.gesture.is_some()
    }

    fn font(&mut self, dpi: u32) -> HFONT {
        if let Some((_, f)) = self.fonts.iter().find(|(d, _)| *d == dpi) {
            return *f;
        }
        let mut ncm = NONCLIENTMETRICSW {
            cbSize: std::mem::size_of::<NONCLIENTMETRICSW>() as u32,
            ..Default::default()
        };
        // SAFETY: ncm có cbSize đúng; hàm chỉ ghi vào ncm.
        let ok = unsafe {
            SystemParametersInfoForDpi(
                SPI_GETNONCLIENTMETRICS.0,
                ncm.cbSize,
                Some(&mut ncm as *mut NONCLIENTMETRICSW as *mut c_void),
                0,
                dpi,
            )
            .is_ok()
        };
        let lf = if ok {
            ncm.lfMessageFont
        } else {
            let mut lf = LOGFONTW {
                lfHeight: -scaled(12, dpi),
                ..Default::default()
            };
            for (slot, u) in lf.lfFaceName.iter_mut().zip(wide("Segoe UI")) {
                *slot = u;
            }
            lf
        };
        // SAFETY: lf hợp lệ; font được lưu và xóa trong Drop của State.
        let f = unsafe { CreateFontIndirectW(&lf) };
        if !f.is_invalid() {
            self.fonts.push((dpi, f));
        }
        f
    }

    fn text_size(&mut self, text: &[u16], dpi: u32) -> (i32, i32) {
        let font = self.font(dpi);
        let mut sz = SIZE::default();
        // SAFETY: DC đo chữ còn sống; font được chọn rồi trả lại ngay.
        unsafe {
            let old = SelectObject(self.measure, font.into());
            let _ = GetTextExtentPoint32W(self.measure, text, &mut sz);
            SelectObject(self.measure, old);
        }
        (sz.cx, sz.cy)
    }

    /// Khung đang hiển thị: ứng viên xem trước (chỉ có khi chưa có vùng chọn thật, hoặc khi vừa
    /// bấm mà chuột chưa rời điểm bấm: vùng tạm < MIN_SIZE không được vẽ), nếu không thì vùng chọn.
    fn shown_rect(&self) -> Option<Rect> {
        self.snap_hover.or(self.sel.rect())
    }

    /// Gợi ý cửa sổ đang chạy (thiết kế 3.1). Có hình đánh dấu thì coi như đã có vùng chọn; ô chữ
    /// đang gõ coi như bảng nổi.
    fn snap_active(&self) -> bool {
        snap::snap_active(
            !self.cands.is_empty(),
            self.sel.rect().is_some() || !self.marks.is_empty(),
            self.dragging(),
            self.palette || self.sizes_open || self.shapes_open || self.text.is_some(),
        )
    }

    /// Ứng viên xem trước tại `p` (None khi gợi ý không chạy).
    fn snap_at(&self, p: Point) -> Option<Rect> {
        if self.snap_active() {
            snap::candidate_at(&self.cands, p)
        } else {
            None
        }
    }

    /// Chọn ứng viên `c` làm vùng chọn thật (như kéo xong: có tay nắm, thanh công cụ).
    fn select_candidate(&mut self, c: Rect) {
        self.sel = Selection::with_rect(
            Rect {
                x: 0,
                y: 0,
                w: self.w,
                h: self.h,
            },
            Some(c),
        );
        self.snap_hover = None;
    }

    /// Nhãn kích thước: vị trí, chữ, DPI.
    fn label(&mut self) -> Option<(Rect, Vec<u16>, u32)> {
        let r = self.shown_rect()?;
        let corner = Point { x: r.x, y: r.y };
        let mon = monitor_containing(corner, &self.monitors);
        let dpi = self.monitor_dpi(mon);
        let text = wide(&format!("{} \u{d7} {}", r.w, r.h));
        let (tw, th) = self.text_size(&text, dpi);
        let (lw, lh) = (tw + 2 * scaled(7, dpi), th + 2 * scaled(2, dpi));
        let gap = scaled(GAP, dpi);
        let above = r.y - gap - lh;
        let y = if above >= mon.y { above } else { r.y + gap };
        let x = if above >= mon.y { r.x } else { r.x + gap };
        let rect = clamp_x(Rect { x, y, w: lw, h: lh }, mon);
        Some((rect, text, dpi))
    }

    /// Viên nhắc (khi đang hiện): vị trí, chữ, DPI.
    fn hint(&mut self) -> Option<(Rect, Vec<u16>, u32)> {
        let (has_sel, dragging) = (self.sel.rect().is_some(), self.dragging());
        if has_sel || dragging {
            return None;
        }
        let mon = monitor_containing(self.mouse, &self.monitors);
        let dpi = self.monitor_dpi(mon);
        let msg = if self.snap_windows {
            Msg::OverlayHintSnap
        } else {
            Msg::OverlayHintDrag
        };
        let text = wide(lang::tr(msg));
        let size = self.text_size(&text, dpi);
        let r = snap::hint_rect(mon, size, dpi);
        snap::hint_visible(r, self.mouse, dpi, has_sel, dragging).then_some((r, text, dpi))
    }

    /// Chú thích của nút đang rê chuột (thanh ngang, thanh dọc, bảng Cỡ, bảng Hình): vị trí, chữ,
    /// DPI.
    fn tooltip(&mut self) -> Option<(Rect, Vec<u16>, u32)> {
        if self.size_hover.is_some() {
            return self.size_tooltip();
        }
        if self.shape_hover.is_some() {
            return self.shape_tooltip();
        }
        if self.side_hover.is_some() {
            return self.side_tooltip();
        }
        let item = self.hover?;
        let (bar, dpi) = self.toolbar()?;
        let btn = bar_item_rect_with(bar, dpi, self.steps, item)?;
        let text = wide(lang::tr(item.tip()));
        let (tw, th) = self.text_size(&text, dpi);
        let (w, h) = (tw + 2 * scaled(8, dpi), th + 2 * scaled(4, dpi));
        let btn_right = btn.right();
        let mon = monitor_containing(
            Point {
                x: btn_right - 1,
                y: bar.y,
            },
            &self.monitors,
        );
        let gap = scaled(7, dpi);
        let above = bar.y - gap - h;
        let y = if above >= mon.y {
            above
        } else {
            bar.bottom() + gap
        };
        let rect = clamp_x(
            Rect {
                x: btn_right - w,
                y,
                w,
                h,
            },
            mon,
        );
        Some((rect, text, dpi))
    }

    /// Chú thích của mục thanh dọc: cạnh thanh, phía xa vùng chọn (không đủ chỗ thì phía kia),
    /// giữa theo chiều dọc nút.
    fn side_tooltip(&mut self) -> Option<(Rect, Vec<u16>, u32)> {
        let item = self.side_hover?;
        let b = self.bars()?;
        let dpi = b.dpi;
        let i = SIDE_ITEMS.iter().position(|x| *x == item)?;
        let btn = side_item_rect(b.side, &b.sl, i)?;
        let text = wide(lang::tr(item.tip()));
        let (tw, th) = self.text_size(&text, dpi);
        let (w, h) = (tw + 2 * scaled(8, dpi), th + 2 * scaled(4, dpi));
        let gap = scaled(7, dpi);
        let right_x = b.side.right() + gap;
        let left_x = b.side.x - gap - w;
        let fits_right = right_x + w <= b.mon.right();
        let fits_left = left_x >= b.mon.x;
        let x = if side_on_right(b.side, b.sel) {
            if fits_right || !fits_left {
                right_x
            } else {
                left_x
            }
        } else if fits_left || !fits_right {
            left_x
        } else {
            right_x
        };
        let rect = clamp_into(
            Rect {
                x,
                y: btn.y + btn.h / 2 - h / 2,
                w,
                h,
            },
            b.mon,
        );
        Some((rect, text, dpi))
    }

    /// Chú thích của ô bảng Cỡ đang rê chuột.
    fn size_tooltip(&mut self) -> Option<(Rect, Vec<u16>, u32)> {
        let level = self.size_hover?;
        let (fly, dpi) = self.size_rect()?;
        let i = SizeLevel::ALL.iter().position(|l| *l == level)?;
        self.flyout_tooltip(size_cell_rect(fly, dpi, i), fly, dpi, size_tip(level))
    }

    /// Chú thích của ô bảng Hình đang rê chuột.
    fn shape_tooltip(&mut self) -> Option<(Rect, Vec<u16>, u32)> {
        let tool = self.shape_hover?;
        let (fly, dpi) = self.shapes_rect()?;
        let i = SHAPE_TOOLS.iter().position(|t| *t == tool)?;
        self.flyout_tooltip(shape_cell_rect(fly, dpi, i), fly, dpi, shape_tip(tool))
    }

    /// Chú thích `msg` của ô `cell` trong bảng `fly`: trên bảng (không đủ chỗ thì dưới), giữa
    /// theo ô.
    fn flyout_tooltip(
        &mut self,
        cell: Rect,
        fly: Rect,
        dpi: u32,
        msg: Msg,
    ) -> Option<(Rect, Vec<u16>, u32)> {
        let mon = self.bars()?.mon;
        let text = wide(lang::tr(msg));
        let (tw, th) = self.text_size(&text, dpi);
        let (w, h) = (tw + 2 * scaled(8, dpi), th + 2 * scaled(4, dpi));
        let gap = scaled(7, dpi);
        let above = fly.y - gap - h;
        let y = if above >= mon.y {
            above
        } else {
            fly.bottom() + gap
        };
        let rect = clamp_into(
            Rect {
                x: cell.x + cell.w / 2 - w / 2,
                y,
                w,
                h,
            },
            mon,
        );
        Some((rect, text, dpi))
    }

    /// Hai thanh, bảng màu, bảng Cỡ, bảng Hình và chú thích: các vùng đổi khi chọn công cụ, màu,
    /// cỡ, khi sổ hình đổi (Hoàn tác, Làm lại), rê chuột trên thanh.
    fn bar_chrome(&mut self) -> Vec<Rect> {
        let mut v = Vec::new();
        if let Some((r, _)) = self.toolbar() {
            v.push(inflate(r, 1));
        }
        if let Some((r, _)) = self.side_bar() {
            v.push(inflate(r, 1));
        }
        if let Some((r, _)) = self.palette_rect() {
            v.push(inflate(r, 1));
        }
        if let Some((r, _)) = self.size_rect() {
            v.push(inflate(r, 1));
        }
        if let Some((r, _)) = self.shapes_rect() {
            v.push(inflate(r, 1));
        }
        if let Some((r, _, _)) = self.tooltip() {
            v.push(r);
        }
        v
    }

    /// Mọi vùng có vẽ thêm ngoài nền tối (khung, tay nắm, nhãn, các thanh, bảng màu, chú thích).
    fn chrome(&mut self) -> Vec<Rect> {
        let mut v = Vec::new();
        if let Some(r) = self.shown_rect() {
            v.push(inflate(r, scaled(HANDLE_SIZE, self.max_dpi()) / 2 + 3));
        }
        if let Some((r, _, _)) = self.label() {
            v.push(r);
        }
        if let Some((r, _, _)) = self.hint() {
            v.push(inflate(r, 1));
        }
        v.extend(self.bar_chrome());
        v
    }

    fn invalidate(&self, rects: &[Rect]) {
        #[cfg(test)]
        self.inval_log.borrow_mut().extend_from_slice(rects);
        // Chưa có cửa sổ (trạng thái dựng trong kiểm thử): InvalidateRect(NULL) sẽ vẽ lại mọi cửa sổ.
        if self.hwnd.is_invalid() {
            return;
        }
        for r in rects {
            let rc = to_rect(*r);
            // SAFETY: hwnd là cửa sổ lớp phủ của luồng này; chỉ đánh dấu vùng cần vẽ lại.
            unsafe {
                let _ = InvalidateRect(Some(self.hwnd), Some(&rc), false);
            }
        }
    }

    /// Thay đổi trạng thái qua `f`, rồi đánh dấu vẽ lại vùng cũ và vùng mới.
    fn update(&mut self, f: impl FnOnce(&mut State)) {
        let before = self.chrome();
        f(self);
        let after = self.chrome();
        self.invalidate(&before);
        self.invalidate(&after);
    }

    /// Như `update` nhưng chỉ cho thay đổi trên thanh dọc, bảng màu, chú thích (không làm mới vùng chọn).
    fn update_bars(&mut self, f: impl FnOnce(&mut State)) {
        let before = self.bar_chrome();
        f(self);
        let after = self.bar_chrome();
        self.invalidate(&before);
        self.invalidate(&after);
    }

    fn cursor_at(&self, p: Point) -> PCWSTR {
        if self.dragging() {
            return self.drag_cursor;
        }
        if self.palette || self.sizes_open || self.shapes_open {
            // Bảng màu, bảng Cỡ hay bảng Hình mở: cú bấm nào ngoài bảng cũng chỉ đóng bảng.
            return IDC_ARROW;
        }
        let on_bar = [self.toolbar(), self.side_bar(), self.palette_rect()]
            .iter()
            .flatten()
            .any(|(r, _)| r.contains(p));
        if on_bar {
            return IDC_ARROW;
        }
        if let Some(g) = self.press_grip(p) {
            return cursor_for_grip(g);
        }
        if self.shape_hit_at(p).is_some() {
            return IDC_SIZEALL;
        }
        let hit = self.sel.hit_test(p, self.hit_radius(p));
        match press_mode(self.tool, !self.marks.is_empty(), hit) {
            PressMode::Draw if self.tool == Some(Tool::Text) => IDC_IBEAM,
            PressMode::Draw => IDC_CROSS,
            PressMode::Ignore => IDC_ARROW,
            PressMode::Selection => cursor_for_hit(hit),
            PressMode::Pick => IDC_ARROW,
        }
    }

    fn finish(&mut self, a: Action) {
        self.close_text(true);
        self.deselect();
        if a == Action::Cancel {
            self.result = Some(None);
        } else if let Some(r) = self.sel.rect() {
            self.result = Some(Some(Picked {
                action: a,
                rect: r,
                shapes: std::mem::take(&mut self.marks).into_shapes(),
            }));
            self.mark_bounds.clear();
        }
    }

    /// Bật `t` (tắt nếu đang bật); bật một hình thì nhớ nó cho nút Hình. Bảng Hình đóng.
    fn toggle_tool(&mut self, t: Tool) {
        self.close_text(true);
        self.deselect();
        let on = self.tool != Some(t);
        if on {
            self.remember_shape(t);
        }
        self.update_bars(|s| {
            s.close_shapes_state();
            s.tool = on.then_some(t);
        });
    }

    /// Bật `t` (không bật/tắt), nhớ nếu là hình, đóng mọi bảng.
    fn use_tool(&mut self, t: Tool) {
        if self.tool != Some(t) {
            self.close_text(true);
            self.deselect();
        }
        self.remember_shape(t);
        self.update_bars(|s| {
            s.close_flyouts_state();
            s.tool = Some(t);
        });
    }

    /// Nhớ `t` làm hình của nút Hình nếu `t` thuộc nhóm Hình.
    fn remember_shape(&mut self, t: Tool) {
        if is_shape(t) {
            LAST_SHAPE.with(|c| c.set(t));
            self.shape_tool = t;
        }
    }

    /// Hoàn tác. Đang gõ: ô có chữ thì chỉ xóa ô đó; ô rỗng thì hủy ô rồi hoàn tác hình cuối.
    /// Đang kéo: không làm gì.
    fn undo(&mut self) {
        if self.dragging() {
            return;
        }
        if let Some(b) = &self.text {
            let had_text = !b.edit.text().is_empty();
            self.close_text(false);
            if had_text {
                return;
            }
        }
        self.deselect();
        if let Some(a) = self.marks.undo() {
            self.apply_change(&a);
        }
    }

    /// Làm lại. Ô chữ đang mở hoặc đang kéo: không làm gì.
    fn redo(&mut self) {
        if self.text.is_some() || self.dragging() {
            return;
        }
        self.deselect();
        if let Some(a) = self.marks.redo() {
            self.apply_change(&a);
        }
    }

    /// Thêm hình vào `marks` (ghi sổ) và áp thay đổi.
    fn add_mark(&mut self, shape: Shape) {
        let a = self.marks.add(shape);
        self.apply_change(&a);
    }

    /// Sau mỗi thay đổi của sổ hình: cập nhật `mark_bounds` (để khi vẽ bỏ qua hình không giao vùng
    /// cần vẽ), làm mới hộp cũ và mới (cả các vòng số đổi `n`) và thanh dọc (Hoàn tác, Làm lại).
    fn apply_change(&mut self, a: &Applied) {
        let dirty = apply_bounds(&mut self.mark_bounds, self.marks.shapes(), a);
        self.invalidate(&dirty);
        let bars = self.bar_chrome();
        self.invalidate(&bars);
    }

    /// Dung sai trúng hình (px) theo DPI màn hình chứa `p`.
    fn shape_tol(&self, p: Point) -> f32 {
        dip_to_px(HIT_TOL_DIP, self.dpi_at(p))
    }

    /// Bán kính trúng tay nắm của hình đang chọn (px) theo DPI màn hình chứa `p`.
    fn grip_radius(&self, p: Point) -> f32 {
        let dpi = self.dpi_at(p);
        (scaled(HANDLE_SIZE, dpi) / 2 + scaled(HANDLE_HIT, dpi)) as f32
    }

    /// Phần nới quanh khung chọn hình khi làm mới (nửa tay nắm + 1).
    fn selection_pad(&self) -> i32 {
        scaled(HANDLE_SIZE, self.max_dpi()) / 2 + 1
    }

    /// Hình đang chọn như đang hiện (bản xem trước khi đang kéo) cùng hộp bao.
    fn shown_selection(&self) -> Option<(&Shape, Rect)> {
        if let (Some(_), Some(s)) = (&self.gesture, &self.preview) {
            return Some((s, self.preview_bounds));
        }
        let i = self.selected?;
        let s = self.marks.shapes().get(i)?;
        let b = self
            .mark_bounds
            .get(i)
            .copied()
            .unwrap_or_else(|| shape_bounds(s));
        Some((s, b))
    }

    /// Vùng màn hình của hình đang chọn cùng khung và tay nắm.
    fn selection_area(&self) -> Option<Rect> {
        self.shown_selection()
            .map(|(s, b)| selection_dirty(s, b, self.selection_pad()))
    }

    /// Đổi trạng thái chọn hình qua `f`, làm mới vùng chọn hình cũ và mới.
    fn change_selection(&mut self, f: impl FnOnce(&mut State)) {
        let before = self.selection_area();
        f(self);
        let after = self.selection_area();
        let dirty: Vec<Rect> = before.into_iter().chain(after).collect();
        self.invalidate(&dirty);
    }

    /// Bỏ chọn hình (hủy cử chỉ đang kéo nếu có, không ghi sổ).
    fn deselect(&mut self) {
        self.cancel_gesture();
        if self.selected.is_some() {
            self.change_selection(|s| s.selected = None);
        }
    }

    /// Hủy cử chỉ kéo hình: hình về như cũ, không ghi sổ; vẫn chọn hình.
    fn cancel_gesture(&mut self) {
        if self.gesture.is_some() || self.preview.is_some() {
            self.change_selection(|s| {
                s.gesture = None;
                s.preview = None;
            });
        }
    }

    /// Tay nắm của hình đang chọn dưới `p` (khi công cụ Chọn bật).
    fn shape_grip_at(&self, p: Point) -> Option<Grip> {
        if self.tool != Some(Tool::Select) {
            return None;
        }
        let (s, b) = self.shown_selection()?;
        grip_at(s, b, to_pt(p), self.grip_radius(p))
    }

    /// Tay nắm sẽ được nắm khi bấm tại `p`. Bấm trúng chính hình đang chọn mà xa tâm mọi tay nắm
    /// hơn nửa cỡ tay nắm (hình nhỏ: vùng trúng tay nắm phủ lòng hình) thì là di chuyển, không nắm.
    fn press_grip(&self, p: Point) -> Option<Grip> {
        let grip = self.shape_grip_at(p)?;
        if self.selected.is_some() && self.shape_hit_at(p) == self.selected {
            let (s, b) = self.shown_selection()?;
            let q = to_pt(p);
            let near = handles(s, b)
                .into_iter()
                .map(|(_, c)| (c.x - q.x).abs().max((c.y - q.y).abs()))
                .fold(f32::INFINITY, f32::min);
            if near > (scaled(HANDLE_SIZE, self.dpi_at(p)) / 2) as f32 {
                return None;
            }
        }
        Some(grip)
    }

    /// Hình trên cùng trúng `p` trong vùng chọn (khi công cụ Chọn bật).
    fn shape_hit_at(&self, p: Point) -> Option<usize> {
        if self.tool != Some(Tool::Select) || !self.sel.rect()?.contains(p) {
            return None;
        }
        topmost_hit(
            self.marks.shapes(),
            &self.mark_bounds,
            to_pt(p),
            self.shape_tol(p),
        )
    }

    /// `p` kẹp vào vùng chọn.
    fn clamp_to_sel(&self, p: Point) -> Pt {
        let q = match self.sel.rect() {
            Some(r) => Point {
                x: p.x.max(r.x).min(r.right() - 1),
                y: p.y.max(r.y).min(r.bottom() - 1),
            },
            None => p,
        };
        to_pt(q)
    }

    /// Chọn hình `index` và bắt đầu kéo nó bằng tay nắm `grip` từ `p`.
    fn begin_gesture(&mut self, index: usize, grip: Grip, p: Point) -> Effects {
        let Some(original) = self.marks.shapes().get(index).cloned() else {
            return Effects::default();
        };
        let original_bounds = self
            .mark_bounds
            .get(index)
            .copied()
            .unwrap_or_else(|| shape_bounds(&original));
        let start = self.clamp_to_sel(p);
        let off = match grip {
            Grip::Move => None,
            _ => handles(&original, original_bounds)
                .into_iter()
                .find(|(g, _)| *g == grip)
                .map(|(_, c)| Pt {
                    x: c.x - start.x,
                    y: c.y - start.y,
                }),
        }
        .filter(|o| o.x.is_finite() && o.y.is_finite())
        .unwrap_or(Pt { x: 0.0, y: 0.0 });
        self.change_selection(|s| {
            s.grip_off = off;
            s.gesture_moved = false;
            s.selected = Some(index);
            s.preview = Some(original.clone());
            s.preview_bounds = original_bounds;
            s.gesture_at = start;
            s.gesture = Some(Gesture {
                index,
                grip,
                start,
                original,
                original_bounds,
            });
        });
        self.pressed = None;
        self.side_pressed = None;
        self.drag_cursor = cursor_for_grip(grip);
        Effects {
            capture: true,
            ..Default::default()
        }
    }

    /// Kéo cử chỉ tới `at` (đã kẹp vào vùng chọn): tính hình xem trước, làm mới hộp cũ và mới.
    fn drag_gesture(&mut self, at: Pt, shift: bool) {
        let Some(g) = self.gesture.as_ref() else {
            return;
        };
        self.gesture_at = at;
        if at != g.start {
            self.gesture_moved = true;
        }
        if !self.gesture_moved {
            return;
        }
        let dpi = self.dpi_at(Point {
            x: g.start.x as i32,
            y: g.start.y as i32,
        });
        let to = Pt {
            x: at.x + self.grip_off.x,
            y: at.y + self.grip_off.y,
        };
        let shape = transform(
            g,
            to,
            shift,
            dip_to_px(MIN_EXTENT_DIP, dpi),
            self.shot_image(),
        );
        let bounds = preview_bounds(g, &shape);
        if self.preview.as_ref() == Some(&shape) {
            return;
        }
        self.change_selection(|s| {
            s.preview = Some(shape);
            s.preview_bounds = bounds;
        });
    }

    /// Chốt cử chỉ: thay hình bằng bản xem trước (một mục sổ; không đổi thì không ghi); giữ chọn.
    fn commit_gesture(&mut self) {
        let before = self.selection_area();
        let Some(g) = self.gesture.take() else {
            return;
        };
        let preview = self.preview.take().filter(|_| self.gesture_moved);
        self.gesture_moved = false;
        if let Some(a) = preview.and_then(|s| self.marks.replace(g.index, s)) {
            self.apply_change(&a);
        }
        let after = self.selection_area();
        let dirty: Vec<Rect> = before.into_iter().chain(after).collect();
        self.invalidate(&dirty);
    }

    /// Xóa hình đang chọn (ghi sổ), bỏ chọn.
    fn delete_selected(&mut self) {
        let Some(i) = self.selected else {
            return;
        };
        self.deselect();
        if let Some(a) = self.marks.remove(i) {
            self.apply_change(&a);
        }
    }

    /// Áp mức `level` và/hoặc màu `color` cho hình đang chọn (một mục sổ; Làm mờ bỏ qua).
    fn restyle_selected(&mut self, level: Option<SizeLevel>, color: Option<Rgb>) {
        if self.gesture.is_some() {
            return;
        }
        let Some((shape, b)) = self.shown_selection() else {
            return;
        };
        let Some(i) = self.selected else {
            return;
        };
        let center = Point {
            x: b.x + b.w / 2,
            y: b.y + b.h / 2,
        };
        let new = restyle(shape, level, color, self.dpi_at(center));
        let before = self.selection_area();
        if let Some(a) = self.marks.replace(i, new) {
            self.apply_change(&a);
            let after = self.selection_area();
            let dirty: Vec<Rect> = before.into_iter().chain(after).collect();
            self.invalidate(&dirty);
        }
    }

    /// Đăng ký (`on`) hoặc gỡ phím tắt Esc toàn cục của lớp phủ. Gỡ khi ô chữ mở để Esc tới được
    /// bộ gõ (hủy chuỗi soạn) và tới lớp phủ qua WM_KEYDOWN; đăng ký lại khi ô đóng.
    fn set_esc_hotkey(&mut self, on: bool) {
        if on == self.esc_registered {
            return;
        }
        // SAFETY: phím tắt gắn với cửa sổ lớp phủ của luồng này; EscHotkey gỡ lần cuối khi đóng.
        unsafe {
            if on {
                self.esc_registered = RegisterHotKey(
                    Some(self.hwnd),
                    ESC_HOTKEY_ID,
                    MOD_NOREPEAT,
                    u32::from(VK_ESCAPE.0),
                )
                .is_ok();
            } else {
                let _ = UnregisterHotKey(Some(self.hwnd), ESC_HOTKEY_ID);
                self.esc_registered = false;
            }
        }
    }

    /// (Đặt lại) bộ hẹn giờ nháy con trỏ theo `GetCaretBlinkTime`; con trỏ hiện ngay.
    /// Hệ thống tắt nháy (INFINITE hoặc 0) thì con trỏ đứng yên.
    fn restart_blink(&mut self) {
        self.caret_on = true;
        // SAFETY: chỉ đọc cài đặt hệ thống và đặt bộ hẹn giờ của chính cửa sổ lớp phủ
        // (mất theo cửa sổ; close_text hủy sớm hơn).
        unsafe {
            let ms = GetCaretBlinkTime();
            if ms == 0 || ms == u32::MAX {
                let _ = KillTimer(Some(self.hwnd), CARET_TIMER_ID);
            } else {
                SetTimer(Some(self.hwnd), CARET_TIMER_ID, ms, None);
            }
        }
    }

    /// Nhịp nháy: đảo con trỏ, chỉ làm mới ô con trỏ (không đo lại chữ).
    fn blink(&mut self) {
        let Some(caret) = self.text.as_ref().map(|b| b.caret) else {
            return;
        };
        self.caret_on = !self.caret_on;
        self.invalidate(&[caret]);
    }

    /// Mở ô chữ tại `p` (đã chốt ô cũ).
    fn open_text(&mut self, p: Point) {
        let b = TextBox::new(p, self.dpi_at(p), self.size, self.color);
        self.invalidate(&[b.dirty()]);
        place_ime(self.hwnd, b.caret, b.size);
        self.text = Some(b);
        self.high_surrogate = None;
        self.set_esc_hotkey(false);
        self.restart_blink();
        // Nút Hoàn tác sáng lên khi có ô chữ.
        let bars = self.bar_chrome();
        self.invalidate(&bars);
    }

    /// Đóng ô chữ: `commit` thì chốt vào `marks` (bỏ nếu chỉ có khoảng trắng), không thì hủy.
    fn close_text(&mut self, commit: bool) {
        if self.text.is_none() {
            return;
        }
        // Chuỗi bộ gõ đang soạn: chốt thì hoàn tất vào chính ô này, hủy thì bỏ.
        let pending = ime_end_composition(self.hwnd, commit);
        let Some(mut b) = self.text.take() else {
            return;
        };
        self.invalidate(&[b.dirty()]);
        if let Some(s) = pending {
            b.edit.insert(&s);
        }
        self.high_surrogate = None;
        // SAFETY: hủy bộ hẹn giờ nháy của chính cửa sổ lớp phủ.
        unsafe {
            let _ = KillTimer(Some(self.hwnd), CARET_TIMER_ID);
        }
        self.set_esc_hotkey(true);
        if commit {
            if let Some(shape) = b.edit.to_shape() {
                // Làm mới cả hộp của hình mới: chữ hoàn tất từ bộ gõ có thể vượt khung cũ.
                self.add_mark(shape);
            }
        }
        // Nút Hoàn tác có thể đổi trạng thái (mờ, sáng).
        let bars = self.bar_chrome();
        self.invalidate(&bars);
    }

    /// Sửa ô chữ đang gõ qua `f`, đo lại, làm mới vùng cũ và mới, dời cửa sổ soạn của bộ gõ.
    fn edit_text(&mut self, f: impl FnOnce(&mut TextEdit)) {
        let Some(b) = self.text.as_mut() else {
            return;
        };
        let before = b.dirty();
        f(&mut b.edit);
        b.measure();
        let (after, caret, size) = (b.dirty(), b.caret, b.size);
        self.invalidate(&[before, after]);
        place_ime(self.hwnd, caret, size);
        self.restart_blink();
    }

    /// WM_CHAR khi đang gõ; `false` khi không có ô chữ (để xử lý mặc định).
    fn on_char(&mut self, unit: u16) -> bool {
        if self.eat_char.take() == Some(unit) {
            return true;
        }
        if self.palette && self.field.is_some() {
            // Ô số đang sửa: chỉ nhận chữ số; mọi ký tự khác (kể cả chữ của công cụ) bị nuốt.
            let d = char::from_u32(u32::from(unit)).filter(char::is_ascii_digit);
            if let (Some(d), Some(f)) = (d, self.field.as_mut()) {
                f.digit(d);
                self.invalidate_picker();
            }
            return true;
        }
        if self.text.is_none() {
            return false;
        }
        if (0xD800..0xDC00).contains(&unit) {
            self.high_surrogate = Some(unit);
            return true;
        }
        let s = if (0xDC00..0xE000).contains(&unit) {
            match self.high_surrogate.take() {
                Some(h) => String::from_utf16_lossy(&[h, unit]),
                None => return true,
            }
        } else {
            self.high_surrogate = None;
            match unit {
                0x08 => {
                    self.edit_text(|e| e.backspace());
                    return true;
                }
                0x0D => "\n".to_string(),
                // Ký tự điều khiển khác (Ctrl+V, Ctrl+Z, Tab, Ctrl+Enter...) không vào ô.
                u if u < 0x20 || u == 0x7F => return true,
                u => String::from_utf16_lossy(&[u]),
            }
        };
        self.edit_text(|e| e.insert(&s));
        true
    }

    /// Phím khi đang gõ (WM_KEYDOWN, trừ Esc): sửa, di con trỏ, Ctrl+Z/V/D/S/C/P/T.
    fn text_key(&mut self, vk: u16) {
        if key_held(VK_CONTROL) {
            match vk {
                // Z; Ctrl+Shift+Z (làm lại) và Ctrl+Y khi ô mở: không làm gì.
                0x5A if !key_held(VK_SHIFT) => self.undo(),
                0x56 => self.paste(), // V
                0x44 => self.finish(Action::Upload),
                0x53 => self.finish(Action::Save),
                0x43 => self.finish(Action::Copy),
                // P: `finish` chốt ô chữ (cả chuỗi bộ gõ đang soạn) trước khi ghim.
                0x50 => self.finish(Action::Pin),
                // T: như P, chốt ô chữ trước khi nhận dạng (chữ vừa gõ có trong ảnh nhận dạng).
                0x54 => self.finish(Action::Ocr),
                // N: thêm bước (chỉ khi có phiên chụp từng bước), chốt ô chữ như P.
                0x4E if self.steps => self.finish(Action::AddStep),
                _ => {}
            }
            return;
        }
        let mv: fn(&mut TextEdit) = match VIRTUAL_KEY(vk) {
            VK_DELETE => TextEdit::delete,
            VK_LEFT => TextEdit::left,
            VK_RIGHT => TextEdit::right,
            VK_UP => TextEdit::up,
            VK_DOWN => TextEdit::down,
            VK_HOME => TextEdit::home,
            VK_END => TextEdit::end,
            _ => return,
        };
        self.edit_text(mv);
    }

    /// Ctrl+V: dán chữ thuần của clipboard vào ô chữ.
    fn paste(&mut self) {
        if self.text.is_none() {
            return;
        }
        let Some(raw) = crate::clipboard::paste_text(self.hwnd) else {
            return;
        };
        let clean = sanitize_paste(&raw);
        if !clean.is_empty() {
            self.edit_text(|e| e.insert(&clean));
        }
    }

    /// Nhấn hoặc nhả Shift khi đang kéo Đường thẳng, Mũi tên, Hình chữ nhật: cập nhật nét ngay
    /// theo điểm chuột cuối.
    fn shift_changed(&mut self, shift: bool) {
        if self.gesture.is_some() {
            let at = self.gesture_at;
            self.drag_gesture(at, shift);
            return;
        }
        let Some(st) = self.stroke.as_mut() else {
            return;
        };
        if !matches!(
            st.tool,
            Tool::Line | Tool::Arrow | Tool::Rect | Tool::Ellipse | Tool::Blur
        ) {
            return;
        }
        let at = st.cursor;
        // SAFETY: ảnh gốc sống suốt pick_region (xem `State::image`).
        let img = unsafe { &*self.image };
        if let Some(r) = extend_stroke(st, at, shift, img) {
            self.invalidate(&[r]);
        }
    }

    /// Dùng màu `c` cho công cụ vẽ (và ô chữ đang gõ), đóng bảng màu.
    fn choose_color(&mut self, c: Rgb) {
        LAST_COLOR.with(|l| l.set(c));
        self.update_bars(|s| {
            s.color = c;
            s.close_picker_state();
        });
        self.edit_text(|e| e.set_color(c));
        self.restyle_selected(None, Some(c));
    }

    /// Bỏ mọi trạng thái tạm của bảng màu và đóng nó (màu tự chọn đã thêm vẫn giữ).
    fn close_picker_state(&mut self) {
        self.palette = false;
        self.field = None;
        self.picker_pressed = None;
        self.picker_drag = None;
        self.target = None;
    }

    /// Đóng bảng màu, màu vẽ không đổi.
    fn close_palette(&mut self) {
        self.update_bars(|s| s.close_picker_state());
    }

    fn open_palette(&mut self) {
        self.update_bars(|s| {
            s.close_flyouts_state();
            s.palette = true;
            s.pick = PickColor::from_rgb(s.color);
            s.target = None;
            s.field = None;
            s.picker_pressed = None;
        });
    }

    fn close_sizes_state(&mut self) {
        self.sizes_open = false;
        self.size_hover = None;
        self.size_pressed = None;
    }

    /// Đóng bảng Cỡ, mức không đổi.
    fn close_sizes(&mut self) {
        self.update_bars(|s| s.close_sizes_state());
    }

    /// Mở bảng Cỡ (đóng bảng khác).
    fn open_sizes(&mut self) {
        self.update_bars(|s| {
            s.close_flyouts_state();
            s.sizes_open = true;
        });
    }

    fn close_shapes_state(&mut self) {
        self.shapes_open = false;
        self.shape_hover = None;
        self.shape_pressed = None;
    }

    /// Đóng bảng Hình, công cụ không đổi.
    fn close_shapes(&mut self) {
        self.update_bars(|s| s.close_shapes_state());
    }

    /// Mở bảng Hình (đóng bảng khác).
    fn open_shapes(&mut self) {
        self.update_bars(|s| {
            s.close_flyouts_state();
            s.shapes_open = true;
        });
    }

    /// Đóng cả ba bảng (màu, Cỡ, Hình) và bỏ trạng thái tạm của chúng: ba bảng không bao giờ mở
    /// cùng lúc.
    fn close_flyouts_state(&mut self) {
        self.close_picker_state();
        self.close_sizes_state();
        self.close_shapes_state();
    }

    /// Đặt mức cỡ cho hình vẽ sau đó (nhớ qua các lần mở); ô chữ đang mở đổi cỡ theo.
    fn set_size(&mut self, level: SizeLevel) {
        LAST_SIZE.with(|l| l.set(level));
        if level != self.size {
            self.update_bars(|s| s.size = level);
        }
        self.restyle_selected(Some(level), None);
        let color = self.color;
        let Some(b) = self.text.as_mut() else {
            return;
        };
        if b.size == dip_to_px(level.text_dip(), b.dpi) {
            return;
        }
        let before = b.dirty();
        b.resize(level, color);
        let (after, caret, size) = (b.dirty(), b.caret, b.size);
        self.invalidate(&[before, after]);
        place_ime(self.hwnd, caret, size);
        self.restart_blink();
    }

    fn activate(&mut self, item: SideItem) {
        match item {
            SideItem::Tool(t) => self.toggle_tool(t),
            // Đang dùng một hình: bật/tắt bảng Hình; chưa: bật hình nhớ, không mở bảng.
            SideItem::Shapes if !self.tool.is_some_and(is_shape) => self.use_tool(self.shape_tool),
            SideItem::Shapes if self.shapes_open => self.close_shapes(),
            SideItem::Shapes => self.open_shapes(),
            SideItem::Size if self.sizes_open => self.close_sizes(),
            SideItem::Size => self.open_sizes(),
            SideItem::Color if self.palette => self.close_palette(),
            SideItem::Color => self.open_palette(),
        }
    }

    /// Nhả chuột trên mục `item` của thanh ngang: Hoàn tác, Làm lại (lớp phủ vẫn mở) hoặc kết
    /// thúc bằng hành động.
    fn activate_bar(&mut self, item: BarItem) {
        match item {
            BarItem::Undo => self.undo(),
            BarItem::Redo => self.redo(),
            BarItem::Act(a) => self.finish(a),
        }
    }

    /// Chốt ô số đang sửa (nếu có) vào màu hiện tại của phần tự chọn.
    fn commit_field(&mut self) {
        if let Some(f) = self.field.take() {
            self.pick = f.commit(self.pick);
            self.invalidate_picker();
        }
    }

    /// Đổi màu hiện tại của phần tự chọn.
    fn set_pick(&mut self, c: PickColor) {
        if c != self.pick {
            self.pick = c;
            self.invalidate_picker();
        }
    }

    /// Kéo hoặc bấm trong ô sắc độ (đổi Hue, Sat) hay trên thanh độ sáng (đổi Lum) tại `p`.
    fn drag_picker(&mut self, kind: PickerDrag, p: Point) {
        let Some((pal, dpi)) = self.palette_rect() else {
            return;
        };
        let l = picker_layout(dpi, self.expanded);
        let cur = self.pick.hls;
        let hls = match kind {
            PickerDrag::Spectrum => {
                let (h, s) = spectrum_hs(at(pal, l.spectrum), p.x, p.y);
                Hls { h, s, l: cur.l }
            }
            PickerDrag::Lum => Hls {
                l: lum_at(at(pal, l.lum_bar), p.y),
                ..cur
            },
        };
        if hls != cur {
            self.set_pick(PickColor::from_hls(hls));
        }
    }

    /// Nhấn chuột trên phần `hit` của bảng màu tại `p`.
    fn picker_down(&mut self, hit: PickerHit, p: Point) -> Effects {
        self.commit_field();
        match hit {
            PickerHit::Spectrum | PickerHit::Lum => {
                let kind = if hit == PickerHit::Spectrum {
                    PickerDrag::Spectrum
                } else {
                    PickerDrag::Lum
                };
                self.picker_drag = Some(kind);
                self.drag_cursor = IDC_ARROW;
                self.drag_picker(kind, p);
                Effects {
                    capture: true,
                    ..Default::default()
                }
            }
            PickerHit::Field(i) => {
                self.field = Some(FieldEdit::new(i, self.pick));
                self.invalidate_picker();
                Effects::default()
            }
            other => {
                self.picker_pressed = Some(other);
                self.invalidate_picker();
                Effects::default()
            }
        }
    }

    /// Nhả chuột trên phần `hit` đã nhấn.
    fn picker_activate(&mut self, hit: PickerHit) {
        match hit {
            PickerHit::Basic(i) => {
                let Some(c) = PALETTE.get(i).copied() else {
                    return;
                };
                if self.expanded {
                    self.set_pick(PickColor::from_rgb(c));
                } else {
                    self.choose_color(c);
                }
            }
            PickerHit::Custom(i) => {
                let c = self.custom.slots.get(i).copied().flatten();
                if self.expanded {
                    self.target = Some(i);
                    if let Some(c) = c {
                        self.pick = PickColor::from_rgb(c);
                    }
                    self.invalidate_picker();
                } else if let Some(c) = c {
                    self.choose_color(c);
                }
            }
            PickerHit::Define => {
                if !self.expanded {
                    self.update_bars(|s| s.expanded = true);
                }
            }
            PickerHit::Add => {
                let had = self.target;
                let idx = self.custom.add(self.pick.rgb, had);
                if had.is_some() {
                    self.target = Some((idx + 1) % CUSTOM_SLOTS);
                }
                self.invalidate_picker();
            }
            PickerHit::Ok => self.choose_color(self.pick.rgb),
            PickerHit::Cancel => self.close_palette(),
            PickerHit::Spectrum | PickerHit::Lum | PickerHit::Field(_) | PickerHit::Panel => {}
        }
    }

    /// WM_KEYDOWN (không Ctrl) khi bảng màu mở; `true` nếu bảng đã dùng phím.
    fn picker_key(&mut self, vk: u16) -> bool {
        if let Some(f) = self.field.as_mut() {
            match VIRTUAL_KEY(vk) {
                VK_TAB => {
                    let back = key_held(VK_SHIFT);
                    let idx = f.idx;
                    self.pick = f.commit(self.pick);
                    let next = if back {
                        (idx + FIELD_COUNT - 1) % FIELD_COUNT
                    } else {
                        (idx + 1) % FIELD_COUNT
                    };
                    self.field = Some(FieldEdit::new(next, self.pick));
                }
                VK_UP | VK_DOWN => {
                    self.pick = f.step(self.pick, vk == VK_UP.0);
                }
                VK_RETURN => {
                    self.eat_char = Some(0x0D);
                    self.commit_field();
                }
                VK_BACK => f.backspace(),
                _ => {}
            }
            self.invalidate_picker();
            return true;
        }
        if vk == VK_RETURN.0 && self.expanded {
            self.eat_char = Some(0x0D);
            self.choose_color(self.pick.rgb);
            return true;
        }
        // Phím chữ không chọn công cụ khi bảng mở; ô chữ đang gõ thì vẫn nhận phím như trước.
        self.text.is_none()
    }

    /// Esc (phím hoặc phím tắt toàn cục): bảng Cỡ hay bảng Hình đang mở thì chỉ đóng bảng; ô số
    /// đang sửa thì bỏ sửa; bảng màu đang mở thì chỉ đóng bảng; đang gõ thì hủy ô chữ (trừ khi bộ
    /// gõ đang soạn: Esc thuộc về bộ gõ); còn lại đóng lớp phủ.
    fn escape(&mut self) {
        if self.sizes_open {
            self.close_sizes();
        } else if self.shapes_open {
            self.close_shapes();
        } else if self.palette && self.field.is_some() {
            self.field = None;
            self.invalidate_picker();
        } else if self.palette {
            self.close_palette();
        } else if self.text.is_some() {
            if !ime_composing(self.hwnd) {
                self.close_text(false);
            }
        } else if self.gesture.is_some() {
            // Chuột được trả sau khi trả mượn trạng thái (`release_if_idle`).
            self.cancel_gesture();
        } else if self.selected.is_some() {
            self.deselect();
        } else {
            self.result = Some(None);
        }
    }

    /// Bấm trong vùng chọn khi công cụ Chữ đang chọn: chốt ô đang gõ (nếu có), mở ô mới tại `p`.
    fn text_press(&mut self, p: Point) -> Effects {
        self.close_text(true);
        self.open_text(p);
        Effects::default()
    }

    /// Chốt nét đang vẽ vào `marks` (hoặc bỏ nếu quá ngắn).
    fn commit_stroke(&mut self) {
        let Some(s) = self.stroke.take() else {
            return;
        };
        let drawn = s.shape.bounds();
        match end_stroke(s, self.shot_image()) {
            Some(shape) => self.add_mark(shape),
            None => self.invalidate(&drawn.into_iter().collect::<Vec<_>>()),
        }
    }

    /// Mất chuột giữa chừng: chốt nét, kết thúc lần kéo vùng.
    fn lost_capture(&mut self) {
        self.picker_drag = None;
        if self.stroke.is_some() {
            self.commit_stroke();
        }
        if self.gesture.is_some() {
            self.commit_gesture();
        }
        self.snap_press = None;
        if self.sel.is_dragging() {
            self.update(|s| {
                s.sel.release();
                s.snap_hover = s.snap_at(s.mouse);
            });
        }
    }

    fn on_down(&mut self, p: Point) -> Effects {
        if self.dragging() {
            return Effects::default();
        }
        // Bấm ra chỗ khác thì chốt ô chữ; trên thanh dọc (Màu, Cỡ, công cụ tự chốt khi cần), trên
        // Hoàn tác/Làm lại của thanh ngang và khi một bảng mở (để đổi màu, cỡ ô đang gõ) thì chưa.
        let on_side = self.side_bar().is_some_and(|(r, _)| r.contains(p));
        let on_history = matches!(
            self.bar_item_at(p),
            Some(BarItem::Undo) | Some(BarItem::Redo)
        );
        let flyout = self.palette || self.sizes_open || self.shapes_open;
        if self.text.is_some() && !flyout && !on_side && !on_history {
            self.close_text(true);
        }
        if self.shapes_open {
            if let Some(t) = self.shape_cell_at(p) {
                self.shape_pressed = Some(t);
                return Effects::default();
            }
            // Bấm ngoài ô của bảng Hình (lề bảng thì không làm gì) chỉ đóng bảng.
            if !self.shapes_rect().is_some_and(|(r, _)| r.contains(p)) {
                self.close_shapes();
            }
            return Effects::default();
        }
        if self.sizes_open {
            if let Some(level) = self.size_cell_at(p) {
                self.size_pressed = Some(level);
                return Effects::default();
            }
            // Bấm ngoài ô của bảng Cỡ (lề bảng thì không làm gì) chỉ đóng bảng.
            if !self.size_rect().is_some_and(|(r, _)| r.contains(p)) {
                self.close_sizes();
            }
            return Effects::default();
        }
        if self.palette {
            if let Some(hit) = self.picker_hit_at(p) {
                return self.picker_down(hit, p);
            }
            // Bấm ngoài bảng màu chỉ đóng bảng (kể cả trên nút Màu), không làm gì khác.
            self.close_palette();
            return Effects::default();
        }
        if let Some((bar, _)) = self.toolbar() {
            if bar.contains(p) {
                self.pressed = self.bar_item_at(p);
                return Effects::default();
            }
        }
        if let Some((side, _)) = self.side_bar() {
            if side.contains(p) {
                self.side_pressed = self.side_item_at(p);
                return Effects::default();
            }
        }
        // Công cụ Chọn: tay nắm của hình đang chọn, rồi hình trên cùng trúng điểm bấm (trước tay
        // nắm vùng chọn).
        if let (Some(grip), Some(i)) = (self.press_grip(p), self.selected) {
            return self.begin_gesture(i, grip, p);
        }
        if let Some(i) = self.shape_hit_at(p) {
            return self.begin_gesture(i, Grip::Move, p);
        }
        let rad = self.hit_radius(p);
        let hit = self.sel.hit_test(p, rad);
        match press_mode(self.tool, !self.marks.is_empty(), hit) {
            PressMode::Ignore => Effects::default(),
            PressMode::Pick => {
                self.deselect();
                Effects::default()
            }
            PressMode::Draw => {
                let Some(tool) = self.tool else {
                    return Effects::default();
                };
                if tool == Tool::Text {
                    return self.text_press(p);
                }
                let at = Pt {
                    x: p.x as f32,
                    y: p.y as f32,
                };
                let dpi = self.dpi_at(p);
                if tool == Tool::Step {
                    // Đặt ngay một vòng tại điểm bấm (không kéo, không giữ chuột); lõi đánh số.
                    self.add_mark(Shape::Step {
                        center: at,
                        radius: dip_to_px(self.size.step_dip(), dpi),
                        n: 0,
                        color: self.color,
                    });
                    return Effects::default();
                }
                let img = self.shot_image();
                let Some(stroke) = begin_stroke(tool, at, dpi, self.size, self.color, img) else {
                    return Effects::default();
                };
                let dirty = stroke.shape.bounds();
                self.update_bars(|s| {
                    s.hover = None;
                    s.side_hover = None;
                    s.pressed = None;
                    s.side_pressed = None;
                    s.drag_cursor = IDC_CROSS;
                    s.stroke = Some(stroke);
                });
                self.invalidate(&dirty.into_iter().collect::<Vec<_>>());
                Effects {
                    capture: true,
                    ..Default::default()
                }
            }
            PressMode::Selection => {
                // Gợi ý đang chạy lúc bấm: giữ xem trước tới khi chuột rời điểm bấm; nhả không kéo
                // thì chọn ứng viên tại điểm bấm (on_up).
                let press = self.snap_active().then_some(p);
                if hit == Hit::Outside {
                    self.deselect();
                }
                self.update(|s| {
                    s.snap_press = press;
                    s.snap_hover = press.and_then(|a| snap::candidate_at(&s.cands, a));
                    s.new_drag = hit == Hit::Outside;
                    s.drag_cursor = cursor_for_hit(hit);
                    s.hover = None;
                    s.side_hover = None;
                    s.pressed = None;
                    s.side_pressed = None;
                    s.sel.press(p, rad);
                });
                Effects {
                    capture: true,
                    ..Default::default()
                }
            }
        }
    }

    fn on_move(&mut self, p: Point) {
        let hint_before = self.hint().map(|h| h.0);
        self.mouse = p;
        if let Some(kind) = self.picker_drag {
            self.drag_picker(kind, p);
        } else if self.gesture.is_some() {
            let at = self.clamp_to_sel(p);
            self.drag_gesture(at, key_held(VK_SHIFT));
        } else if self.stroke.is_some() {
            let at = Pt {
                x: p.x as f32,
                y: p.y as f32,
            };
            // SAFETY: ảnh gốc sống suốt pick_region (xem `State::image`).
            let img = unsafe { &*self.image };
            let r = self
                .stroke
                .as_mut()
                .and_then(|st| extend_stroke(st, at, key_held(VK_SHIFT), img));
            if let Some(r) = r {
                self.invalidate(&[r]);
            }
        } else if self.sel.is_dragging() {
            self.update(|s| {
                s.sel.drag_to(p);
                // Chuột rời điểm bấm từ MIN_SIZE: thành kéo vùng, bỏ xem trước.
                if s.snap_press.is_some_and(|a| !snap::is_click(a, p)) {
                    s.snap_press = None;
                    s.snap_hover = None;
                }
            });
        } else {
            let size_hover = self.size_cell_at(p);
            if size_hover != self.size_hover {
                self.update_bars(|s| s.size_hover = size_hover);
            }
            let shape_hover = self.shape_cell_at(p);
            if shape_hover != self.shape_hover {
                self.update_bars(|s| s.shape_hover = shape_hover);
            }
            let hover = self.bar_item_at(p);
            if hover != self.hover {
                self.update_bars(|s| s.hover = hover);
            }
            let side_hover = self.side_item_at(p);
            if side_hover != self.side_hover {
                self.update_bars(|s| s.side_hover = side_hover);
            }
            let snap_hover = self.snap_at(p);
            if snap_hover != self.snap_hover {
                self.update(|s| s.snap_hover = snap_hover);
            }
        }
        let hint_after = self.hint().map(|h| h.0);
        if hint_before != hint_after {
            let dirty: Vec<Rect> = [hint_before, hint_after]
                .into_iter()
                .flatten()
                .map(|r| inflate(r, 1))
                .collect();
            self.invalidate(&dirty);
        }
        set_cursor(self.cursor_at(p));
    }

    fn on_up(&mut self, p: Point) -> Effects {
        if let Some(kind) = self.picker_drag {
            // Nhả ở đâu (kể cả ngoài bảng) cũng chỉ kết thúc lần kéo, không đóng bảng.
            self.drag_picker(kind, p);
            self.picker_drag = None;
            return Effects {
                release: true,
                ..Default::default()
            };
        }
        if self.gesture.is_some() {
            let at = self.clamp_to_sel(p);
            self.drag_gesture(at, key_held(VK_SHIFT));
            self.commit_gesture();
            return Effects {
                release: true,
                ..Default::default()
            };
        }
        if self.stroke.is_some() {
            let at = Pt {
                x: p.x as f32,
                y: p.y as f32,
            };
            // SAFETY: ảnh gốc sống suốt pick_region (xem `State::image`).
            let img = unsafe { &*self.image };
            let r = self
                .stroke
                .as_mut()
                .and_then(|st| extend_stroke(st, at, key_held(VK_SHIFT), img));
            if let Some(r) = r {
                self.invalidate(&[r]);
            }
            self.commit_stroke();
            return Effects {
                release: true,
                ..Default::default()
            };
        }
        if self.sel.is_dragging() {
            let press = self.snap_press.take();
            self.update(|s| {
                s.sel.drag_to(p);
                s.sel.release();
                // Bấm không kéo (dịch < MIN_SIZE theo cả hai trục) khi gợi ý đang chạy: chọn ứng
                // viên tại điểm bấm. Kéo dẹt (một trục >= MIN_SIZE) không chọn gì, như đợt 7.
                if let Some(a) = press.filter(|a| snap::is_click(*a, p)) {
                    if let Some(c) = snap::candidate_at(&s.cands, a) {
                        s.select_candidate(c);
                    }
                }
                s.hover = s.bar_item_at(p);
                s.snap_hover = s.snap_at(p);
            });
            return Effects {
                release: true,
                ..Default::default()
            };
        }
        if let Some(level) = self.size_pressed.take() {
            if self.size_cell_at(p) == Some(level) {
                self.set_size(level);
                self.close_sizes();
            }
            return Effects::default();
        }
        if let Some(t) = self.shape_pressed.take() {
            if self.shape_cell_at(p) == Some(t) {
                self.use_tool(t);
            }
            return Effects::default();
        }
        if let Some(hit) = self.picker_pressed.take() {
            if self.picker_hit_at(p) == Some(hit) {
                self.picker_activate(hit);
            }
            self.invalidate_picker();
            return Effects::default();
        }
        if let Some(item) = self.side_pressed.take() {
            if self.side_item_at(p) == Some(item) {
                self.activate(item);
            }
            return Effects::default();
        }
        if let Some(item) = self.pressed.take() {
            if self.bar_item_at(p) == Some(item) {
                self.activate_bar(item);
            }
        }
        Effects::default()
    }

    /// WM_KEYDOWN; `repeat`: lần lặp của phím đang giữ.
    fn on_key(&mut self, vk: u16, repeat: bool) {
        self.eat_char = None;
        if vk == VK_ESCAPE.0 {
            self.escape();
            return;
        }
        if vk == VK_SHIFT.0 {
            self.shift_changed(true);
            return;
        }
        if self.palette && !key_held(VK_CONTROL) && self.picker_key(vk) {
            return;
        }
        if self.text.is_some() {
            self.text_key(vk);
            return;
        }
        if vk == VK_RETURN.0 && !repeat && self.sel.rect().is_none() {
            // Enter khi chưa có vùng chọn: chọn ứng viên đang xem trước (như bấm).
            if let Some(c) = self.snap_hover.filter(|_| self.snap_active()) {
                self.update(|s| {
                    s.select_candidate(c);
                    s.hover = s.bar_item_at(s.mouse);
                });
            }
            return;
        }
        if self.dragging() || self.sel.rect().is_none() {
            return;
        }
        let ctrl = key_held(VK_CONTROL);
        if !ctrl {
            if vk == VK_DELETE.0 || vk == VK_BACK.0 {
                self.delete_selected();
                return;
            }
            if !key_held(VK_MENU) && !repeat {
                if let Some(t) = Tool::from_key(vk) {
                    self.toggle_tool(t);
                } else if let Some(level) = size_for_key(self.size, vk) {
                    self.set_size(level);
                }
            }
            return;
        }
        let a = match vk {
            // Ctrl+Shift+Z làm lại.
            0x5A if key_held(VK_SHIFT) => {
                self.redo();
                return;
            }
            0x5A => {
                // Z
                self.undo();
                return;
            }
            0x59 => {
                // Y
                self.redo();
                return;
            }
            0x44 => Action::Upload, // D
            0x53 => Action::Save,   // S
            0x43 => Action::Copy,   // C
            0x50 => Action::Pin,    // P
            0x54 => Action::Ocr,    // T
            // N: thêm bước, chỉ khi có phiên chụp từng bước.
            0x4E if self.steps => Action::AddStep,
            _ => return,
        };
        self.finish(a);
    }

    fn paint(&mut self, target: HDC, area: Rect) {
        let Some(area) = intersect(
            area,
            Rect {
                x: 0,
                y: 0,
                w: self.w,
                h: self.h,
            },
        ) else {
            return;
        };
        if let Some(dc) = self.back.as_ref().map(|b| b.dc) {
            // Bộ đệm cố định cùng tọa độ với client: chỉ vẽ trong `area` rồi chép đúng vùng đó.
            // SAFETY: DC của BackBuffer còn sống; vùng cắt được bỏ ngay sau khi vẽ.
            unsafe {
                IntersectClipRect(dc, area.x, area.y, area.right(), area.bottom());
            }
            self.draw(&Painter { dc, ox: 0, oy: 0 }, area);
            // SAFETY: như trên; target là DC của BeginPaint còn hiệu lực.
            unsafe {
                SelectClipRgn(dc, None);
                let _ = BitBlt(
                    target,
                    area.x,
                    area.y,
                    area.w,
                    area.h,
                    Some(dc),
                    area.x,
                    area.y,
                    SRCCOPY,
                );
            }
            return;
        }
        // Dự phòng khi không tạo được bộ đệm cố định: bitmap tạm cỡ vùng cần vẽ.
        // SAFETY: DC và bitmap bộ nhớ chỉ sống trong khối này và được giải phóng ở cuối,
        // sau khi bỏ chọn bitmap.
        unsafe {
            let mem = CreateCompatibleDC(Some(target));
            if mem.is_invalid() {
                return;
            }
            let bmp = CreateCompatibleBitmap(target, area.w, area.h);
            if bmp.is_invalid() {
                let _ = DeleteDC(mem);
                return;
            }
            let old = SelectObject(mem, bmp.into());
            let p = Painter {
                dc: mem,
                ox: area.x,
                oy: area.y,
            };
            self.draw(&p, area);
            let _ = BitBlt(
                target,
                area.x,
                area.y,
                area.w,
                area.h,
                Some(mem),
                0,
                0,
                SRCCOPY,
            );
            SelectObject(mem, old);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(mem);
        }
    }

    fn draw(&mut self, p: &Painter, area: Rect) {
        // Khung hiển thị: vùng chọn, hoặc ứng viên xem trước (sáng trong, tối ngoài như vùng chọn).
        let shown = self.shown_rect();
        let sel = shown.and_then(|r| intersect(r, area));
        let (w, h) = (self.w, self.h);
        // SAFETY: `bright`/`bright_len` là bộ đệm của Screenshot::image, sống suốt pick_region
        // (State bị hủy trước khi pick_region trả về) và không bị ai ghi trong lúc này.
        let bright = unsafe { std::slice::from_raw_parts(self.bright, self.bright_len) };
        let dark = self.dark.as_slice();
        // Nền tối chỉ vẽ ngoài khung, ảnh gốc chỉ vẽ trong khung: không vẽ chồng.
        match sel {
            None => p.image(dark, w, h, area),
            Some(s) => {
                for strip in [
                    Rect {
                        h: s.y - area.y,
                        ..area
                    },
                    Rect {
                        y: s.bottom(),
                        h: area.bottom() - s.bottom(),
                        ..area
                    },
                    Rect {
                        y: s.y,
                        w: s.x - area.x,
                        h: s.h,
                        ..area
                    },
                    Rect {
                        x: s.right(),
                        y: s.y,
                        w: area.right() - s.right(),
                        h: s.h,
                    },
                ] {
                    if strip.w > 0 && strip.h > 0 {
                        p.image(dark, w, h, strip);
                    }
                }
                p.image(bright, w, h, s);
            }
        }
        // Hình đánh dấu: sau ảnh sáng, trước khung và các thanh; cắt theo vùng chọn.
        if let (Some(clip), true) = (sel, self.gdiplus) {
            // Chỉ vẽ các hình có hộp bao giao vùng cần vẽ; giữ thứ tự bằng cách vẽ từng dải liền.
            // Đang kéo hình: bản xem trước vẽ thay hình đó, đúng vị trí trong thứ tự.
            let shapes = self.marks.shapes();
            let sub = match (&self.gesture, &self.preview) {
                (Some(g), Some(s)) => Some((g.index, s)),
                _ => None,
            };
            let visible = |i: usize| match sub {
                Some((gi, _)) if gi == i => intersect(self.preview_bounds, clip).is_some(),
                _ => self
                    .mark_bounds
                    .get(i)
                    .is_none_or(|r| intersect(*r, clip).is_some()),
            };
            let draw_run = |a: usize, b: usize| {
                if let Some(run) = shapes.get(a..b).filter(|r| !r.is_empty()) {
                    draw_shapes(p.dc, run, p.ox, p.oy, clip);
                }
            };
            let mut i = 0;
            while i < shapes.len() {
                if !visible(i) {
                    i += 1;
                    continue;
                }
                let start = i;
                while i < shapes.len() && visible(i) {
                    i += 1;
                }
                match sub {
                    Some((gi, s)) if (start..i).contains(&gi) => {
                        draw_run(start, gi);
                        draw_shapes(p.dc, std::slice::from_ref(s), p.ox, p.oy, clip);
                        draw_run(gi + 1, i);
                    }
                    _ => draw_run(start, i),
                }
            }
            if let Some(s) = &self.stroke {
                draw_shapes(p.dc, std::slice::from_ref(&s.shape), p.ox, p.oy, clip);
            }
            let typing = self
                .text
                .as_ref()
                .filter(|b| intersect(b.frame, clip).is_some())
                .and_then(|b| b.edit.to_shape());
            if let Some(shape) = typing {
                draw_shapes(p.dc, std::slice::from_ref(&shape), p.ox, p.oy, clip);
            }
        }
        // Khung và tay nắm của hình đang chọn: chỉ trên màn hình, không qua draw_shapes/burn.
        if let Some((shape, b)) = self.shown_selection() {
            if intersect(selection_dirty(shape, b, self.selection_pad()), area).is_some() {
                if let Some(f) = frame(shape, b) {
                    p.frame_outside(inflate(f, -1), ACCENT);
                }
                for (_, c) in handles(shape, b) {
                    let (hx, hy) = (c.x.round() as i32, c.y.round() as i32);
                    let size = scaled(HANDLE_SIZE, self.dpi_at(Point { x: hx, y: hy }));
                    p.handle(hx, hy, size);
                }
            }
        }
        // Khung nét đứt và con trỏ của ô đang gõ: chỉ trên màn hình, không in ra ảnh.
        if let Some(b) = &self.text {
            p.dashed_frame(b.frame, b.dpi);
            if self.caret_on {
                p.fill(b.caret, ACCENT);
            }
        }
        let Some(r) = shown else {
            self.draw_hint(p, area);
            return;
        };
        p.border(r);
        // Xem trước ứng viên: không tay nắm (và không thanh công cụ: `bars` cần vùng chọn).
        if self.snap_hover.is_none() && self.sel.rect().is_some() {
            for (hx, hy) in [
                (r.x, r.y),
                (r.x + r.w / 2, r.y),
                (r.right(), r.y),
                (r.right(), r.y + r.h / 2),
                (r.right(), r.bottom()),
                (r.x + r.w / 2, r.bottom()),
                (r.x, r.bottom()),
                (r.x, r.y + r.h / 2),
            ] {
                let size = scaled(HANDLE_SIZE, self.dpi_at(Point { x: hx, y: hy }));
                p.handle(hx, hy, size);
            }
        }
        if let Some((rect, mut text, dpi)) = self.label() {
            let font = self.font(dpi);
            p.bubble(
                rect,
                scaled(4, dpi),
                &mut text,
                font,
                OVERLAY_LABEL,
                OVERLAY_LABEL,
            );
        }
        if let Some((bar, dpi)) = self.toolbar() {
            let look = BarLook {
                hover: self.hover,
                // Đang gõ thì Hoàn tác xóa ô chữ (hoặc hủy ô rỗng rồi hoàn tác hình cuối).
                can_undo: self.marks.can_undo() || self.text.is_some(),
                can_redo: self.marks.can_redo(),
                steps: self.steps,
            };
            p.toolbar(bar, dpi, &look, self.gdiplus);
        }
        if let Some(b) = self.bars() {
            let look = SideLook {
                tool: self.tool,
                hover: self.side_hover,
                color: self.color,
                size: self.size,
                shape: self.shape_tool,
            };
            p.side_toolbar(b.side, &b.sl, b.dpi, &look, self.gdiplus);
        }
        if let Some((fly, dpi)) = self.size_rect() {
            p.size_flyout(fly, dpi, self.size, self.size_hover, self.gdiplus);
        }
        if let Some((fly, dpi)) = self.shapes_rect() {
            p.shapes_flyout(fly, dpi, self.tool, self.shape_hover, self.gdiplus);
        }
        if let Some((pal, dpi)) = self.palette_rect() {
            let l = picker_layout(dpi, self.expanded);
            let font = self.font(dpi);
            if self.expanded {
                self.ensure_picker_images(&l);
            }
            let spectrum = self
                .spectra
                .iter()
                .find(|(k, _)| *k == (l.spectrum.w, l.spectrum.h))
                .map(|(_, v)| v.as_slice());
            let lum = self.lum_img.as_ref().map(|(_, v)| v.as_slice());
            let look = PickerLook {
                expanded: self.expanded,
                current: self.color,
                pick: self.pick,
                target: self.target,
                field: self.field.as_ref(),
                custom: &self.custom,
                pressed: self.picker_pressed,
                spectrum,
                lum,
            };
            p.picker(pal, &l, dpi, font, &look);
        }
        if let Some((rect, mut text, dpi)) = self.tooltip() {
            let font = self.font(dpi);
            p.bubble(
                rect,
                scaled(4, dpi),
                &mut text,
                font,
                OVERLAY_TIP,
                OVERLAY_BORDER,
            );
        }
        self.draw_hint(p, area);
    }

    /// Viên nhắc (chỉ trên lớp phủ, không bao giờ vào ảnh xuất ra).
    fn draw_hint(&mut self, p: &Painter, area: Rect) {
        let Some((rect, mut text, dpi)) = self.hint() else {
            return;
        };
        if intersect(inflate(rect, 1), area).is_none() {
            return;
        }
        let font = self.font(dpi);
        p.bubble(
            rect,
            scaled(snap::HINT_RADIUS_DIP, dpi),
            &mut text,
            font,
            OVERLAY_LABEL,
            OVERLAY_BORDER,
        );
    }
}

/// Trạng thái hiển thị của thanh công cụ vẽ.
struct SideLook {
    tool: Option<Tool>,
    hover: Option<SideItem>,
    color: Rgb,
    size: SizeLevel,
    /// Hình nhớ (biểu tượng của nút Hình).
    shape: Tool,
}

impl SideLook {
    fn selected(&self, item: SideItem) -> bool {
        match item {
            SideItem::Tool(t) => Some(t) == self.tool,
            SideItem::Shapes => self.tool.is_some_and(is_shape),
            SideItem::Size | SideItem::Color => false,
        }
    }
}

/// Trạng thái hiển thị của thanh ngang.
struct BarLook {
    hover: Option<BarItem>,
    can_undo: bool,
    can_redo: bool,
    /// Có phiên chụp từng bước (thêm ô Thêm bước).
    steps: bool,
}

impl BarLook {
    fn enabled(&self, item: BarItem) -> bool {
        match item {
            BarItem::Undo => self.can_undo,
            BarItem::Redo => self.can_redo,
            BarItem::Act(_) => true,
        }
    }
}

/// Trạng thái hiển thị của bảng chọn màu.
struct PickerLook<'a> {
    expanded: bool,
    /// Màu vẽ hiện tại.
    current: Rgb,
    pick: PickColor,
    target: Option<usize>,
    field: Option<&'a FieldEdit>,
    custom: &'a CustomColors,
    pressed: Option<PickerHit>,
    /// Ảnh ô sắc độ và thanh độ sáng đã dựng (cùng cỡ với ô trong bố cục).
    spectrum: Option<&'a [u8]>,
    lum: Option<&'a [u8]>,
}

/// Vẽ vào DC bộ nhớ có gốc tại (`ox`, `oy`) của tọa độ client.
struct Painter {
    dc: HDC,
    ox: i32,
    oy: i32,
}

impl Painter {
    fn rc(&self, r: Rect) -> RECT {
        to_rect(Rect {
            x: r.x - self.ox,
            y: r.y - self.oy,
            ..r
        })
    }

    /// Chép vùng `r` (tọa độ ảnh) của ảnh BGRA `img_w`×`img_h` từ `bits`; sai biên thì bỏ qua.
    fn image(&self, bits: &[u8], img_w: i32, img_h: i32, r: Rect) {
        if !blit_in_bounds(r, img_w, img_h, bits.len()) {
            return;
        }
        // Đưa con trỏ tới hàng đầu của vùng và khai báo DIB đúng `r.h` hàng, nên nguồn
        // luôn là trọn chiều cao DIB: không phụ thuộc cách hiểu ySrc của DIB từ trên xuống.
        let offset = r.y as usize * img_w as usize * 4;
        let Some(rows) = bits.get(offset..) else {
            return;
        };
        let bmi = dib_info(img_w, r.h);
        // SAFETY: blit_in_bounds bảo đảm `rows` (từ hàng r.y) chứa đủ r.h hàng rộng img_w
        // và cột r.x..r.right() nằm trong mỗi hàng; bộ đệm sống suốt lời gọi.
        unsafe {
            StretchDIBits(
                self.dc,
                r.x - self.ox,
                r.y - self.oy,
                r.w,
                r.h,
                r.x,
                0,
                r.w,
                r.h,
                Some(rows.as_ptr() as *const c_void),
                &bmi,
                DIB_RGB_COLORS,
                SRCCOPY,
            );
        }
    }

    fn border(&self, r: Rect) {
        let rc = self.rc(r);
        // SAFETY: bút tạo và xóa trong khối; đối tượng cũ được chọn lại trước khi xóa.
        unsafe {
            let pen = CreatePen(PS_SOLID, 1, cr(ACCENT));
            let outer = CreatePen(PS_SOLID, 1, cr(OVERLAY_LABEL));
            let old_pen = SelectObject(self.dc, outer.into());
            let old_brush = SelectObject(self.dc, GetStockObject(NULL_BRUSH));
            SetBkMode(self.dc, OPAQUE);
            // Nét tối ngoài 1px để viền xanh nổi trên nền xanh.
            let _ = Rectangle(
                self.dc,
                rc.left - 2,
                rc.top - 2,
                rc.right + 2,
                rc.bottom + 2,
            );
            SelectObject(self.dc, pen.into());
            let _ = Rectangle(
                self.dc,
                rc.left - 1,
                rc.top - 1,
                rc.right + 1,
                rc.bottom + 1,
            );
            SelectObject(self.dc, old_brush);
            SelectObject(self.dc, old_pen);
            let _ = DeleteObject(pen.into());
            let _ = DeleteObject(outer.into());
        }
    }

    /// Khung nét đứt mảnh `ACCENT` (1 px; nét `TEXT_DASH`, khe `TEXT_DASH_GAP` DIP), khe trong suốt.
    /// Vẽ từng nét bằng ô tô màu để độ dài nét đúng pixel (bút kiểu GDI tính theo "style unit").
    fn dashed_frame(&self, r: Rect, dpi: u32) {
        let (dash, gap) = (scaled(TEXT_DASH, dpi), scaled(TEXT_DASH_GAP, dpi));
        let mut segs = Vec::new();
        let mut x = r.x;
        while x < r.right() {
            let w = dash.min(r.right() - x);
            segs.push(Rect { x, y: r.y, w, h: 1 });
            segs.push(Rect {
                x,
                y: r.bottom() - 1,
                w,
                h: 1,
            });
            x += dash + gap;
        }
        let mut y = r.y;
        while y < r.bottom() {
            let h = dash.min(r.bottom() - y);
            segs.push(Rect { x: r.x, y, w: 1, h });
            segs.push(Rect {
                x: r.right() - 1,
                y,
                w: 1,
                h,
            });
            y += dash + gap;
        }
        // SAFETY: chổi tạo và xóa trong khối, không được chọn vào DC.
        unsafe {
            let brush = CreateSolidBrush(cr(ACCENT));
            if brush.is_invalid() {
                return;
            }
            for seg in segs {
                FillRect(self.dc, &self.rc(seg), brush);
            }
            let _ = DeleteObject(brush.into());
        }
    }

    fn handle(&self, x: i32, y: i32, size: i32) {
        let (x, y) = (x - self.ox, y - self.oy);
        let half = size / 2;
        // SAFETY: như `border`.
        unsafe {
            let pen = CreatePen(PS_SOLID, 1, cr(ACCENT));
            let brush = CreateSolidBrush(cr(WHITE));
            let old_pen = SelectObject(self.dc, pen.into());
            let old_brush = SelectObject(self.dc, brush.into());
            let _ = Rectangle(
                self.dc,
                x - half,
                y - half,
                x - half + size,
                y - half + size,
            );
            SelectObject(self.dc, old_brush);
            SelectObject(self.dc, old_pen);
            let _ = DeleteObject(brush.into());
            let _ = DeleteObject(pen.into());
        }
    }

    fn round_rect(&self, r: Rect, radius: i32, fill: Rgb, edge: Rgb) {
        let rc = self.rc(r);
        // SAFETY: như `border`.
        unsafe {
            let pen = CreatePen(PS_SOLID, 1, cr(edge));
            let brush = CreateSolidBrush(cr(fill));
            let old_pen = SelectObject(self.dc, pen.into());
            let old_brush = SelectObject(self.dc, brush.into());
            let _ = RoundRect(
                self.dc,
                rc.left,
                rc.top,
                rc.right,
                rc.bottom,
                2 * radius,
                2 * radius,
            );
            SelectObject(self.dc, old_brush);
            SelectObject(self.dc, old_pen);
            let _ = DeleteObject(brush.into());
            let _ = DeleteObject(pen.into());
        }
    }

    fn fill(&self, r: Rect, color: Rgb) {
        let rc = self.rc(r);
        // SAFETY: chổi tạo và xóa trong khối, không được chọn vào DC.
        unsafe {
            let brush = CreateSolidBrush(cr(color));
            FillRect(self.dc, &rc, brush);
            let _ = DeleteObject(brush.into());
        }
    }

    /// Ô chữ sáng trên nền tối bo góc (nhãn kích thước, chú thích) với nền `fill` và viền `edge`.
    fn bubble(&self, r: Rect, radius: i32, text: &mut [u16], font: HFONT, fill: Rgb, edge: Rgb) {
        self.round_rect(r, radius, fill, edge);
        let mut rc = self.rc(r);
        // SAFETY: font còn sống (State giữ); đối tượng cũ được chọn lại.
        unsafe {
            let old = SelectObject(self.dc, font.into());
            SetBkMode(self.dc, TRANSPARENT);
            SetTextColor(self.dc, cr(TEXT));
            DrawTextW(
                self.dc,
                text,
                &mut rc,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );
            SelectObject(self.dc, old);
        }
    }

    /// Thanh công cụ vẽ (dọc) đặt tại `bar` theo bố cục `l`: các mục, vạch ngăn, ô màu hiện tại,
    /// biểu tượng.
    fn side_toolbar(&self, bar: Rect, l: &SideLayout, dpi: u32, look: &SideLook, gdiplus: bool) {
        self.round_rect(bar, scaled(6, dpi), SURFACE, OVERLAY_BORDER);
        let mark = scaled(MARK, dpi);
        let mark_inset = scaled(6, dpi);
        let items = || {
            SIDE_ITEMS
                .iter()
                .enumerate()
                .filter_map(|(i, item)| side_item_rect(bar, l, i).map(|r| (*item, r)))
        };
        for (item, r) in items() {
            let selected = look.selected(item);
            if selected || look.hover == Some(item) {
                self.fill(inflate(r, -1), OVERLAY_HOVER);
            }
            if selected {
                self.fill(
                    Rect {
                        x: r.x + 1,
                        y: r.y + mark_inset,
                        w: mark,
                        h: r.h - 2 * mark_inset,
                    },
                    ACCENT,
                );
            }
        }
        // Vạch ngăn 1 px giữa ô vạch ngăn, có lề hai bên.
        let margin = scaled(GAP, dpi);
        for s in &l.seps {
            self.fill(
                Rect {
                    x: bar.x + s.x + margin,
                    y: bar.y + s.y + s.h / 2,
                    w: s.w - 2 * margin,
                    h: 1,
                },
                OVERLAY_BORDER,
            );
        }
        // Nút Màu: ô vuông tô màu hiện tại, viền OVERLAY_BORDER.
        if let Some(btn) = side_item_rect(bar, l, side_index(SideItem::Color)) {
            let sw = scaled(SWATCH, dpi);
            self.round_rect(
                Rect {
                    x: btn.x + (btn.w - sw) / 2,
                    y: btn.y + (btn.h - sw) / 2,
                    w: sw,
                    h: sw,
                },
                scaled(2, dpi),
                look.color,
                OVERLAY_BORDER,
            );
        }
        if !gdiplus {
            return;
        }
        let Some(g) = Gp::new(self.dc) else { return };
        let icon = scaled(ICON, dpi) as f32;
        for (item, r) in items() {
            if item == SideItem::Color {
                continue;
            }
            let cx = (r.x - self.ox) as f32 + r.w as f32 * 0.5;
            let cy = (r.y - self.oy) as f32 + r.h as f32 * 0.5;
            let color = if look.selected(item) || look.hover == Some(item) {
                ICON_HOVER_ARGB
            } else {
                ICON_ARGB
            };
            match item {
                SideItem::Size => g.dot(cx, cy, size_dot(look.size) * icon / 24.0, color),
                SideItem::Tool(t) => g.side_icon(Glyph::Tool(t), cx, cy, icon, color),
                SideItem::Shapes => {
                    g.side_icon(Glyph::Tool(look.shape), cx, cy, icon, color);
                    // Tam giác đặc nhỏ ở góc dưới phải ô: nút có bảng bật ra.
                    let k = icon / 24.0;
                    let inset = scaled(4, dpi) as f32;
                    let (x1, y1) = (
                        (r.right() - self.ox) as f32 - inset,
                        (r.bottom() - self.oy) as f32 - inset,
                    );
                    let t = 5.0 * k;
                    g.fill_triangle([(x1, y1 - t), (x1, y1), (x1 - t, y1)], color);
                }
                SideItem::Color => {}
            }
        }
    }

    /// Bảng Cỡ `fly`: bốn ô, mỗi ô một chấm lớn dần; ô mức `current` viền `ACCENT`, ô `hover` nền
    /// `OVERLAY_HOVER`.
    fn size_flyout(
        &self,
        fly: Rect,
        dpi: u32,
        current: SizeLevel,
        hover: Option<SizeLevel>,
        gdiplus: bool,
    ) {
        self.round_rect(fly, scaled(6, dpi), SURFACE, OVERLAY_BORDER);
        let t = scaled(MARK, dpi);
        for (i, level) in SizeLevel::ALL.iter().enumerate() {
            let r = size_cell_rect(fly, dpi, i);
            if hover == Some(*level) {
                self.fill(inflate(r, -1), OVERLAY_HOVER);
            }
            if *level == current {
                for edge in [
                    Rect { h: t, ..r },
                    Rect {
                        y: r.bottom() - t,
                        h: t,
                        ..r
                    },
                    Rect { w: t, ..r },
                    Rect {
                        x: r.right() - t,
                        w: t,
                        ..r
                    },
                ] {
                    self.fill(edge, ACCENT);
                }
            }
        }
        if !gdiplus {
            return;
        }
        let Some(g) = Gp::new(self.dc) else { return };
        let icon = scaled(ICON, dpi) as f32;
        for (i, level) in SizeLevel::ALL.iter().enumerate() {
            let r = size_cell_rect(fly, dpi, i);
            let cx = (r.x - self.ox) as f32 + r.w as f32 * 0.5;
            let cy = (r.y - self.oy) as f32 + r.h as f32 * 0.5;
            let color = if hover == Some(*level) || *level == current {
                ICON_HOVER_ARGB
            } else {
                ICON_ARGB
            };
            g.dot(cx, cy, size_dot(*level) * icon / 24.0, color);
        }
    }

    /// Bảng Hình `fly`: bốn ô theo `SHAPE_TOOLS`, mỗi ô biểu tượng công cụ; ô của công cụ `current`
    /// viền `ACCENT`, ô `hover` nền `OVERLAY_HOVER`.
    fn shapes_flyout(
        &self,
        fly: Rect,
        dpi: u32,
        current: Option<Tool>,
        hover: Option<Tool>,
        gdiplus: bool,
    ) {
        self.round_rect(fly, scaled(6, dpi), SURFACE, OVERLAY_BORDER);
        let t = scaled(MARK, dpi);
        for (i, tool) in SHAPE_TOOLS.iter().enumerate() {
            let r = shape_cell_rect(fly, dpi, i);
            if hover == Some(*tool) {
                self.fill(inflate(r, -1), OVERLAY_HOVER);
            }
            if current == Some(*tool) {
                for edge in [
                    Rect { h: t, ..r },
                    Rect {
                        y: r.bottom() - t,
                        h: t,
                        ..r
                    },
                    Rect { w: t, ..r },
                    Rect {
                        x: r.right() - t,
                        w: t,
                        ..r
                    },
                ] {
                    self.fill(edge, ACCENT);
                }
            }
        }
        if !gdiplus {
            return;
        }
        let Some(g) = Gp::new(self.dc) else { return };
        let icon = scaled(ICON, dpi) as f32;
        for (i, tool) in SHAPE_TOOLS.iter().enumerate() {
            let r = shape_cell_rect(fly, dpi, i);
            let cx = (r.x - self.ox) as f32 + r.w as f32 * 0.5;
            let cy = (r.y - self.oy) as f32 + r.h as f32 * 0.5;
            let color = if hover == Some(*tool) || current == Some(*tool) {
                ICON_HOVER_ARGB
            } else {
                ICON_ARGB
            };
            g.side_icon(Glyph::Tool(*tool), cx, cy, icon, color);
        }
    }

    /// Chữ `text` trong `r` bằng phông `font`, màu `color`, nền trong suốt, một dòng.
    fn text(&self, r: Rect, text: &str, font: HFONT, color: Rgb, flags: DRAW_TEXT_FORMAT) {
        let mut buf = wide(text);
        if buf.is_empty() {
            return;
        }
        let mut rc = self.rc(r);
        // SAFETY: font còn sống (State giữ); đối tượng cũ được chọn lại; bộ đệm sống suốt lời gọi.
        unsafe {
            let old = SelectObject(self.dc, font.into());
            SetBkMode(self.dc, TRANSPARENT);
            SetTextColor(self.dc, cr(color));
            DrawTextW(
                self.dc,
                &mut buf,
                &mut rc,
                flags | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
            );
            SelectObject(self.dc, old);
        }
    }

    /// Chép trọn ảnh BGRA `w`×`h` (`bits`) vào `dest` (cùng cỡ); ảnh thiếu byte thì bỏ qua.
    fn bitmap(&self, bits: &[u8], w: i32, h: i32, dest: Rect) {
        let need = i64::from(w) * i64::from(h) * 4;
        if w <= 0 || h <= 0 || i64::try_from(bits.len()).unwrap_or(i64::MAX) < need {
            return;
        }
        let bmi = dib_info(w, h);
        // SAFETY: `bits` chứa đủ w×h điểm 32 bit (đã kiểm ở trên) và sống suốt lời gọi.
        unsafe {
            StretchDIBits(
                self.dc,
                dest.x - self.ox,
                dest.y - self.oy,
                w,
                h,
                0,
                0,
                w,
                h,
                Some(bits.as_ptr() as *const c_void),
                &bmi,
                DIB_RGB_COLORS,
                SRCCOPY,
            );
        }
    }

    /// Viền 1 px `color` ngay ngoài `r`.
    fn frame_outside(&self, r: Rect, color: Rgb) {
        let o = inflate(r, 1);
        for side in [
            Rect { h: 1, ..o },
            Rect {
                y: o.bottom() - 1,
                h: 1,
                ..o
            },
            Rect { w: 1, ..o },
            Rect {
                x: o.right() - 1,
                w: 1,
                ..o
            },
        ] {
            self.fill(side, color);
        }
    }

    /// Viền TEXT dày `ring` quanh ô `r`, cách ô 1 px nền để nổi cả trên ô trắng.
    fn ring(&self, r: Rect, ring: i32) {
        self.fill(inflate(r, ring + 1), TEXT);
        self.fill(inflate(r, 1), SURFACE);
    }

    /// Nút của bảng màu: nút phụ (`BUTTON`) hoặc nút chính (`ACCENT_BUTTON`, chữ trắng).
    #[allow(clippy::too_many_arguments)]
    fn picker_button(
        &self,
        r: Rect,
        text: &str,
        font: HFONT,
        dpi: u32,
        primary: bool,
        enabled: bool,
        pressed: bool,
    ) {
        let (fill, edge, fg) = match (primary, pressed) {
            (true, false) => (ACCENT_BUTTON, ACCENT_BUTTON, WHITE),
            (true, true) => (ACCENT_BUTTON_PRESSED, ACCENT_BUTTON_PRESSED, WHITE),
            (false, false) => (BUTTON_FILL, BUTTON_BORDER, TEXT),
            (false, true) => (BUTTON_PRESSED, BUTTON_BORDER, TEXT),
        };
        let fg = if enabled { fg } else { TEXT_DISABLED };
        self.round_rect(r, scaled(4, dpi), fill, edge);
        self.text(r, text, font, fg, DT_CENTER);
    }

    /// Bảng chọn màu ở góc trên trái `pal` theo bố cục `l`: màu cơ bản, màu tự chọn, nút
    /// "Định nghĩa…"; dạng đủ thêm ô sắc độ, thanh độ sáng, ô xem màu, sáu ô số và các nút.
    fn picker(&self, pal: Rect, l: &PickerLayout, dpi: u32, font: HFONT, look: &PickerLook) {
        self.round_rect(pal, scaled(6, dpi), SURFACE, OVERLAY_BORDER);
        let ring = scaled(MARK, dpi);
        self.text(
            at(pal, l.basic_label),
            lang::tr(Msg::PickerBasic),
            font,
            TEXT,
            DT_LEFT,
        );
        self.text(
            at(pal, l.custom_label),
            lang::tr(Msg::PickerCustom),
            font,
            TEXT,
            DT_LEFT,
        );
        // Ô đang chọn: dạng gọn là màu vẽ, dạng đủ là màu hiện tại của phần tự chọn.
        let shown = if look.expanded {
            look.pick.rgb
        } else {
            look.current
        };
        for (r, c) in l.basic.iter().zip(PALETTE.iter()) {
            let r = at(pal, *r);
            if *c == shown {
                self.ring(r, ring);
            }
            self.round_rect(r, 0, *c, OVERLAY_BORDER);
        }
        for (i, (r, slot)) in l.custom.iter().zip(look.custom.slots.iter()).enumerate() {
            let r = at(pal, *r);
            let marked = if look.expanded {
                look.target == Some(i)
            } else {
                *slot == Some(shown)
            };
            if marked {
                self.ring(r, ring);
            }
            self.round_rect(r, 0, slot.unwrap_or(INPUT), OVERLAY_BORDER);
        }
        self.picker_button(
            at(pal, l.define_btn),
            lang::tr(Msg::PickerDefine),
            font,
            dpi,
            false,
            !look.expanded,
            !look.expanded && look.pressed == Some(PickerHit::Define),
        );
        if !look.expanded {
            return;
        }
        let spectrum = at(pal, l.spectrum);
        if let Some(bits) = look.spectrum {
            self.bitmap(bits, spectrum.w, spectrum.h, spectrum);
        }
        self.frame_outside(spectrum, OVERLAY_BORDER);
        // Dấu chữ thập ở (Hue, Sat) hiện tại, cắt theo ô sắc độ.
        let (cx, cy) = spectrum_point(spectrum, look.pick.hls.h, look.pick.hls.s);
        let (arm, gap, t) = (
            scaled(CROSS_ARM, dpi),
            scaled(CROSS_GAP, dpi),
            scaled(CROSS_THICK, dpi),
        );
        let half = t / 2;
        for arm_rect in [
            Rect {
                x: cx - arm,
                y: cy - half,
                w: arm - gap,
                h: t,
            },
            Rect {
                x: cx + gap + 1,
                y: cy - half,
                w: arm - gap,
                h: t,
            },
            Rect {
                x: cx - half,
                y: cy - arm,
                w: t,
                h: arm - gap,
            },
            Rect {
                x: cx - half,
                y: cy + gap + 1,
                w: t,
                h: arm - gap,
            },
        ] {
            if let Some(r) = intersect(arm_rect, spectrum) {
                self.fill(r, OVERLAY_LABEL);
            }
        }
        let bar = at(pal, l.lum_bar);
        if let Some(bits) = look.lum {
            self.bitmap(bits, bar.w, bar.h, bar);
        }
        self.frame_outside(bar, OVERLAY_BORDER);
        // Tam giác chỉ Lum hiện tại, mũi nhọn quay vào thanh: cột thứ `i` cao 2·i·k + 1.
        let ly = lum_y(bar, look.pick.hls.l);
        let aw = scaled(LUM_ARROW, dpi);
        let ax = pal.x + l.lum_arrow_x + scaled(1, dpi);
        for i in 0..aw {
            let half_h = i * 3 / 4;
            self.fill(
                Rect {
                    x: ax + i,
                    y: ly - half_h,
                    w: 1,
                    h: 2 * half_h + 1,
                },
                TEXT,
            );
        }
        self.round_rect(at(pal, l.preview), 0, look.pick.rgb, OVERLAY_BORDER);
        const LABELS: [Msg; FIELD_COUNT] = [
            Msg::PickerHue,
            Msg::PickerSat,
            Msg::PickerLum,
            Msg::PickerRed,
            Msg::PickerGreen,
            Msg::PickerBlue,
        ];
        let pad = scaled(4, dpi);
        for (i, ((label, bx), msg)) in l.fields.iter().zip(LABELS).enumerate() {
            let label = at(pal, *label);
            self.text(
                Rect {
                    w: label.w - pad,
                    ..label
                },
                lang::tr(msg),
                font,
                TEXT_MUTED,
                DT_RIGHT,
            );
            let bx = at(pal, *bx);
            let editing = look.field.filter(|f| f.idx == i);
            let edge = if editing.is_some() {
                ACCENT
            } else {
                INPUT_BORDER
            };
            self.round_rect(bx, scaled(2, dpi), INPUT, edge);
            let value = match editing {
                Some(f) => f.text.clone(),
                None => field_value(look.pick, i).to_string(),
            };
            self.text(bx, &value, font, TEXT, DT_CENTER);
        }
        let pressed = |h| look.pressed == Some(h);
        self.picker_button(
            at(pal, l.add_btn),
            lang::tr(Msg::PickerAdd),
            font,
            dpi,
            false,
            true,
            pressed(PickerHit::Add),
        );
        self.picker_button(
            at(pal, l.ok_btn),
            i18n::OK_LABEL,
            font,
            dpi,
            true,
            true,
            pressed(PickerHit::Ok),
        );
        self.picker_button(
            at(pal, l.cancel_btn),
            lang::tr(Msg::BtnCancel),
            font,
            dpi,
            false,
            true,
            pressed(PickerHit::Cancel),
        );
    }

    /// Thanh ngang `bar`: Hoàn tác, Làm lại (mờ khi không dùng được), vạch ngăn, năm nút hành
    /// động ngăn bằng vạch 1 px.
    fn toolbar(&self, bar: Rect, dpi: u32, look: &BarLook, gdiplus: bool) {
        self.round_rect(bar, scaled(6, dpi), SURFACE, OVERLAY_BORDER);
        let l = bar_layout_with(dpi, look.steps);
        let cells = || {
            l.cells.iter().map(|(item, r)| {
                (
                    *item,
                    Rect {
                        x: bar.x + r.x,
                        y: bar.y + r.y,
                        ..*r
                    },
                )
            })
        };
        let margin = scaled(GAP, dpi);
        let mut prev: Option<BarItem> = None;
        for (item, r) in cells() {
            match item {
                Some(it) => {
                    if look.hover == Some(it) && look.enabled(it) {
                        self.fill(inflate(r, -1), OVERLAY_HOVER);
                    }
                    // Vạch 1 px giữa hai nút hành động liền nhau (như thanh bốn nút cũ).
                    if matches!((prev, it), (Some(BarItem::Act(_)), BarItem::Act(_))) {
                        self.fill(
                            Rect {
                                x: r.x,
                                y: r.y + 1,
                                w: 1,
                                h: r.h - 2,
                            },
                            OVERLAY_BORDER,
                        );
                    }
                }
                // Vạch ngăn nhóm: 1 px giữa ô vạch ngăn, có lề trên dưới (như thanh dọc).
                None => self.fill(
                    Rect {
                        x: r.x + r.w / 2,
                        y: r.y + margin,
                        w: 1,
                        h: r.h - 2 * margin,
                    },
                    OVERLAY_BORDER,
                ),
            }
            prev = item;
        }
        if !gdiplus {
            return;
        }
        let Some(g) = Gp::new(self.dc) else { return };
        let icon = scaled(ICON, dpi) as f32;
        for (item, r) in cells() {
            let Some(it) = item else { continue };
            let cx = (r.x - self.ox) as f32 + r.w as f32 * 0.5;
            let cy = (r.y - self.oy) as f32 + r.h as f32 * 0.5;
            let color = if !look.enabled(it) {
                ICON_DISABLED_ARGB
            } else if look.hover == Some(it) {
                ICON_HOVER_ARGB
            } else {
                ICON_ARGB
            };
            match it {
                BarItem::Act(a) => g.icon(a, cx, cy, icon, color),
                BarItem::Undo => g.side_icon(Glyph::Undo, cx, cy, icon, color),
                BarItem::Redo => g.side_icon(Glyph::Redo, cx, cy, icon, color),
            }
        }
    }
}

/// Một điểm trong lưới 24×24 của biểu tượng.
type GridPt = (f32, f32);

/// Đinh ghim trong lưới 24×24: viền đầu và thân (đường khép kín) và mũi kim (hai điểm: gốc, mũi).
/// Dựng đứng (đầu hình thang trên thân thon, loe ra ở chân) rồi xoay 45° theo chiều kim đồng hồ
/// quanh tâm lưới và dời cho cân: đầu đinh lên trên phải, mũi kim chéo xuống dưới trái.
fn pin_glyph() -> ([GridPt; 6], [GridPt; 2]) {
    const UPRIGHT: [GridPt; 6] = [
        (9.0, 3.0),
        (15.0, 3.0),
        (14.0, 9.0),
        (17.0, 13.0),
        (7.0, 13.0),
        (10.0, 9.0),
    ];
    const NEEDLE: [GridPt; 2] = [(12.0, 13.0), (12.0, 21.0)];
    let s = std::f32::consts::FRAC_1_SQRT_2;
    // Xoay (trục y hướng xuống) rồi dời (-1, +1) để khung bao cân giữa lưới.
    let turn = |(x, y): GridPt| {
        let (dx, dy) = (x - 12.0, y - 12.0);
        (12.0 + (dx - dy) * s - 1.0, 12.0 + (dx + dy) * s + 1.0)
    };
    (UPRIGHT.map(turn), NEEDLE.map(turn))
}

/// Nhận dạng chữ trong lưới 24×24: bốn góc khung (mỗi góc một đường gãy: đầu, góc, đầu) và chữ "T"
/// (gạch ngang, thân dọc) ở giữa khung.
fn ocr_glyph() -> ([[GridPt; 3]; 4], [GridPt; 2], [GridPt; 2]) {
    const CORNERS: [[GridPt; 3]; 4] = [
        [(4.0, 9.0), (4.0, 4.0), (9.0, 4.0)],
        [(15.0, 4.0), (20.0, 4.0), (20.0, 9.0)],
        [(20.0, 15.0), (20.0, 20.0), (15.0, 20.0)],
        [(9.0, 20.0), (4.0, 20.0), (4.0, 15.0)],
    ];
    const BAR: [GridPt; 2] = [(8.5, 8.5), (15.5, 8.5)];
    const STEM: [GridPt; 2] = [(12.0, 8.5), (12.0, 16.0)];
    (CORNERS, BAR, STEM)
}

/// Đường GDI+ đang dựng cho một biểu tượng: tọa độ trong lưới 24×24, gốc (`ox`, `oy`), hệ số `k`.
struct IconPath {
    path: *mut GpPath,
    ox: f32,
    oy: f32,
    k: f32,
}

impl IconPath {
    fn arc(&self, x: f32, y: f32, r: f32, start: f32, sweep: f32) {
        let k = self.k;
        // SAFETY: path hợp lệ suốt đời của IconPath (Gp::stroke_icon tạo và xóa).
        unsafe {
            GdipAddPathArc(
                self.path,
                self.ox + (x - r) * k,
                self.oy + (y - r) * k,
                2.0 * r * k,
                2.0 * r * k,
                start,
                sweep,
            );
        }
    }

    /// Elip trọn (hình mới) tâm (`x`, `y`), bán trục `rx`, `ry`.
    fn oval(&self, x: f32, y: f32, rx: f32, ry: f32) {
        let k = self.k;
        self.start();
        // SAFETY: như `arc`.
        unsafe {
            GdipAddPathArc(
                self.path,
                self.ox + (x - rx) * k,
                self.oy + (y - ry) * k,
                2.0 * rx * k,
                2.0 * ry * k,
                0.0,
                360.0,
            );
        }
        self.close();
    }

    fn line(&self, x1: f32, y1: f32, x2: f32, y2: f32) {
        let (ox, oy, k) = (self.ox, self.oy, self.k);
        // SAFETY: như `arc`.
        unsafe {
            GdipAddPathLine(
                self.path,
                ox + x1 * k,
                oy + y1 * k,
                ox + x2 * k,
                oy + y2 * k,
            );
        }
    }

    fn start(&self) {
        // SAFETY: như `arc`.
        unsafe {
            GdipStartPathFigure(self.path);
        }
    }

    fn close(&self) {
        // SAFETY: như `arc`.
        unsafe {
            GdipClosePathFigure(self.path);
        }
    }

    /// Đường gấp khúc mở, bắt đầu hình mới.
    fn poly(&self, pts: &[(f32, f32)]) {
        self.start();
        for s in pts.windows(2) {
            let ((x1, y1), (x2, y2)) = (s[0], s[1]);
            self.line(x1, y1, x2, y2);
        }
    }
}

impl Gp {
    /// Chấm tròn đặc màu `argb`, đường kính `d` pixel, tâm (`cx`, `cy`).
    fn dot(&self, cx: f32, cy: f32, d: f32, argb: u32) {
        let mut brush: *mut GpSolidFill = std::ptr::null_mut();
        // SAFETY: chổi được tạo, dùng và xóa trong khối; kiểm null trước khi dùng.
        unsafe {
            if GdipCreateSolidFill(argb, &mut brush).0 != 0 || brush.is_null() {
                return;
            }
            let r = d / 2.0;
            GdipFillEllipse(self.raw(), brush.cast::<GpBrush>(), cx - r, cy - r, d, d);
            GdipDeleteBrush(brush.cast::<GpBrush>());
        }
    }

    /// Tam giác đặc màu `argb` qua ba đỉnh (pixel).
    fn fill_triangle(&self, pts: [(f32, f32); 3], argb: u32) {
        let pts = pts.map(|(x, y)| PointF { X: x, Y: y });
        let mut brush: *mut GpSolidFill = std::ptr::null_mut();
        // SAFETY: chổi được tạo, dùng và xóa trong khối; kiểm null trước khi dùng; `pts` có đúng
        // 3 điểm, sống suốt lời gọi.
        unsafe {
            if GdipCreateSolidFill(argb, &mut brush).0 != 0 || brush.is_null() {
                return;
            }
            GdipFillPolygon(
                self.raw(),
                brush.cast::<GpBrush>(),
                pts.as_ptr(),
                3,
                FillModeAlternate,
            );
            GdipDeleteBrush(brush.cast::<GpBrush>());
        }
    }

    /// Vẽ nét mảnh (1,8 trong lưới 24×24, đầu và chỗ nối tròn) của đường do `build` dựng,
    /// cỡ `size` pixel, tâm (`cx`, `cy`).
    fn stroke_icon(&self, cx: f32, cy: f32, size: f32, argb: u32, build: impl FnOnce(&IconPath)) {
        let k = size / 24.0;
        let mut pen = std::ptr::null_mut();
        let mut path: *mut GpPath = std::ptr::null_mut();
        // SAFETY: bút và đường được tạo, dùng và xóa trong khối; kiểm null trước khi dùng.
        unsafe {
            if GdipCreatePen1(argb, 1.8 * k, UnitPixel, &mut pen).0 != 0 || pen.is_null() {
                return;
            }
            GdipSetPenLineCap197819(pen, LineCapRound, LineCapRound, DashCapFlat);
            GdipSetPenLineJoin(pen, LineJoinRound);
            if GdipCreatePath(FillModeAlternate, &mut path).0 != 0 || path.is_null() {
                GdipDeletePen(pen);
                return;
            }
        }
        build(&IconPath {
            path,
            ox: cx - size / 2.0,
            oy: cy - size / 2.0,
            k,
        });
        // SAFETY: như trên; xóa đúng một lần.
        unsafe {
            GdipDrawPath(self.raw(), pen, path);
            GdipDeletePath(path);
            GdipDeletePen(pen);
        }
    }

    /// Vẽ biểu tượng trong lưới 24×24 (theo hình mẫu), cỡ `size` pixel, tâm (`cx`, `cy`).
    fn icon(&self, a: Action, cx: f32, cy: f32, size: f32, argb: u32) {
        self.stroke_icon(cx, cy, size, argb, |p| match a {
            Action::Upload => {
                // Đám mây: ba cung tròn (đổi từ đường SVG của hình mẫu), mở ở đáy.
                p.arc(6.989, 14.0, 4.0, 89.84, 173.14);
                p.arc(12.328, 11.4565, 6.0, 193.75, 147.22);
                p.arc(17.75, 13.75, 4.2574, 273.37, 180.0);
                // Mũi tên lên.
                p.poly(&[(12.0, 20.0), (12.0, 12.0)]);
                p.poly(&[(9.0, 15.0), (12.0, 12.0), (15.0, 15.0)]);
            }
            Action::Save => {
                p.poly(&[
                    (5.0, 3.0),
                    (16.0, 3.0),
                    (19.0, 6.0),
                    (19.0, 21.0),
                    (5.0, 21.0),
                ]);
                p.close();
                p.poly(&[(8.0, 3.0), (8.0, 9.0), (15.0, 9.0), (15.0, 3.0)]);
                p.poly(&[(8.0, 21.0), (8.0, 14.0), (16.0, 14.0), (16.0, 21.0)]);
            }
            Action::Copy => {
                // Ô trước: hình vuông bo góc (9,9)-(20,20), bán kính 2.
                p.start();
                p.arc(11.0, 11.0, 2.0, 180.0, 90.0);
                p.arc(18.0, 11.0, 2.0, 270.0, 90.0);
                p.arc(18.0, 18.0, 2.0, 0.0, 90.0);
                p.arc(11.0, 18.0, 2.0, 90.0, 90.0);
                p.close();
                // Ô sau: chỉ phần lộ ra (cạnh trái và cạnh trên).
                p.poly(&[(5.0, 15.0), (5.0, 5.0)]);
                p.arc(7.0, 5.0, 2.0, 180.0, 90.0);
                p.line(7.0, 3.0, 15.0, 3.0);
            }
            Action::Pin => {
                let (outline, needle) = pin_glyph();
                p.poly(&outline);
                p.close();
                p.poly(&needle);
            }
            Action::Ocr => {
                let (corners, bar, stem) = ocr_glyph();
                for c in &corners {
                    p.poly(c);
                }
                p.poly(&bar);
                p.poly(&stem);
            }
            Action::AddStep => {
                // Dấu cộng trong ô vuông bo góc (4,4)-(20,20), bán kính 2.
                p.start();
                p.arc(6.0, 6.0, 2.0, 180.0, 90.0);
                p.arc(18.0, 6.0, 2.0, 270.0, 90.0);
                p.arc(18.0, 18.0, 2.0, 0.0, 90.0);
                p.arc(6.0, 18.0, 2.0, 90.0, 90.0);
                p.close();
                p.poly(&[(12.0, 8.0), (12.0, 16.0)]);
                p.poly(&[(8.0, 12.0), (16.0, 12.0)]);
            }
            Action::Cancel => {
                p.poly(&[(6.0, 6.0), (18.0, 18.0)]);
                p.poly(&[(18.0, 6.0), (6.0, 18.0)]);
            }
        });
    }

    /// Biểu tượng công cụ vẽ, Hoàn tác, Làm lại; cùng lưới và nét với `icon` (tự vẽ, hình học đơn
    /// giản).
    fn side_icon(&self, glyph: Glyph, cx: f32, cy: f32, size: f32, argb: u32) {
        self.stroke_icon(cx, cy, size, argb, |p| match glyph {
            Glyph::Tool(Tool::Pen) => {
                // Bút chì chéo: thân, đầu nhọn ở dưới trái, vạch nối đầu bút.
                p.poly(&[
                    (4.0, 20.0),
                    (5.0, 15.5),
                    (15.5, 5.0),
                    (19.0, 8.5),
                    (8.5, 19.0),
                ]);
                p.close();
                p.poly(&[(13.0, 7.5), (16.5, 11.0)]);
            }
            Glyph::Tool(Tool::Line) => p.poly(&[(5.0, 19.0), (19.0, 5.0)]),
            Glyph::Tool(Tool::Arrow) => {
                p.poly(&[(5.0, 19.0), (18.5, 5.5)]);
                p.poly(&[(10.0, 5.0), (19.0, 5.0), (19.0, 14.0)]);
            }
            Glyph::Tool(Tool::Rect) => {
                p.poly(&[(4.0, 6.0), (20.0, 6.0), (20.0, 18.0), (4.0, 18.0)]);
                p.close();
            }
            Glyph::Tool(Tool::Marker) => {
                // Bút dạ quang: thân chéo, đầu vát, vạch tô bên dưới.
                p.poly(&[(9.0, 13.0), (15.0, 7.0), (19.0, 11.0), (13.0, 17.0)]);
                p.close();
                p.poly(&[(9.0, 13.0), (6.0, 18.0), (8.0, 20.0), (13.0, 17.0)]);
                p.poly(&[(12.0, 21.0), (20.0, 21.0)]);
            }
            Glyph::Tool(Tool::Blur) => {
                // Khung như Hình chữ nhật, ba vạch chéo song song cắt trong khung.
                p.poly(&[(4.0, 6.0), (20.0, 6.0), (20.0, 18.0), (4.0, 18.0)]);
                p.close();
                p.poly(&[(4.0, 14.0), (12.0, 6.0)]);
                p.poly(&[(8.0, 18.0), (20.0, 6.0)]);
                p.poly(&[(16.0, 18.0), (20.0, 14.0)]);
            }
            Glyph::Tool(Tool::Text) => {
                p.poly(&[(5.0, 7.0), (5.0, 5.0), (19.0, 5.0), (19.0, 7.0)]);
                p.poly(&[(12.0, 5.0), (12.0, 19.0)]);
                p.poly(&[(9.0, 19.0), (15.0, 19.0)]);
            }
            Glyph::Undo => {
                // Mũi tên quay lại: đầu nhọn bên trái, thân vòng nửa tròn bên phải.
                p.poly(&[(8.0, 5.0), (4.0, 9.0), (8.0, 13.0)]);
                p.start();
                p.line(4.0, 9.0, 14.5, 9.0);
                p.arc(14.5, 14.5, 5.5, 270.0, 180.0);
                p.line(14.5, 20.0, 9.0, 20.0);
            }
            Glyph::Redo => {
                // Hoàn tác lật ngang: đầu nhọn bên phải, thân vòng nửa tròn bên trái.
                p.poly(&[(16.0, 5.0), (20.0, 9.0), (16.0, 13.0)]);
                p.start();
                p.line(20.0, 9.0, 9.5, 9.0);
                p.arc(9.5, 14.5, 5.5, 270.0, -180.0);
                p.line(9.5, 20.0, 15.0, 20.0);
            }
            Glyph::Tool(Tool::Select) => {
                // Mũi tên con trỏ: tam giác nghiêng, đuôi chéo xuống phải.
                p.poly(&[
                    (6.0, 3.5),
                    (6.0, 18.5),
                    (10.0, 14.5),
                    (12.8, 20.5),
                    (15.2, 19.4),
                    (12.4, 13.5),
                    (18.0, 13.5),
                ]);
                p.close();
            }
            Glyph::Tool(Tool::Ellipse) => p.oval(12.0, 12.0, 8.5, 6.0),
            Glyph::Tool(Tool::Step) => {
                // Vòng tròn với số "1" dựng bằng nét: mũ chéo, thân đứng, chân ngang.
                p.oval(12.0, 12.0, 8.5, 8.5);
                p.poly(&[(10.0, 9.5), (12.5, 7.5), (12.5, 16.5)]);
                p.poly(&[(10.0, 16.5), (15.0, 16.5)]);
            }
        });
    }
}

/// Lấy lại trạng thái khi `pick_region` kết thúc trên mọi đường đi (font, DC được giải phóng).
struct StateGuard;
impl Drop for StateGuard {
    fn drop(&mut self) {
        let s = OVERLAY.with(|o| o.try_borrow_mut().ok().and_then(|mut b| b.take()));
        drop(s);
    }
}

/// Chép màu tự chọn của lớp phủ ra cho bên gọi khi `pick_region` kết thúc.
struct CustomOut<'a>(&'a mut CustomColors);
impl Drop for CustomOut<'_> {
    fn drop(&mut self) {
        if let Some(c) = with_state(|s| s.custom) {
            *self.0 = c;
        }
    }
}

struct WindowGuard(HWND);
impl Drop for WindowGuard {
    fn drop(&mut self) {
        // SAFETY: cửa sổ lớp phủ của luồng này; hủy một lần, kể cả khi đã bị hủy trước (lỗi bỏ qua).
        unsafe {
            let _ = ReleaseCapture();
            let _ = DestroyWindow(self.0);
        }
    }
}

/// Trả chuột khi lớp phủ còn giữ mà không còn kéo gì (Esc hủy kéo hình, đóng bảng màu giữa lúc
/// kéo). Gọi ngoài lúc mượn trạng thái: `ReleaseCapture` gửi WM_CAPTURECHANGED đồng bộ.
fn release_if_idle(hwnd: HWND) {
    let dragging = with_state(|s| s.dragging()).unwrap_or(true);
    // SAFETY: chỉ hỏi cửa sổ đang giữ chuột của luồng này.
    if !dragging && unsafe { GetCapture() } == hwnd {
        // SAFETY: trả chuột do chính cửa sổ này giữ.
        unsafe {
            let _ = ReleaseCapture();
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        // Vẽ nền ở WM_PAINT; xóa nền ở đây sẽ gây nháy.
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            // SAFETY: cặp BeginPaint/EndPaint chuẩn trên cửa sổ của luồng này.
            let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
            if !hdc.is_invalid() {
                let rc = ps.rcPaint;
                let area = Rect {
                    x: rc.left,
                    y: rc.top,
                    w: rc.right - rc.left,
                    h: rc.bottom - rc.top,
                };
                with_state(|s| s.paint(hdc, area));
            }
            // SAFETY: kết thúc lượt vẽ đã mở ở trên.
            unsafe {
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        WM_SETCURSOR if (lp.0 & 0xFFFF) as u32 == HTCLIENT => {
            let mut pt = POINT::default();
            // SAFETY: chỉ đọc vị trí chuột.
            if unsafe { GetCursorPos(&mut pt) }.is_ok() {
                let mut wr = RECT::default();
                // SAFETY: đọc vị trí cửa sổ của ta để đổi sang tọa độ client.
                let _ = unsafe { GetWindowRect(hwnd, &mut wr) };
                let p = Point {
                    x: pt.x - wr.left,
                    y: pt.y - wr.top,
                };
                if let Some(id) = with_state(|s| s.cursor_at(p)) {
                    set_cursor(id);
                    return LRESULT(1);
                }
            }
            // SAFETY: chuyển nguyên tham số cho xử lý mặc định.
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
        WM_LBUTTONDOWN => {
            let p = point_from_lparam(lp);
            if let Some(fx) = with_state(|s| s.on_down(p)) {
                if fx.capture {
                    // SAFETY: giữ chuột cho cửa sổ của ta trong lúc kéo.
                    unsafe {
                        SetCapture(hwnd);
                    }
                }
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let p = point_from_lparam(lp);
            with_state(|s| s.on_move(p));
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let p = point_from_lparam(lp);
            if let Some((fx, dragging)) = with_state(|s| (s.on_up(p), s.dragging())) {
                // SAFETY: chỉ hỏi cửa sổ đang giữ chuột của luồng này.
                let has_capture = unsafe { GetCapture() } == hwnd;
                if should_release(fx.release, has_capture, dragging) {
                    // SAFETY: trả chuột sau khi kéo xong; WM_CAPTURECHANGED đến khi đã trả mượn.
                    unsafe {
                        let _ = ReleaseCapture();
                    }
                }
            }
            LRESULT(0)
        }
        WM_CAPTURECHANGED => {
            // Mất chuột giữa chừng (cửa sổ khác chiếm): chốt nét đang vẽ, kết thúc lần kéo.
            with_state(|s| s.lost_capture());
            LRESULT(0)
        }
        WM_KEYDOWN => {
            with_state(|s| s.on_key(wp.0 as u16, key_repeat(lp)));
            release_if_idle(hwnd);
            LRESULT(0)
        }
        WM_KEYUP => {
            if wp.0 == usize::from(VK_SHIFT.0) {
                with_state(|s| s.shift_changed(false));
            }
            // SAFETY: chuyển nguyên tham số cho xử lý mặc định.
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
        WM_CHAR => {
            if with_state(|s| s.on_char(wp.0 as u16)) == Some(true) {
                return LRESULT(0);
            }
            // SAFETY: chuyển nguyên tham số cho xử lý mặc định.
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
        WM_TIMER if wp.0 == CARET_TIMER_ID => {
            with_state(|s| s.blink());
            LRESULT(0)
        }
        WM_ACTIVATE => {
            // WA_INACTIVE = 0: mất tiêu điểm (Alt+Tab, Win...) thì hủy.
            if wp.0 & 0xFFFF == 0 {
                with_state(|s| {
                    if s.result.is_none() {
                        s.result = Some(None);
                    }
                });
            }
            // Xử lý mặc định đặt tiêu điểm bàn phím vào cửa sổ khi được kích hoạt.
            // SAFETY: chuyển nguyên tham số cho xử lý mặc định.
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
        // Lớp phủ đã phủ đúng pixel vật lý; không đổi kích thước theo DPI.
        WM_DPICHANGED => LRESULT(0),
        WM_HOTKEY if wp.0 == ESC_HOTKEY_ID as usize => {
            with_state(|s| s.escape());
            release_if_idle(hwnd);
            LRESULT(0)
        }
        WM_TIMER if wp.0 == FOCUS_TIMER_ID => {
            let tries = with_state(|s| {
                s.focus_retries += 1;
                s.focus_retries
            })
            .unwrap_or(MAX_FOCUS_RETRIES);
            // Gọi ngoài lúc mượn trạng thái: SetForegroundWindow gửi WM_ACTIVATE đồng bộ.
            if take_focus(hwnd) || tries >= MAX_FOCUS_RETRIES {
                // SAFETY: hủy bộ hẹn giờ của chính cửa sổ này.
                unsafe {
                    let _ = KillTimer(Some(hwnd), FOCUS_TIMER_ID);
                }
            }
            LRESULT(0)
        }
        WM_CLOSE | WM_DESTROY => {
            with_state(|s| {
                if s.result.is_none() {
                    s.result = Some(None);
                }
            });
            if msg == WM_CLOSE {
                return LRESULT(0);
            }
            // SAFETY: chuyển nguyên tham số cho xử lý mặc định.
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
        // SAFETY: chuyển nguyên tham số cho xử lý mặc định.
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

fn monitor_dpi_at(p: POINT) -> u32 {
    let (mut x, mut y) = (96u32, 96u32);
    // SAFETY: MonitorFromPoint với DEFAULTTONEAREST luôn trả về một màn hình; chỉ đọc DPI.
    let ok = unsafe {
        let m = MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST);
        GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut x, &mut y).is_ok()
    };
    if ok && x > 0 {
        x
    } else {
        96
    }
}

fn create_window(origin: Point, w: i32, h: i32) -> Option<HWND> {
    // SAFETY: đăng ký lớp (lần sau trả lỗi "đã có", bỏ qua) và tạo cửa sổ ở luồng chính.
    unsafe {
        let hinst = GetModuleHandleW(None).ok()?;
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            CLASS,
            w!("darkshot"),
            WS_POPUP,
            origin.x,
            origin.y,
            w,
            h,
            None,
            None,
            Some(hinst.into()),
            None,
        )
        .ok()?;
        // Bảo đảm client đúng bằng màn hình ảo (tọa độ client = tọa độ ảnh).
        let mut rc = RECT::default();
        if GetClientRect(hwnd, &mut rc).is_ok() && (rc.right != w || rc.bottom != h) {
            let _ = SetWindowPos(
                hwnd,
                None,
                origin.x,
                origin.y,
                w,
                h,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        Some(hwnd)
    }
}

/// Đưa lớp phủ lên trước và nhận bàn phím. Windows có thể từ chối SetForegroundWindow
/// (quy tắc chống giành tiêu điểm); khi đó gắn tạm hàng đợi nhập với luồng đang ở trước rồi thử lại.
/// Trả về `true` khi lớp phủ đã ở trước.
fn take_focus(hwnd: HWND) -> bool {
    // SAFETY: các API chỉ thao tác trên cửa sổ của ta và luồng đang ở trước; gắn rồi gỡ ngay.
    unsafe {
        let _ = SetForegroundWindow(hwnd);
        let fg = GetForegroundWindow();
        if fg != hwnd {
            let me = GetCurrentThreadId();
            let other = GetWindowThreadProcessId(fg, None);
            if other != 0 && other != me && AttachThreadInput(me, other, true).as_bool() {
                let _ = BringWindowToTop(hwnd);
                let _ = SetForegroundWindow(hwnd);
                let _ = AttachThreadInput(me, other, false);
            }
        }
        let _ = SetFocus(Some(hwnd));
        GetForegroundWindow() == hwnd
    }
}

/// Phím Esc toàn cục trong lúc lớp phủ mở, để không bao giờ kẹt khi chưa giành được
/// tiêu điểm. `State::set_esc_hotkey` đăng ký (và gỡ tạm khi ô chữ mở); guard này gỡ khi
/// `pick_region` kết thúc trên mọi đường đi, kể cả khi lớp phủ đóng giữa lúc đang gõ
/// (gỡ một phím chưa đăng ký chỉ trả lỗi, bỏ qua).
struct EscHotkey(HWND);

impl Drop for EscHotkey {
    fn drop(&mut self) {
        with_state(|s| s.esc_registered = false);
        // SAFETY: phím tắt của cửa sổ lớp phủ, gỡ khi cửa sổ còn sống (guard này thả trước WindowGuard).
        unsafe {
            let _ = UnregisterHotKey(Some(self.0), ESC_HOTKEY_ID);
        }
    }
}

impl State {
    /// Trạng thái ban đầu của lớp phủ cho ảnh `shot` cỡ `w`×`h` (đã kiểm hợp lệ).
    fn new(
        shot: &Screenshot,
        w: i32,
        h: i32,
        initial: Option<Rect>,
        custom: &CustomColors,
        gdiplus: bool,
    ) -> Result<State, String> {
        let img = &shot.image;
        let bounds = Rect { x: 0, y: 0, w, h };
        let cands = match &shot.windows {
            Some(wins) => snap::candidates(wins, &shot.monitors, bounds),
            None => Vec::new(),
        };
        let mut dark = Vec::new();
        if dark.try_reserve_exact(img.bgra.len()).is_err() {
            return Err(lang::tr(Msg::OverlayErrMemory).to_string());
        }
        dark.extend(img.bgra.iter().map(|b| b >> 1));

        let dpis = shot
            .monitors
            .iter()
            .map(|m| {
                monitor_dpi_at(POINT {
                    x: shot.origin.x + m.x + m.w / 2,
                    y: shot.origin.y + m.y + m.h / 2,
                })
            })
            .collect();
        // SAFETY: DC bộ nhớ để đo chữ; State xóa nó khi drop.
        let measure = unsafe { CreateCompatibleDC(None) };
        let color = LAST_COLOR.with(|c| c.get());
        let mut s = State {
            hwnd: HWND::default(),
            w,
            h,
            bright: img.bgra.as_ptr(),
            bright_len: img.bgra.len(),
            image: img,
            dark,
            back: BackBuffer::new(w, h),
            focus_retries: 0,
            monitors: shot.monitors.clone(),
            dpis,
            sel: Selection::with_rect(Rect { x: 0, y: 0, w, h }, initial),
            new_drag: false,
            drag_cursor: IDC_CROSS,
            pressed: None,
            hover: None,
            tool: None,
            marks: Annotations::default(),
            mark_bounds: Vec::new(),
            text: None,
            caret_on: false,
            high_surrogate: None,
            esc_registered: false,
            color,
            size: LAST_SIZE.with(|c| c.get()),
            sizes_open: false,
            size_hover: None,
            size_pressed: None,
            shape_tool: LAST_SHAPE.with(|c| c.get()),
            shapes_open: false,
            shape_hover: None,
            shape_pressed: None,
            stroke: None,
            selected: None,
            gesture: None,
            preview: None,
            preview_bounds: NO_RECT,
            gesture_at: Pt { x: 0.0, y: 0.0 },
            grip_off: Pt { x: 0.0, y: 0.0 },
            gesture_moved: false,
            side_hover: None,
            side_pressed: None,
            palette: false,
            expanded: false,
            custom: *custom,
            steps: false,
            pick: PickColor::from_rgb(color),
            target: None,
            field: None,
            picker_pressed: None,
            picker_drag: None,
            spectra: Vec::new(),
            lum_img: None,
            eat_char: None,
            result: None,
            fonts: Vec::new(),
            measure,
            gdiplus,
            cands,
            snap_windows: shot.windows.is_some(),
            snap_hover: None,
            snap_press: None,
            #[cfg(test)]
            inval_log: Default::default(),
            mouse: Point {
                x: shot.cursor.x.saturating_sub(shot.origin.x),
                y: shot.cursor.y.saturating_sub(shot.origin.y),
            },
        };
        // Khung đầu tiên đã có ứng viên dưới con trỏ (không chớp tối trước lần di chuột đầu).
        s.snap_hover = s.snap_at(s.mouse);
        Ok(s)
    }
}

/// Hiện lớp phủ trên ảnh `shot` và chờ người dùng chọn. `Rect` trong tọa độ ảnh.
/// `initial` (tọa độ ảnh): khung chọn sẵn, lớp phủ mở ở trạng thái đã chọn xong; khung không
/// nằm trọn trong ảnh thì như `None`. `Ok(None)` khi người dùng hủy; `Err` khi không mở được.
/// `custom`: 16 màu tự chọn; bảng màu sửa trực tiếp (thêm màu), giữ cả khi lớp phủ bị hủy.
pub fn pick_region(
    shot: &Screenshot,
    initial: Option<Rect>,
    custom: &mut CustomColors,
    steps: bool,
) -> Result<Option<Picked>, String> {
    let img = &shot.image;
    let size = (i32::try_from(img.width), i32::try_from(img.height));
    let (Ok(w), Ok(h)) = size else {
        return Err(lang::tr(Msg::OverlayErrTooLarge).to_string());
    };
    if !img.is_valid() {
        return Err(lang::tr(Msg::OverlayErrInvalid).to_string());
    }
    // Không mở lồng hai lớp phủ.
    let busy = OVERLAY.with(|o| o.try_borrow().map(|s| s.is_some()).unwrap_or(true));
    if busy {
        return Err(lang::tr(Msg::OverlayErrAlreadyOpen).to_string());
    }

    let gdiplus = GdiPlus::start();
    let mut state = State::new(shot, w, h, initial, custom, gdiplus.ok())?;
    state.steps = steps;
    let placed = OVERLAY.with(|o| match o.try_borrow_mut() {
        Ok(mut b) => {
            *b = Some(state);
            true
        }
        Err(_) => false,
    });
    if !placed {
        return Err(lang::tr(Msg::OverlayErrAlreadyOpen).to_string());
    }
    let _state = StateGuard;
    // Thả trước _state: chép màu tự chọn ra cho bên gọi trên mọi đường đi.
    let _custom = CustomOut(custom);
    let Some(hwnd) = create_window(shot.origin, w, h) else {
        return Err(lang::tr(Msg::OverlayErrWindow).to_string());
    };
    // Thả trước _state: cửa sổ bị hủy khi trạng thái (và ảnh tối) còn sống.
    let _window = WindowGuard(hwnd);
    with_state(|s| s.hwnd = hwnd);
    // Thả trước _window: gỡ phím Esc khi cửa sổ còn sống.
    let _esc = EscHotkey(hwnd);
    with_state(|s| s.set_esc_hotkey(true));
    // Chuột có thể đã rời chỗ lúc chụp: tính lại ứng viên theo vị trí hiện tại TRƯỚC khung đầu.
    let mut pt = POINT::default();
    // SAFETY: chỉ đọc vị trí chuột.
    if unsafe { GetCursorPos(&mut pt) }.is_ok() {
        let p = Point {
            x: pt.x.saturating_sub(shot.origin.x),
            y: pt.y.saturating_sub(shot.origin.y),
        };
        with_state(|s| {
            s.mouse = p;
            s.snap_hover = s.snap_at(p);
        });
    }

    // SAFETY: hiện và vẽ ngay cửa sổ vừa tạo của luồng này.
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = UpdateWindow(hwnd);
    }
    if !take_focus(hwnd) {
        // SAFETY: bộ hẹn giờ của cửa sổ lớp phủ; tự mất khi cửa sổ bị hủy.
        unsafe {
            SetTimer(Some(hwnd), FOCUS_TIMER_ID, FOCUS_RETRY_MS, None);
        }
    }
    let follow = with_state(|s| (initial.is_some() && s.sel.rect().is_some()) || s.snap_active());
    if follow == Some(true) {
        // Mở với khung có sẵn: con trỏ chuột và nút đang rê theo chỗ chuột đứng ngay,
        // không đợi lần di chuột đầu tiên. Gợi ý cửa sổ: hiện ngay ứng viên dưới con trỏ.
        let mut pt = POINT::default();
        // SAFETY: chỉ đọc vị trí chuột.
        if unsafe { GetCursorPos(&mut pt) }.is_ok() {
            let p = Point {
                x: pt.x - shot.origin.x,
                y: pt.y - shot.origin.y,
            };
            with_state(|s| s.on_move(p));
        }
    }

    let mut msg = MSG::default();
    loop {
        if let Some(Some(r)) = with_state(|s| s.result.take()) {
            return Ok(r);
        }
        // SAFETY: vòng lặp thông điệp chuẩn của luồng này.
        let got = unsafe { GetMessageW(&mut msg, None, 0, 0) }.0;
        if got == 0 {
            // WM_QUIT (ứng dụng đang thoát): hủy lớp phủ và trả WM_QUIT cho vòng lặp chính.
            // SAFETY: đăng lại thông điệp thoát cho luồng này.
            unsafe { PostQuitMessage(msg.wParam.0 as i32) };
            return Ok(None);
        }
        if got < 0 {
            return Err(lang::tr(Msg::OverlayErrLoop).to_string());
        }
        // SAFETY: phân phát thông điệp vừa nhận.
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkshot_core::annotate::STROKE_DIP;
    use darkshot_core::color::{hls_to_rgb, rgb_to_hls};
    use darkshot_core::overlay_layout::{bar_item_rect, bar_layout, bars_for};

    /// Mở lớp phủ thật, giả lập kéo khung rồi bấm nút Lưu (hoặc Esc), kiểm kết quả và
    /// số đối tượng GDI không tăng sau nhiều lần mở. Bỏ qua mặc định vì lớp phủ hiện
    /// lên màn hình của người đang dùng máy.
    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored"]
    fn overlay_round_trip_without_gdi_leaks() {
        use crate::capture::{capture_virtual_screen, GDI_TEST_LOCK};
        use std::time::Duration;
        use windows::Win32::System::Threading::{
            GetCurrentProcess, GetGuiResources, GR_GDIOBJECTS,
        };
        use windows::Win32::UI::HiDpi::{
            SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        };
        use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW};

        let _gdi = GDI_TEST_LOCK.lock();
        // SAFETY: như ứng dụng thật (manifest PerMonitorV2), áp cho luồng kiểm thử.
        unsafe {
            SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        let shot = capture_virtual_screen(false).unwrap();
        let m0 = shot.monitors[0];
        let (a, b) = (
            Point {
                x: m0.x + 100,
                y: m0.y + 100,
            },
            Point {
                x: m0.x + 400,
                y: m0.y + 300,
            },
        );
        let sel = Rect::from_points(a, b);
        // Vị trí nút Lưu, tính như State::toolbar.
        let corner = Point {
            x: sel.right() - 1,
            y: sel.bottom() - 1,
        };
        let mon = monitor_containing(corner, &shot.monitors);
        let dpi = monitor_dpi_at(POINT {
            x: shot.origin.x + mon.x + mon.w / 2,
            y: shot.origin.y + mon.y + mon.h / 2,
        });
        let (bar, _) = bars_for(sel, mon, dpi);
        let save_btn = bar_item_rect(bar, dpi, BarItem::Act(Action::Save)).unwrap();
        let save = Point {
            x: save_btn.x + save_btn.w / 2,
            y: save_btn.y + save_btn.h / 2,
        };

        // 0: bấm nút Lưu, 1: phím Esc (WM_KEYDOWN), 2: phím tắt Esc toàn cục (WM_HOTKEY).
        let run = |finish: u8| {
            let driver = std::thread::spawn(move || {
                let lp =
                    |p: Point| LPARAM(((p.y as isize & 0xFFFF) << 16) | (p.x as isize & 0xFFFF));
                for _ in 0..50 {
                    std::thread::sleep(Duration::from_millis(100));
                    // SAFETY: tìm cửa sổ lớp phủ theo tên lớp, chỉ đăng thông điệp vào nó.
                    let Ok(h) = (unsafe { FindWindowW(CLASS, None) }) else {
                        continue;
                    };
                    let post = |m: u32, wp: usize, l: LPARAM| {
                        // SAFETY: đăng thông điệp vào hàng đợi của cửa sổ còn sống.
                        let _ = unsafe { PostMessageW(Some(h), m, WPARAM(wp), l) };
                    };
                    post(WM_LBUTTONDOWN, 1, lp(a));
                    post(WM_MOUSEMOVE, 1, lp(b));
                    post(WM_LBUTTONUP, 0, lp(b));
                    std::thread::sleep(Duration::from_millis(300));
                    post(WM_MOUSEMOVE, 0, lp(save));
                    std::thread::sleep(Duration::from_millis(300));
                    match finish {
                        0 => {
                            post(WM_LBUTTONDOWN, 1, lp(save));
                            post(WM_LBUTTONUP, 0, lp(save));
                        }
                        1 => post(WM_KEYDOWN, usize::from(VK_ESCAPE.0), LPARAM(0)),
                        _ => post(WM_HOTKEY, ESC_HOTKEY_ID as usize, LPARAM(0)),
                    }
                    return true;
                }
                false
            });
            let r = pick_region(&shot, None, &mut CustomColors::default(), false);
            assert!(driver.join().unwrap(), "không thấy cửa sổ lớp phủ");
            r
        };
        // SAFETY: chỉ đọc số đối tượng GDI của tiến trình này.
        let gdi = || unsafe { GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS) };

        let saved = Ok(Some(Picked {
            action: Action::Save,
            rect: sel,
            shapes: Vec::new(),
        }));
        assert_eq!(run(0), saved);
        let after_first = gdi();
        assert_eq!(run(1), Ok(None));
        assert_eq!(run(2), Ok(None));
        // Vòng có vẽ: bút, hình chữ nhật, hoàn tác, mở bảng màu, đóng bằng Esc, rồi Lưu.
        let l = live::layout(&shot, sel);
        let inside = |dx, dy| Point {
            x: sel.x + dx,
            y: sel.y + dy,
        };
        let mut steps = vec![live::Step::Key(u16::from(b'P'))];
        steps.extend(live::drag(inside(20, 20), inside(120, 80)));
        steps.push(live::Step::Key(u16::from(b'B')));
        steps.extend(live::drag(inside(40, 60), inside(200, 150)));
        steps.push(live::Step::Key(u16::from(b'R')));
        steps.extend(live::drag(inside(30, 30), inside(150, 120)));
        steps.push(live::Step::Ctrl(u16::from(b'Z')));
        // Bảng Hình (công cụ đang là Hình chữ nhật): mở, rê lên ô, đóng bằng Esc; mở rồi đóng bằng
        // nút Hình.
        steps.extend(live::click(l.item(SideItem::Shapes)));
        steps.push(live::Step::Move(l.shape_cell(Tool::Line)));
        steps.push(live::Step::Key(VK_ESCAPE.0));
        steps.extend(live::click(l.item(SideItem::Shapes)));
        steps.extend(live::click(l.item(SideItem::Shapes)));
        steps.extend(live::click(l.item(SideItem::Color)));
        steps.push(live::Step::Key(VK_ESCAPE.0));
        // Bảng Cỡ: mở, chọn Vừa (đóng); mở lại, đóng bằng Esc. Hình elip, vòng số, hoàn tác, làm lại.
        steps.extend(live::click(l.item(SideItem::Size)));
        steps.extend(live::click(l.size_cell(SizeLevel::Medium)));
        steps.extend(live::click(l.item(SideItem::Size)));
        steps.push(live::Step::Key(VK_ESCAPE.0));
        steps.push(live::Step::Key(u16::from(b'E')));
        steps.extend(live::drag(inside(50, 50), inside(180, 140)));
        steps.push(live::Step::Key(u16::from(b'N')));
        steps.extend(live::click(inside(250, 40)));
        steps.push(live::Step::Ctrl(u16::from(b'Z')));
        steps.push(live::Step::Ctrl(u16::from(b'Y')));
        // Hoàn tác và Làm lại bằng nút của thanh ngang (rê chuột hiện gợi ý).
        steps.extend(live::click(l.bar_item(BarItem::Undo)));
        steps.extend(live::click(l.bar_item(BarItem::Redo)));
        steps.extend(live::click(l.save()));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        let p = r.unwrap().unwrap();
        assert_eq!((p.action, p.rect, p.shapes.len()), (Action::Save, sel, 4));
        assert!(matches!(p.shapes[0], Shape::Pen { .. }));
        assert!(matches!(p.shapes[1], Shape::Blur { .. }));
        assert!(matches!(p.shapes[2], Shape::Ellipse { .. }));
        assert!(matches!(p.shapes[3], Shape::Step { n: 1, .. }));
        // Vòng có chữ: mở ô, gõ, bấm chỗ khác (chốt, mở ô mới), gõ, Ctrl+S.
        let mut steps = vec![live::Step::Key(u16::from(b'T'))];
        steps.extend(live::click(inside(30, 40)));
        steps.extend(live::chars("ab"));
        steps.extend(live::click(inside(60, 120)));
        steps.extend(live::chars("c"));
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        let p = r.unwrap().unwrap();
        assert_eq!(p.shapes.len(), 2);
        assert!(p.shapes.iter().all(|s| matches!(s, Shape::Text { .. })));
        // Vòng bảng chọn màu: mở dạng đủ, kéo ô sắc độ và thanh độ sáng, thêm màu, đóng bằng Esc.
        let mut steps = live::click(l.item(SideItem::Color));
        steps.extend(live::click(l.define()));
        steps.extend(live::drag(
            l.full_center(|p| p.spectrum),
            l.full_center(|p| Rect {
                x: p.spectrum.x + 10,
                y: p.spectrum.y + 10,
                w: 1,
                h: 1,
            }),
        ));
        steps.extend(live::drag(
            l.full_center(|p| p.lum_bar),
            l.full_center(|p| Rect { h: 1, ..p.lum_bar }),
        ));
        steps.extend(live::click(l.full_center(|p| p.add_btn)));
        steps.push(live::Step::Key(VK_ESCAPE.0));
        steps.extend(live::click(l.save()));
        let mut custom = CustomColors::default();
        let (r, ok) = live::drive_custom(&shot, Some(sel), &mut custom, steps);
        assert!(ok);
        assert_eq!(r.unwrap().unwrap().action, Action::Save);
        assert_eq!(custom.slots.iter().flatten().count(), 1);
        // Vòng công cụ Chọn: hình chữ nhật, chọn, dời, đổi cỡ (tay nắm SE), xóa, hoàn tác, Lưu.
        let mut steps = vec![live::Step::Key(u16::from(b'R'))];
        steps.extend(live::drag(inside(40, 40), inside(140, 110)));
        steps.push(live::Step::Key(u16::from(b'V')));
        steps.extend(live::click(inside(40, 70)));
        // Dời: nắm cạnh trên giữa hai tay nắm (65, 40), +20,+10.
        steps.extend(live::drag(inside(65, 40), inside(85, 50)));
        steps.extend(live::drag(inside(160, 120), inside(220, 160)));
        steps.push(live::Step::Key(VK_DELETE.0));
        steps.push(live::Step::Ctrl(u16::from(b'Z')));
        steps.extend(live::click(l.save()));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        let p = r.unwrap().unwrap();
        assert_eq!(p.shapes.len(), 1);
        match &p.shapes[0] {
            Shape::Rect { a, b, .. } => {
                assert_eq!(
                    (*a, *b),
                    (live::pt(inside(60, 50)), live::pt(inside(220, 160)))
                );
            }
            other => panic!("không phải Rect: {other:?}"),
        }
        assert_eq!(run(0), saved);
        let after_third = gdi();
        assert!(
            after_third <= after_first,
            "GDI tăng từ {after_first} lên {after_third}"
        );
    }

    /// Gợi ý cửa sổ bật: mở/đóng lớp phủ 20 lần, mỗi lần rê qua các ứng viên (đổi khung xem
    /// trước, ẩn/hiện viên nhắc), chọn bằng bấm hoặc Enter rồi Lưu; số đối tượng GDI không tăng.
    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored snap_overlay_round_trip_without_gdi_leaks --test-threads=1"]
    fn snap_overlay_round_trip_without_gdi_leaks() {
        use windows::Win32::System::Threading::{
            GetCurrentProcess, GetGuiResources, GR_GDIOBJECTS,
        };
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let mut shot = live_setup();
        let m0 = shot.monitors[0];
        let win = Rect {
            x: m0.x + 150,
            y: m0.y + 150,
            w: 400,
            h: 300,
        };
        let inner = Rect {
            x: m0.x + 200,
            y: m0.y + 200,
            w: 120,
            h: 90,
        };
        shot.windows = Some(vec![inner, win]);
        let in_inner = Point {
            x: inner.x + 60,
            y: inner.y + 45,
        };
        let in_win = Point {
            x: win.x + 300,
            y: win.y + 250,
        };
        let near_hint = Point {
            x: m0.x + m0.w / 2,
            y: m0.y + 20,
        };
        let background = Point {
            x: m0.right() - 30,
            y: m0.bottom() - 30,
        };
        // SAFETY: chỉ đọc số đối tượng GDI của tiến trình này.
        let gdi = || unsafe { GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS) };
        let run = |i: usize| {
            let (target, at) = if i.is_multiple_of(2) {
                (win, in_win)
            } else {
                (inner, in_inner)
            };
            let mut steps = vec![
                live::Step::Move(near_hint),
                live::Step::Move(in_inner),
                live::Step::Move(in_win),
                live::Step::Move(background),
            ];
            // Con trỏ thật đứng tại `at`: lần di chuột hệ thống tự sinh (khi giữ chuột, khi hiện
            // cửa sổ) không đổi ứng viên đang xem trước, không biến cú bấm thành kéo.
            steps.push(live::Step::Cursor(at));
            if i % 4 < 2 {
                steps.extend(live::click(at));
            } else {
                steps.push(live::Step::Move(at));
                steps.push(live::Step::Key(VK_RETURN.0));
            }
            steps.extend(live::click(live::layout(&shot, target).save()));
            let (r, ok) = live::drive(&shot, None, steps);
            assert!(ok, "lần {i}");
            assert_eq!(
                r,
                Ok(Some(Picked {
                    action: Action::Save,
                    rect: target,
                    shapes: Vec::new(),
                })),
                "lần {i}"
            );
        };
        run(0);
        let after_first = gdi();
        for i in 1..20 {
            run(i);
        }
        let after = gdi();
        assert!(
            after <= after_first,
            "GDI tăng từ {after_first} lên {after}"
        );
    }

    /// Review Focus 1: `mark_bounds` luôn cùng độ dài, thứ tự với danh sách hình qua thêm, xóa
    /// giữa, hoàn tác, làm lại; vùng làm mới chứa hộp cũ và mới.
    #[test]
    fn mark_bounds_follow_history() {
        use darkshot_core::annotate::DEFAULT_COLOR;
        let line = |x: f32| Shape::Line {
            a: Pt { x, y: 0.0 },
            b: Pt {
                x: x + 30.0,
                y: 9.0,
            },
            width: 3.0,
            color: DEFAULT_COLOR,
        };
        let step = |x: f32| Shape::Step {
            center: Pt { x, y: 50.0 },
            radius: 13.0,
            n: 0,
            color: DEFAULT_COLOR,
        };
        let covers = |dirty: &[Rect], r: Rect| {
            dirty.iter().any(|d| {
                d.x <= r.x && d.y <= r.y && d.right() >= r.right() && d.bottom() >= r.bottom()
            })
        };
        let mut marks = Annotations::default();
        let mut bounds: Vec<Rect> = Vec::new();
        let check = |marks: &Annotations, bounds: &[Rect]| {
            assert_eq!(bounds.len(), marks.shapes().len());
            for (i, s) in marks.shapes().iter().enumerate() {
                assert_eq!(bounds[i], shape_bounds(s), "mục {i}");
            }
        };
        for s in [step(10.0), line(100.0), step(200.0)] {
            let a = marks.add(s);
            let dirty = apply_bounds(&mut bounds, marks.shapes(), &a);
            check(&marks, &bounds);
            let last = *bounds.last().unwrap();
            assert!(covers(&dirty, last));
        }
        // Xóa giữa... rồi xóa vòng số đầu: vòng sau đổi số, phải được làm mới.
        let a = marks.remove(1).unwrap();
        let removed = shape_bounds(&line(100.0));
        let dirty = apply_bounds(&mut bounds, marks.shapes(), &a);
        check(&marks, &bounds);
        assert!(covers(&dirty, removed));
        let a = marks.remove(0).unwrap();
        assert_eq!(a.renumbered, vec![0]);
        let dirty = apply_bounds(&mut bounds, marks.shapes(), &a);
        check(&marks, &bounds);
        assert!(covers(&dirty, bounds[0]));
        // Hoàn tác hai lần, làm lại một lần.
        for _ in 0..2 {
            let a = marks.undo().unwrap();
            let dirty = apply_bounds(&mut bounds, marks.shapes(), &a);
            check(&marks, &bounds);
            assert!(!dirty.is_empty());
        }
        assert_eq!(marks.shapes().len(), 3);
        let a = marks.redo().unwrap();
        apply_bounds(&mut bounds, marks.shapes(), &a);
        check(&marks, &bounds);
        // Thay hình: hộp tại vị trí được tính lại, vùng làm mới gồm cả cũ và mới.
        let old = bounds[1];
        let a = marks.replace(1, step(400.0)).unwrap();
        let dirty = apply_bounds(&mut bounds, marks.shapes(), &a);
        check(&marks, &bounds);
        assert!(covers(&dirty, old) && covers(&dirty, bounds[1]));
        // Hoàn tác hết rồi làm lại hết.
        while let Some(a) = marks.undo() {
            apply_bounds(&mut bounds, marks.shapes(), &a);
            check(&marks, &bounds);
        }
        while let Some(a) = marks.redo() {
            apply_bounds(&mut bounds, marks.shapes(), &a);
            check(&marks, &bounds);
        }
        // Lệch độ dài (không nên xảy ra): dựng lại cả danh sách.
        bounds.pop();
        let a = marks.add(line(5.0));
        apply_bounds(&mut bounds, marks.shapes(), &a);
        check(&marks, &bounds);
    }

    /// Lớp phủ thật được điều khiển bằng thông điệp giả từ một luồng phụ.
    mod live {
        use super::*;
        use crate::capture::Screenshot;
        use std::sync::atomic::Ordering;
        use std::time::Duration;
        use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, IsWindow, PostMessageW};

        pub enum Step {
            Down(Point),
            Move(Point),
            Up(Point),
            /// WM_KEYDOWN không phím bổ trợ.
            Key(u16),
            /// WM_KEYDOWN lặp (phím đang giữ: bit 30 của lParam).
            KeyRepeat(u16),
            /// WM_CHAR với một đơn vị UTF-16.
            Char(u16),
            /// Gửi WM_CLOSE (đóng lớp phủ như khi hệ thống yêu cầu).
            Close,
            /// Phím Esc toàn cục đang trống (`true`: không ai giữ, kể cả lớp phủ) hay đang bị giữ.
            /// Kiểm bằng `RegisterHotKey` ở luồng phụ, gỡ ngay nếu đăng ký được.
            EscFree(bool),
            /// WM_KEYDOWN khi Ctrl đang giữ.
            Ctrl(u16),
            /// WM_KEYDOWN khi Ctrl và Shift đang giữ.
            CtrlShift(u16),
            /// Phím tắt Esc toàn cục.
            Hotkey,
            /// Lớp phủ vẫn mở sau bước này.
            Alive,
            /// Lớp phủ đang giữ chuột (`true`) hay không (đọc qua `GetGUIThreadInfo`).
            Captured(bool),
            /// Giữ (`true`) hoặc nhả Shift giả cho các bước sau (`key_held(VK_SHIFT)`).
            Shift(bool),
            /// Đặt con trỏ thật tại điểm (tọa độ ảnh chụp, cộng `origin` của ảnh) bằng `SetCursorPos`.
            Cursor(Point),
            /// Chờ lớp phủ vẽ xong rồi chạy phép kiểm ở luồng phụ; `false` làm `drive` trả `false`.
            Probe(Box<dyn FnOnce() -> bool + Send>),
        }

        pub fn drag(a: Point, b: Point) -> Vec<Step> {
            let mid = Point {
                x: (a.x + b.x) / 2,
                y: (a.y + b.y) / 2,
            };
            vec![Step::Down(a), Step::Move(mid), Step::Move(b), Step::Up(b)]
        }

        pub fn click(p: Point) -> Vec<Step> {
            vec![Step::Move(p), Step::Down(p), Step::Up(p)]
        }

        pub struct Layout {
            pub bar: Rect,
            pub side: Rect,
            /// Bảng màu dạng gọn và dạng đủ.
            pub palette: Rect,
            pub full: Rect,
            /// Bảng Cỡ.
            pub sizes: Rect,
            /// Bảng Hình.
            pub shapes: Rect,
            pub dpi: u32,
            pub sl: SideLayout,
        }

        impl Layout {
            fn center(r: Rect) -> Point {
                Point {
                    x: r.x + r.w / 2,
                    y: r.y + r.h / 2,
                }
            }

            /// Giữa mục `item` của thanh dọc.
            pub fn item(&self, item: SideItem) -> Point {
                let r = side_item_rect(self.side, &self.sl, side_index(item)).unwrap();
                Self::center(r)
            }

            /// Giữa ô của công cụ `tool` trong bảng Hình.
            pub fn shape_cell(&self, tool: Tool) -> Point {
                let i = SHAPE_TOOLS.iter().position(|t| *t == tool).unwrap();
                Self::center(shape_cell_rect(self.shapes, self.dpi, i))
            }

            /// Giữa mục `item` của thanh ngang.
            pub fn bar_item(&self, item: BarItem) -> Point {
                Self::center(bar_item_rect(self.bar, self.dpi, item).unwrap())
            }

            /// Giữa ô mức `level` của bảng Cỡ.
            pub fn size_cell(&self, level: SizeLevel) -> Point {
                let i = SizeLevel::ALL.iter().position(|l| *l == level).unwrap();
                Self::center(size_cell_rect(self.sizes, self.dpi, i))
            }

            /// Giữa ô màu cơ bản thứ `i` của bảng dạng gọn.
            pub fn cell(&self, i: usize) -> Point {
                Self::center(at(self.palette, picker_layout(self.dpi, false).basic[i]))
            }

            /// Bảng (gốc) và bố cục theo dạng.
            pub fn picker(&self, expanded: bool) -> (Rect, PickerLayout) {
                let pal = if expanded { self.full } else { self.palette };
                (pal, picker_layout(self.dpi, expanded))
            }

            /// Ô `r` (tương đối) của bảng dạng gọn/đủ, tọa độ ảnh.
            pub fn abs(&self, expanded: bool, r: Rect) -> Rect {
                at(self.picker(expanded).0, r)
            }

            /// Giữa nút "Định nghĩa màu tự chọn >>" (dạng gọn).
            pub fn define(&self) -> Point {
                let (pal, l) = self.picker(false);
                Self::center(at(pal, l.define_btn))
            }

            /// Giữa phần `f` của bố cục dạng đủ.
            pub fn full_center(&self, f: impl Fn(&PickerLayout) -> Rect) -> Point {
                let (pal, l) = self.picker(true);
                Self::center(at(pal, f(&l)))
            }

            /// Giữa nút Lưu của thanh ngang.
            pub fn save(&self) -> Point {
                self.bar_item(BarItem::Act(Action::Save))
            }
        }

        fn dpi_of(shot: &Screenshot, mon: Rect) -> u32 {
            monitor_dpi_at(POINT {
                x: shot.origin.x + mon.x + mon.w / 2,
                y: shot.origin.y + mon.y + mon.h / 2,
            })
        }

        /// Vị trí các thanh, tính như `State`.
        pub fn layout(shot: &Screenshot, sel: Rect) -> Layout {
            let corner = Point {
                x: sel.right() - 1,
                y: sel.bottom() - 1,
            };
            let mon = monitor_containing(corner, &shot.monitors);
            let dpi = dpi_of(shot, mon);
            let (bar, side) = bars_for(sel, mon, dpi);
            let palette = palette_rect_for(side, sel, mon, dpi, false);
            let full = palette_rect_for(side, sel, mon, dpi, true);
            let sizes = size_rect_for(side, sel, mon, dpi);
            let shapes = shapes_rect_for(side, sel, mon, dpi);
            Layout {
                bar,
                side,
                palette,
                full,
                sizes,
                shapes,
                dpi,
                sl: side_layout_for(mon, dpi),
            }
        }

        /// DPI của màn hình chứa điểm `p` (tọa độ ảnh chụp).
        pub fn dpi_at(shot: &Screenshot, p: Point) -> u32 {
            dpi_of(shot, monitor_containing(p, &shot.monitors))
        }

        /// Vùng chọn thử trên màn hình đầu tiên.
        pub fn test_sel(shot: &Screenshot) -> Rect {
            let m0 = shot.monitors[0];
            Rect {
                x: m0.x + 100,
                y: m0.y + 100,
                w: 300,
                h: 200,
            }
        }

        /// Mở lớp phủ, chạy `steps` từ luồng phụ. Trả kết quả và `false` nếu một bước `Alive`
        /// thấy lớp phủ đã đóng, hoặc lớp phủ không tự đóng sau bước cuối (khi đó gửi WM_CLOSE).
        pub fn drive(
            shot: &Screenshot,
            initial: Option<Rect>,
            steps: Vec<Step>,
        ) -> (Result<Option<Picked>, String>, bool) {
            drive_custom(shot, initial, &mut CustomColors::default(), steps)
        }

        /// Như `drive`, với màu tự chọn `custom` (lớp phủ sửa trực tiếp).
        pub fn drive_custom(
            shot: &Screenshot,
            initial: Option<Rect>,
            custom: &mut CustomColors,
            steps: Vec<Step>,
        ) -> (Result<Option<Picked>, String>, bool) {
            let origin = shot.origin;
            let driver = std::thread::spawn(move || {
                let lp =
                    |p: Point| LPARAM(((p.y as isize & 0xFFFF) << 16) | (p.x as isize & 0xFFFF));
                let mut found = None;
                for _ in 0..50 {
                    std::thread::sleep(Duration::from_millis(100));
                    // SAFETY: tìm cửa sổ lớp phủ theo tên lớp.
                    if let Ok(h) = unsafe { FindWindowW(CLASS, None) } {
                        found = Some(h);
                        break;
                    }
                }
                let Some(h) = found else { return false };
                std::thread::sleep(Duration::from_millis(300));
                let post = |m: u32, wp: usize, l: LPARAM| {
                    // SAFETY: đăng thông điệp vào hàng đợi của cửa sổ lớp phủ.
                    let _ = unsafe { PostMessageW(Some(h), m, WPARAM(wp), l) };
                };
                // SAFETY: chỉ hỏi cửa sổ còn sống không.
                let alive = || unsafe { IsWindow(Some(h)) }.as_bool();
                let mut ok = true;
                for step in steps {
                    match step {
                        Step::Down(p) => post(WM_LBUTTONDOWN, 1, lp(p)),
                        Step::Move(p) => post(WM_MOUSEMOVE, 1, lp(p)),
                        Step::Up(p) => post(WM_LBUTTONUP, 0, lp(p)),
                        Step::Key(vk) => post(WM_KEYDOWN, usize::from(vk), LPARAM(0)),
                        Step::KeyRepeat(vk) => post(WM_KEYDOWN, usize::from(vk), LPARAM(1 << 30)),
                        Step::Char(c) => post(WM_CHAR, usize::from(c), LPARAM(1)),
                        Step::Close => post(WM_CLOSE, 0, LPARAM(0)),
                        Step::EscFree(want) => {
                            std::thread::sleep(Duration::from_millis(300));
                            ok &= esc_free() == want;
                        }
                        Step::Ctrl(vk) => {
                            TEST_MODS.store(1, Ordering::SeqCst);
                            post(WM_KEYDOWN, usize::from(vk), LPARAM(0));
                            std::thread::sleep(Duration::from_millis(200));
                            TEST_MODS.store(0, Ordering::SeqCst);
                        }
                        Step::CtrlShift(vk) => {
                            TEST_MODS.store(3, Ordering::SeqCst);
                            post(WM_KEYDOWN, usize::from(vk), LPARAM(0));
                            std::thread::sleep(Duration::from_millis(200));
                            TEST_MODS.store(0, Ordering::SeqCst);
                        }
                        Step::Hotkey => post(WM_HOTKEY, ESC_HOTKEY_ID as usize, LPARAM(0)),
                        Step::Alive => {
                            std::thread::sleep(Duration::from_millis(300));
                            ok &= alive();
                        }
                        Step::Captured(want) => {
                            std::thread::sleep(Duration::from_millis(300));
                            ok &= captured(h) == want;
                        }
                        Step::Shift(on) => {
                            TEST_MODS.store(if on { 2 } else { 0 }, Ordering::SeqCst);
                            std::thread::sleep(Duration::from_millis(100));
                        }
                        Step::Cursor(p) => {
                            use windows::Win32::UI::HiDpi::{
                                SetThreadDpiAwarenessContext,
                                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
                            };
                            use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;
                            // SAFETY: tọa độ pixel vật lý như lớp phủ (PerMonitorV2 cho luồng
                            // này); chỉ dời con trỏ chuột.
                            unsafe {
                                SetThreadDpiAwarenessContext(
                                    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
                                );
                                let _ = SetCursorPos(origin.x + p.x, origin.y + p.y);
                            }
                            std::thread::sleep(Duration::from_millis(100));
                        }
                        Step::Probe(f) => {
                            std::thread::sleep(Duration::from_millis(400));
                            ok &= f();
                        }
                    }
                    std::thread::sleep(Duration::from_millis(40));
                }
                for _ in 0..30 {
                    if !alive() {
                        return ok;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                post(WM_CLOSE, 0, LPARAM(0));
                false
            });
            let r = pick_region(shot, initial, custom, false);
            let ok = driver.join().unwrap();
            (r, ok)
        }

        /// Cửa sổ `h` đang giữ chuột của luồng của nó.
        fn captured(h: HWND) -> bool {
            use windows::Win32::UI::WindowsAndMessaging::{GetGUIThreadInfo, GUITHREADINFO};
            let mut info = GUITHREADINFO {
                cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };
            // SAFETY: chỉ đọc; info có cbSize đúng.
            unsafe {
                let tid = GetWindowThreadProcessId(h, None);
                GetGUIThreadInfo(tid, &mut info).is_ok() && info.hwndCapture == h
            }
        }

        /// Thử đăng ký Esc toàn cục ở luồng này; được thì gỡ ngay và trả `true`.
        fn esc_free() -> bool {
            const PROBE_ID: i32 = 0x0E5D;
            // SAFETY: phím tắt gắn với luồng này (không cửa sổ); gỡ ngay khi đăng ký được.
            unsafe {
                let free =
                    RegisterHotKey(None, PROBE_ID, MOD_NOREPEAT, u32::from(VK_ESCAPE.0)).is_ok();
                if free {
                    let _ = UnregisterHotKey(None, PROBE_ID);
                }
                free
            }
        }

        /// Các bước gửi chuỗi `s` bằng WM_CHAR (mỗi đơn vị UTF-16 một bước).
        pub fn chars(s: &str) -> Vec<Step> {
            s.encode_utf16().map(Step::Char).collect()
        }

        pub fn pt(p: Point) -> Pt {
            Pt {
                x: p.x as f32,
                y: p.y as f32,
            }
        }
    }

    fn live_setup() -> crate::capture::Screenshot {
        use crate::capture::capture_virtual_screen;
        use windows::Win32::UI::HiDpi::{
            SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        };
        // SAFETY: như ứng dụng thật (manifest PerMonitorV2), áp cho luồng kiểm thử.
        unsafe {
            SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        capture_virtual_screen(false).unwrap()
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored drag_outside_with_shapes_keeps_everything --test-threads=1"]
    fn drag_outside_with_shapes_keeps_everything() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let l = live::layout(&shot, sel);
        let at = |dx, dy| Point {
            x: sel.x + dx,
            y: sel.y + dy,
        };
        let mut steps = vec![live::Step::Key(u16::from(b'P'))];
        steps.extend(live::drag(at(30, 40), at(150, 90)));
        steps.push(live::Step::Key(u16::from(b'P')));
        // Kéo ngoài vùng (trên trái vùng chọn) khi đã có hình, không công cụ.
        steps.extend(live::drag(at(-80, -80), at(-20, -30)));
        steps.extend(live::click(l.save()));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        let p = r.unwrap().unwrap();
        assert_eq!((p.action, p.rect), (Action::Save, sel));
        assert_eq!(p.shapes.len(), 1);
        assert!(matches!(p.shapes[0], Shape::Pen { .. }));
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored draw_line_and_save_returns_shape --test-threads=1"]
    fn draw_line_and_save_returns_shape() {
        use darkshot_core::annotate::{dip_to_px, PALETTE};
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let l = live::layout(&shot, sel);
        let (a, b) = (
            Point {
                x: sel.x + 40,
                y: sel.y + 50,
            },
            Point {
                x: sel.x + 210,
                y: sel.y + 130,
            },
        );
        let width = dip_to_px(3.0, live::dpi_at(&shot, a));
        let line = |color| Shape::Line {
            a: live::pt(a),
            b: live::pt(b),
            width,
            color,
        };
        // Chọn màu xanh lam (ô 5, #0080FF) qua bảng màu, chọn Đường thẳng, kéo, Ctrl+S.
        let mut steps = live::click(l.item(SideItem::Color));
        steps.extend(live::click(l.cell(5)));
        steps.push(live::Step::Key(u16::from(b'L')));
        steps.extend(live::drag(a, b));
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        let p = r.unwrap().unwrap();
        assert_eq!((p.action, p.rect), (Action::Save, sel));
        assert_eq!(p.shapes, vec![line(PALETTE[5])]);
        // Lần mở sau nhớ màu; công cụ thì không (L chọn lại).
        let mut steps = vec![live::Step::Key(u16::from(b'L'))];
        steps.extend(live::drag(a, b));
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        assert_eq!(r.unwrap().unwrap().shapes, vec![line(PALETTE[5])]);
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored blur_save_redacts --test-threads=1"]
    fn blur_save_redacts() {
        use darkshot_core::annotate::{blur_pixel_rect, redact_color};
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let l = live::layout(&shot, sel);
        let (a, b) = (
            Point {
                x: sel.x + 30,
                y: sel.y + 40,
            },
            Point {
                x: sel.x + 220,
                y: sel.y + 120,
            },
        );
        let mut steps = vec![live::Step::Key(u16::from(b'B'))];
        steps.extend(live::drag(a, b));
        steps.extend(live::click(l.save()));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        let p = r.unwrap().unwrap();
        assert_eq!((p.action, p.rect), (Action::Save, sel));
        let rect = blur_pixel_rect(live::pt(a), live::pt(b));
        let want = redact_color(&shot.image, rect).unwrap();
        assert_eq!(
            p.shapes,
            vec![Shape::Blur {
                a: live::pt(a),
                b: live::pt(b),
                color: want,
            }]
        );
        let out = crate::picked_image(&shot.image, p.rect, &p.shapes).unwrap();
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                let i = (((y - sel.y) as u32 * out.width + (x - sel.x) as u32) * 4) as usize;
                assert_eq!(
                    &out.bgra[i..i + 4],
                    &[want.2, want.1, want.0, 255],
                    "({x},{y})"
                );
            }
        }
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored undo_removes_last_shape --test-threads=1"]
    fn undo_removes_last_shape() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let l = live::layout(&shot, sel);
        let at = |dx, dy| Point {
            x: sel.x + dx,
            y: sel.y + dy,
        };
        let mut steps = vec![live::Step::Key(u16::from(b'R'))];
        steps.extend(live::drag(at(20, 20), at(120, 90)));
        steps.push(live::Step::Key(u16::from(b'H')));
        steps.extend(live::drag(at(30, 150), at(250, 160)));
        steps.push(live::Step::Ctrl(u16::from(b'Z')));
        steps.extend(live::click(l.save()));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        let p = r.unwrap().unwrap();
        assert_eq!(p.shapes.len(), 1);
        assert!(matches!(p.shapes[0], Shape::Rect { .. }));
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored no_tool_flow_unchanged --test-threads=1"]
    fn no_tool_flow_unchanged() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let l = live::layout(&shot, sel);
        let mut steps = live::drag(
            Point { x: sel.x, y: sel.y },
            Point {
                x: sel.right(),
                y: sel.bottom(),
            },
        );
        steps.extend(live::click(l.save()));
        let (r, ok) = live::drive(&shot, None, steps);
        assert!(ok);
        assert_eq!(
            r,
            Ok(Some(Picked {
                action: Action::Save,
                rect: sel,
                shapes: Vec::new(),
            }))
        );
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored esc_closes_palette_not_overlay --test-threads=1"]
    fn esc_closes_palette_not_overlay() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let l = live::layout(&shot, sel);
        // Esc bằng WM_KEYDOWN.
        let mut steps = live::click(l.item(SideItem::Color));
        steps.push(live::Step::Key(VK_ESCAPE.0));
        steps.push(live::Step::Alive);
        steps.push(live::Step::Key(VK_ESCAPE.0));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(
            ok,
            "Esc thứ nhất phải chỉ đóng bảng màu, Esc thứ hai đóng lớp phủ"
        );
        assert_eq!(r, Ok(None));
        // Esc qua phím tắt toàn cục.
        let mut steps = live::click(l.item(SideItem::Color));
        steps.push(live::Step::Hotkey);
        steps.push(live::Step::Alive);
        steps.push(live::Step::Hotkey);
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok, "phím tắt Esc thứ nhất phải chỉ đóng bảng màu");
        assert_eq!(r, Ok(None));
    }

    #[test]
    fn key_repeat_bit_is_read() {
        assert!(!key_repeat(LPARAM(0)));
        assert!(!key_repeat(LPARAM(1)));
        assert!(key_repeat(LPARAM(1 << 30)));
        assert!(key_repeat(LPARAM((1 << 30) | 1)));
    }

    /// Chữ (origin, nội dung) của các hình Text trong `shapes`; hình khác làm kiểm thử thất bại.
    fn texts(shapes: &[Shape]) -> Vec<(Pt, String)> {
        shapes
            .iter()
            .map(|s| match s {
                Shape::Text { origin, text, .. } => (*origin, text.clone()),
                other => panic!("không phải Text: {other:?}"),
            })
            .collect()
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored held_tool_key_does_not_toggle --test-threads=1"]
    fn held_tool_key_does_not_toggle() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let l = live::layout(&shot, sel);
        let at = |dx, dy| Point {
            x: sel.x + dx,
            y: sel.y + dy,
        };
        // Giữ P: một WM_KEYDOWN rồi các lần lặp; công cụ phải vẫn là Bút vẽ.
        let mut steps = vec![
            live::Step::Key(u16::from(b'P')),
            live::Step::KeyRepeat(u16::from(b'P')),
            live::Step::KeyRepeat(u16::from(b'P')),
            live::Step::KeyRepeat(u16::from(b'P')),
        ];
        steps.extend(live::drag(at(30, 40), at(150, 90)));
        steps.extend(live::click(l.save()));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        let p = r.unwrap().unwrap();
        assert_eq!(p.shapes.len(), 1);
        assert!(matches!(p.shapes[0], Shape::Pen { .. }));
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored wm_char_sequence_builds_text --test-threads=1"]
    fn wm_char_sequence_builds_text() {
        use darkshot_core::annotate::{dip_to_px, TEXT_DIP};
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let at = Point {
            x: sel.x + 40,
            y: sel.y + 50,
        };
        const BS: u16 = 0x08;
        let mut steps = vec![live::Step::Key(u16::from(b'T'))];
        steps.extend(live::click(at));
        // Unikey: gõ "Viet" kiểu Telex, bộ gõ xóa rồi gửi lại ký tự dựng sẵn.
        steps.extend(live::chars("Vie"));
        steps.push(live::Step::Char(BS));
        steps.extend(live::chars("\u{ea}t"));
        steps.push(live::Step::Char(BS));
        steps.push(live::Step::Char(BS));
        steps.extend(live::chars("\u{1ec7}t"));
        // Enter rồi "2".
        steps.extend(live::chars("\r2"));
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        let p = r.unwrap().unwrap();
        assert_eq!((p.action, p.rect), (Action::Save, sel));
        assert_eq!(
            texts(&p.shapes),
            vec![(live::pt(at), "Vi\u{1ec7}t\n2".to_string())]
        );
        let Shape::Text { size, .. } = &p.shapes[0] else {
            unreachable!()
        };
        assert_eq!(*size, dip_to_px(TEXT_DIP, live::dpi_at(&shot, at)));
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored esc_cancels_text_not_overlay --test-threads=1"]
    fn esc_cancels_text_not_overlay() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let at = Point {
            x: sel.x + 40,
            y: sel.y + 50,
        };
        let typing = || {
            let mut steps = vec![live::Step::Key(u16::from(b'T'))];
            steps.extend(live::click(at));
            steps.extend(live::chars("abc"));
            steps
        };
        // Esc bằng WM_KEYDOWN: lần đầu hủy ô, lần sau đóng lớp phủ.
        let mut steps = typing();
        steps.push(live::Step::Key(VK_ESCAPE.0));
        steps.push(live::Step::Alive);
        steps.push(live::Step::Key(VK_ESCAPE.0));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok, "Esc thứ nhất phải chỉ hủy ô chữ");
        assert_eq!(r, Ok(None));
        // Esc qua phím tắt toàn cục.
        let mut steps = typing();
        steps.push(live::Step::Hotkey);
        steps.push(live::Step::Alive);
        steps.push(live::Step::Hotkey);
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok, "phím tắt Esc thứ nhất phải chỉ hủy ô chữ");
        assert_eq!(r, Ok(None));
        // Ô bị hủy không được chốt.
        let mut steps = typing();
        steps.push(live::Step::Key(VK_ESCAPE.0));
        steps.push(live::Step::Alive);
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        assert_eq!(r.unwrap().unwrap().shapes, Vec::new());
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored click_elsewhere_commits --test-threads=1"]
    fn click_elsewhere_commits() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let l = live::layout(&shot, sel);
        let (a, b) = (
            Point {
                x: sel.x + 30,
                y: sel.y + 40,
            },
            Point {
                x: sel.x + 120,
                y: sel.y + 130,
            },
        );
        let mut steps = vec![live::Step::Key(u16::from(b'T'))];
        steps.extend(live::click(a));
        steps.extend(live::chars("A"));
        steps.extend(live::click(b));
        steps.extend(live::chars("B"));
        steps.extend(live::click(l.save()));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        let p = r.unwrap().unwrap();
        assert_eq!(p.action, Action::Save);
        assert_eq!(
            texts(&p.shapes),
            vec![
                (live::pt(a), "A".to_string()),
                (live::pt(b), "B".to_string())
            ]
        );
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored empty_text_is_dropped --test-threads=1"]
    fn empty_text_is_dropped() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let l = live::layout(&shot, sel);
        let mut steps = vec![live::Step::Key(u16::from(b'T'))];
        steps.extend(live::click(Point {
            x: sel.x + 30,
            y: sel.y + 40,
        }));
        // Đổi công cụ bằng nút Bút vẽ trên thanh dọc (phím P lúc này đi vào ô chữ).
        steps.extend(live::click(l.item(SideItem::Tool(Tool::Pen))));
        steps.extend(live::click(l.save()));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        let p = r.unwrap().unwrap();
        assert_eq!((p.action, p.shapes), (Action::Save, Vec::new()));
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored ctrl_z_while_typing --test-threads=1"]
    fn ctrl_z_while_typing() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let at = |dx, dy| Point {
            x: sel.x + dx,
            y: sel.y + dy,
        };
        let setup = || {
            let mut steps = vec![live::Step::Key(u16::from(b'P'))];
            steps.extend(live::drag(at(20, 20), at(120, 80)));
            steps.push(live::Step::Key(u16::from(b'T')));
            steps.extend(live::click(at(40, 140)));
            steps.extend(live::chars("x"));
            steps
        };
        // Ctrl+Z một lần: xóa ô đang gõ, nét Pen còn.
        let mut steps = setup();
        steps.push(live::Step::Ctrl(u16::from(b'Z')));
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        let p = r.unwrap().unwrap();
        assert_eq!(p.shapes.len(), 1);
        assert!(matches!(p.shapes[0], Shape::Pen { .. }));
        // Ctrl+Z lần nữa: hoàn tác nét Pen.
        let mut steps = setup();
        steps.push(live::Step::Ctrl(u16::from(b'Z')));
        steps.push(live::Step::Ctrl(u16::from(b'Z')));
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        assert_eq!(r.unwrap().unwrap().shapes, Vec::new());
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored tool_letters_go_into_text --test-threads=1"]
    fn tool_letters_go_into_text() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let (a, b) = (
            Point {
                x: sel.x + 30,
                y: sel.y + 40,
            },
            Point {
                x: sel.x + 120,
                y: sel.y + 130,
            },
        );
        let mut steps = vec![live::Step::Key(u16::from(b'T'))];
        steps.extend(live::click(a));
        // Như bàn phím thật: chỉ WM_KEYDOWN 'P'; TranslateMessage của vòng lặp lớp phủ sinh
        // WM_CHAR 'p'. Phím không đổi công cụ, ký tự vào ô. Cần Caps Lock tắt (bật thì ra "P").
        steps.push(live::Step::Key(u16::from(b'P')));
        // Công cụ vẫn là Chữ: bấm chỗ khác mở ô mới (không vẽ chấm Bút vẽ).
        steps.extend(live::click(b));
        steps.extend(live::chars("q"));
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        assert_eq!(
            texts(&r.unwrap().unwrap().shapes),
            vec![
                (live::pt(a), "p".to_string()),
                (live::pt(b), "q".to_string())
            ]
        );
    }

    #[test]
    #[ignore = "mở lớp phủ thật; cần Esc toàn cục không bị ứng dụng khác giữ; chạy tay: cargo test -p darkshot-windows -- --ignored escape_hotkey_restored --test-threads=1"]
    fn escape_hotkey_restored() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let at = Point {
            x: sel.x + 40,
            y: sel.y + 50,
        };
        // Lớp phủ giữ Esc toàn cục; nhả khi ô chữ mở; giữ lại khi ô đóng.
        let mut steps = vec![live::Step::EscFree(false), live::Step::Key(u16::from(b'T'))];
        steps.extend(live::click(at));
        steps.extend(live::chars("a"));
        steps.push(live::Step::EscFree(true));
        steps.push(live::Step::Key(VK_ESCAPE.0));
        steps.push(live::Step::EscFree(false));
        // Mở ô lần nữa rồi đóng lớp phủ giữa lúc đang gõ.
        steps.extend(live::click(at));
        steps.extend(live::chars("b"));
        steps.push(live::Step::EscFree(true));
        steps.push(live::Step::Close);
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        assert_eq!(r, Ok(None));
        // Lần mở sau: Esc toàn cục được giữ lại và đóng lớp phủ.
        let steps = vec![live::Step::EscFree(false), live::Step::Hotkey];
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        assert_eq!(r, Ok(None));
    }

    /// Hai điểm trong vùng chọn thử để vẽ một đường.
    fn line_points(sel: Rect) -> (Point, Point) {
        (
            Point {
                x: sel.x + 40,
                y: sel.y + 50,
            },
            Point {
                x: sel.x + 210,
                y: sel.y + 130,
            },
        )
    }

    /// Màu của hình Line duy nhất trong `shapes`.
    fn line_color(shapes: &[Shape]) -> Rgb {
        match shapes {
            [Shape::Line { color, .. }] => *color,
            other => panic!("không phải một Line: {other:?}"),
        }
    }

    fn near(a: Rgb, b: Rgb, tol: u8) -> bool {
        a.0.abs_diff(b.0) <= tol && a.1.abs_diff(b.1) <= tol && a.2.abs_diff(b.2) <= tol
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored picker_compact_click_selects_and_closes --test-threads=1"]
    fn picker_compact_click_selects_and_closes() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let l = live::layout(&shot, sel);
        let (a, b) = line_points(sel);
        let mut steps = live::click(l.item(SideItem::Color));
        steps.extend(live::click(l.cell(5)));
        steps.push(live::Step::Alive);
        // Bảng đã đóng: phím L chọn công cụ, kéo vẽ được (bảng mở thì cú bấm chỉ đóng bảng).
        steps.push(live::Step::Key(u16::from(b'L')));
        steps.extend(live::drag(a, b));
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok, "lớp phủ phải còn sau khi chọn màu");
        let p = r.unwrap().unwrap();
        assert_eq!(line_color(&p.shapes), PALETTE[5]);
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored picker_define_add_ok --test-threads=1"]
    fn picker_define_add_ok() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let l = live::layout(&shot, sel);
        let (a, b) = line_points(sel);
        let full = picker_layout(l.dpi, true);
        let sp = l.abs(true, full.spectrum);
        let click_sp = Point {
            x: sp.x + sp.w / 3,
            y: sp.y + sp.h / 4,
        };
        let (h, s) = spectrum_hs(sp, click_sp.x, click_sp.y);
        let bar = l.abs(true, full.lum_bar);
        let lum_from = Point {
            x: bar.x + bar.w / 2,
            y: bar.y + bar.h / 2,
        };
        let lum_to = Point {
            y: bar.y + bar.h / 3,
            ..lum_from
        };
        let want = hls_to_rgb(Hls {
            h,
            l: lum_at(bar, lum_to.y),
            s,
        });
        let mut steps = live::click(l.item(SideItem::Color));
        steps.extend(live::click(l.define()));
        steps.extend(live::click(click_sp));
        steps.extend(live::drag(lum_from, lum_to));
        steps.extend(live::click(l.full_center(|p| p.add_btn)));
        steps.extend(live::click(l.full_center(|p| p.ok_btn)));
        steps.push(live::Step::Key(u16::from(b'L')));
        steps.extend(live::drag(a, b));
        steps.extend(live::click(l.save()));
        let mut custom = CustomColors::default();
        let (r, ok) = live::drive_custom(&shot, Some(sel), &mut custom, steps);
        assert!(ok);
        let p = r.unwrap().unwrap();
        assert_eq!(p.action, Action::Save);
        let c = line_color(&p.shapes);
        assert!(near(c, want, 2), "{c:?} khác {want:?}");
        let added: Vec<Rgb> = custom.slots.iter().flatten().copied().collect();
        assert_eq!(added, vec![c]);
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored picker_cancel_keeps_custom --test-threads=1"]
    fn picker_cancel_keeps_custom() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let l = live::layout(&shot, sel);
        let (a, b) = line_points(sel);
        let before = LAST_COLOR.with(|c| c.get());
        let full = picker_layout(l.dpi, true);
        let sp = l.abs(true, full.spectrum);
        let click_sp = Point {
            x: sp.x + sp.w * 2 / 3,
            y: sp.y + sp.h / 5,
        };
        let (h, s) = spectrum_hs(sp, click_sp.x, click_sp.y);
        let added_color = hls_to_rgb(Hls {
            h,
            l: rgb_to_hls(before).l,
            s,
        });
        let mut steps = live::click(l.item(SideItem::Color));
        steps.extend(live::click(l.define()));
        steps.extend(live::click(click_sp));
        steps.extend(live::click(l.full_center(|p| p.add_btn)));
        steps.push(live::Step::Key(VK_ESCAPE.0));
        steps.push(live::Step::Alive);
        steps.push(live::Step::Key(u16::from(b'L')));
        steps.extend(live::drag(a, b));
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let mut custom = CustomColors::default();
        let (r, ok) = live::drive_custom(&shot, Some(sel), &mut custom, steps);
        assert!(ok, "Esc phải chỉ đóng bảng");
        assert_eq!(line_color(&r.unwrap().unwrap().shapes), before);
        let added: Vec<Rgb> = custom.slots.iter().flatten().copied().collect();
        assert_eq!(added, vec![added_color]);
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored picker_typing_does_not_reach_tools --test-threads=1"]
    fn picker_typing_does_not_reach_tools() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let l = live::layout(&shot, sel);
        let before = LAST_COLOR.with(|c| c.get());
        let red = l.full_center(|p| p.fields[3].1);
        // Bảng mở, chưa sửa ô số: phím chữ không chọn công cụ.
        let mut steps = live::click(l.item(SideItem::Color));
        steps.push(live::Step::Key(u16::from(b'H')));
        steps.extend(live::click(l.define()));
        steps.extend(live::click(red));
        // Đang sửa Red: phím công cụ bị nuốt, chữ số vào ô.
        steps.push(live::Step::Key(u16::from(b'P')));
        steps.push(live::Step::Char(u16::from(b'1')));
        steps.push(live::Step::Key(u16::from(b'R')));
        steps.push(live::Step::Char(u16::from(b'2')));
        // Enter chốt ô, Enter lần nữa là OK.
        steps.push(live::Step::Key(VK_RETURN.0));
        steps.push(live::Step::Key(VK_RETURN.0));
        steps.push(live::Step::Alive);
        // Không công cụ nào được chọn: kéo trong vùng chỉ dời vùng, không vẽ.
        steps.extend(live::drag(
            Point {
                x: sel.x + 30,
                y: sel.y + 40,
            },
            Point {
                x: sel.x + 150,
                y: sel.y + 90,
            },
        ));
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        assert_eq!(r.unwrap().unwrap().shapes, Vec::new());
        // Màu đã nhận Red = 12 (lần mở sau nhớ màu).
        let (a, b) = line_points(sel);
        let mut steps = vec![live::Step::Key(u16::from(b'L'))];
        steps.extend(live::drag(a, b));
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        assert_eq!(
            line_color(&r.unwrap().unwrap().shapes),
            Rgb(12, before.1, before.2)
        );
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored picker_close_while_dragging_releases_capture --test-threads=1"]
    fn picker_close_while_dragging_releases_capture() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let l = live::layout(&shot, sel);
        let from = l.full_center(|p| p.spectrum);
        let to = l.full_center(|p| p.lum_bar);
        for close in [VK_ESCAPE.0, VK_RETURN.0] {
            let mut steps = live::click(l.item(SideItem::Color));
            steps.extend(live::click(l.define()));
            // Kéo trong ô sắc độ, đóng bảng giữa chừng, rồi nhả.
            steps.push(live::Step::Down(from));
            steps.push(live::Step::Move(to));
            steps.push(live::Step::Captured(true));
            steps.push(live::Step::Key(close));
            steps.push(live::Step::Alive);
            steps.push(live::Step::Up(to));
            steps.push(live::Step::Captured(false));
            steps.push(live::Step::Alive);
            steps.push(live::Step::Key(VK_ESCAPE.0));
            let (r, ok) = live::drive(&shot, Some(sel), steps);
            assert!(ok, "phím {close:#x}: lớp phủ phải trả chuột sau khi nhả");
            assert_eq!(r, Ok(None));
        }
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored ellipse_draw_save --test-threads=1"]
    fn ellipse_draw_save() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let l = live::layout(&shot, sel);
        let (a, b) = (
            Point {
                x: sel.x + 40,
                y: sel.y + 30,
            },
            Point {
                x: sel.x + 240,
                y: sel.y + 170,
            },
        );
        let mut steps = vec![live::Step::Key(u16::from(b'E'))];
        steps.extend(live::drag(a, b));
        steps.extend(live::click(l.save()));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        let p = r.unwrap().unwrap();
        assert_eq!((p.action, p.rect), (Action::Save, sel));
        let color = match p.shapes.as_slice() {
            [Shape::Ellipse {
                a: pa,
                b: pb,
                width,
                color,
            }] => {
                assert_eq!((*pa, *pb), (live::pt(a), live::pt(b)));
                assert_eq!(*width, dip_to_px(STROKE_DIP, live::dpi_at(&shot, a)));
                *color
            }
            other => panic!("không phải một Ellipse: {other:?}"),
        };
        let out = crate::picked_image(&shot.image, p.rect, &p.shapes).unwrap();
        let px = |x: i32, y: i32| {
            let i = (((y - sel.y) as u32 * out.width + (x - sel.x) as u32) * 4) as usize;
            Rgb(out.bgra[i + 2], out.bgra[i + 1], out.bgra[i])
        };
        let src = |x: i32, y: i32| {
            let i = ((y as u32 * shot.image.width + x as u32) * 4) as usize;
            Rgb(
                shot.image.bgra[i + 2],
                shot.image.bgra[i + 1],
                shot.image.bgra[i],
            )
        };
        let (cx, cy) = ((a.x + b.x) / 2, (a.y + b.y) / 2);
        // Mép phải, mép dưới, mép trái của elip: có điểm ảnh đúng màu nét.
        for (ex, ey) in [(b.x, cy), (cx, b.y), (a.x, cy)] {
            let hit = (-1..=1).any(|d| {
                let (x, y) = if ey == cy { (ex + d, ey) } else { (ex, ey + d) };
                near(px(x, y), color, 8)
            });
            assert!(hit, "mép ({ex},{ey}) không có màu nét");
        }
        // Tâm giữ nguyên ảnh gốc.
        assert_eq!(px(cx, cy), src(cx, cy));
    }

    /// Số của các vòng Đánh số trong `shapes`; hình khác làm kiểm thử thất bại.
    fn step_numbers(shapes: &[Shape]) -> Vec<u32> {
        shapes
            .iter()
            .map(|s| match s {
                Shape::Step { n, .. } => *n,
                other => panic!("không phải Step: {other:?}"),
            })
            .collect()
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored steps_number_and_undo_redo --test-threads=1"]
    fn steps_number_and_undo_redo() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let l = live::layout(&shot, sel);
        let at = |dx, dy| Point {
            x: sel.x + dx,
            y: sel.y + dy,
        };
        let spots = [at(40, 40), at(110, 60), at(180, 90), at(250, 150)];
        let mut steps = vec![live::Step::Key(u16::from(b'N'))];
        for p in &spots[..3] {
            steps.extend(live::click(*p));
        }
        steps.push(live::Step::Ctrl(u16::from(b'Z')));
        steps.push(live::Step::Ctrl(u16::from(b'Y')));
        steps.extend(live::click(spots[3]));
        steps.extend(live::click(l.save()));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        let p = r.unwrap().unwrap();
        assert_eq!(p.action, Action::Save);
        assert_eq!(step_numbers(&p.shapes), vec![1, 2, 3, 4]);
        for (s, spot) in p.shapes.iter().zip(spots) {
            let Shape::Step { center, radius, .. } = s else {
                unreachable!()
            };
            assert_eq!(*center, live::pt(spot));
            assert_eq!(*radius, dip_to_px(13.0, live::dpi_at(&shot, spot)));
        }
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored undo_redo_multi --test-threads=1"]
    fn undo_redo_multi() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let l = live::layout(&shot, sel);
        let at = |dx, dy| Point {
            x: sel.x + dx,
            y: sel.y + dy,
        };
        let mut steps = vec![live::Step::Key(u16::from(b'L'))];
        for i in 0..3 {
            steps.extend(live::drag(at(20, 20 + 40 * i), at(200, 30 + 40 * i)));
        }
        for _ in 0..3 {
            steps.push(live::Step::Ctrl(u16::from(b'Z')));
        }
        // Làm lại bằng Ctrl+Y và Ctrl+Shift+Z.
        steps.push(live::Step::Ctrl(u16::from(b'Y')));
        steps.push(live::Step::CtrlShift(u16::from(b'Z')));
        steps.extend(live::drag(at(30, 170), at(250, 180)));
        // Thao tác mới đã xóa ngăn làm lại: Ctrl+Y và nút Làm lại không làm gì.
        steps.push(live::Step::Ctrl(u16::from(b'Y')));
        steps.extend(live::click(l.bar_item(BarItem::Redo)));
        steps.extend(live::click(l.save()));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        let p = r.unwrap().unwrap();
        assert_eq!(p.shapes.len(), 3);
        let ys: Vec<f32> = p
            .shapes
            .iter()
            .map(|s| match s {
                Shape::Line { a, .. } => a.y,
                other => panic!("không phải Line: {other:?}"),
            })
            .collect();
        let y0 = sel.y as f32;
        assert_eq!(ys, vec![y0 + 20.0, y0 + 60.0, y0 + 170.0]);
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored size_flyout --test-threads=1"]
    fn size_flyout() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let l = live::layout(&shot, sel);
        let (a, b) = line_points(sel);
        let dpi = live::dpi_at(&shot, a);
        // Bấm nút Cỡ, bấm ô Lớn, vẽ Bút vẽ.
        let mut steps = live::click(l.item(SideItem::Size));
        steps.extend(live::click(l.size_cell(SizeLevel::Large)));
        steps.push(live::Step::Alive);
        steps.push(live::Step::Key(u16::from(b'P')));
        steps.extend(live::drag(a, b));
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        LAST_SIZE.with(|s| s.set(SizeLevel::Medium));
        assert!(ok);
        let p = r.unwrap().unwrap();
        match p.shapes.as_slice() {
            [Shape::Pen { width, .. }] => assert_eq!(*width, 5.0 * dpi as f32 / 96.0),
            other => panic!("không phải một Pen: {other:?}"),
        }
        // Bấm nút Cỡ rồi kéo trong vùng (không công cụ): chỉ đóng bảng, vùng không dời. Bảng đã
        // đóng thì L rồi kéo vẽ được một đường.
        let mut steps = live::click(l.item(SideItem::Size));
        steps.extend(live::drag(
            Point {
                x: sel.x + 30,
                y: sel.y + 40,
            },
            Point {
                x: sel.x + 150,
                y: sel.y + 90,
            },
        ));
        steps.push(live::Step::Alive);
        steps.push(live::Step::Key(u16::from(b'L')));
        steps.extend(live::drag(a, b));
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        let p = r.unwrap().unwrap();
        assert_eq!(p.rect, sel);
        match p.shapes.as_slice() {
            [Shape::Line { width, .. }] => assert_eq!(*width, dip_to_px(STROKE_DIP, dpi)),
            other => panic!("không phải một Line: {other:?}"),
        }
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored text_ctrl_z_unchanged --test-threads=1"]
    fn text_ctrl_z_unchanged() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let at = |dx, dy| Point {
            x: sel.x + dx,
            y: sel.y + dy,
        };
        // Hai nét, hoàn tác nét thứ hai (ngăn làm lại có một mục), mở ô chữ, gõ.
        let mut steps = vec![live::Step::Key(u16::from(b'P'))];
        steps.extend(live::drag(at(20, 20), at(120, 80)));
        steps.extend(live::drag(at(20, 100), at(120, 160)));
        steps.push(live::Step::Ctrl(u16::from(b'Z')));
        steps.push(live::Step::Key(u16::from(b'T')));
        steps.extend(live::click(at(160, 40)));
        steps.extend(live::chars("x"));
        // Ô mở: Ctrl+Y và Ctrl+Shift+Z không làm gì; Ctrl+Z xóa ô (chữ không được chốt).
        steps.push(live::Step::Ctrl(u16::from(b'Y')));
        steps.push(live::Step::CtrlShift(u16::from(b'Z')));
        steps.push(live::Step::Ctrl(u16::from(b'Z')));
        steps.push(live::Step::Alive);
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        let p = r.unwrap().unwrap();
        assert_eq!(p.shapes.len(), 1);
        match &p.shapes[0] {
            Shape::Pen { points, .. } => assert_eq!(points[0], live::pt(at(20, 20))),
            other => panic!("không phải Pen: {other:?}"),
        }
    }

    /// Kéo `a`→`b` qua `n` bước di chuột đều nhau.
    fn drag_n(a: Point, b: Point, n: i32) -> Vec<live::Step> {
        let mut v = vec![live::Step::Down(a)];
        for i in 1..=n {
            v.push(live::Step::Move(Point {
                x: a.x + (b.x - a.x) * i / n,
                y: a.y + (b.y - a.y) * i / n,
            }));
        }
        v.push(live::Step::Up(b));
        v
    }

    /// (a, b) của hình Rect duy nhất trong `shapes`.
    fn rect_ab(shapes: &[Shape]) -> (Pt, Pt) {
        match shapes {
            [Shape::Rect { a, b, .. }] => (*a, *b),
            other => panic!("không phải một Rect: {other:?}"),
        }
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored select_move_one_undo --test-threads=1"]
    fn select_move_one_undo() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let at = |dx, dy| Point {
            x: sel.x + dx,
            y: sel.y + dy,
        };
        let old = (live::pt(at(40, 40)), live::pt(at(140, 110)));
        let new = (live::pt(at(80, 60)), live::pt(at(180, 130)));
        let run = |tail: Vec<live::Step>| {
            let mut steps = vec![live::Step::Key(u16::from(b'R'))];
            steps.extend(live::drag(at(40, 40), at(140, 110)));
            steps.push(live::Step::Key(u16::from(b'V')));
            // Bấm trên cạnh trái, kéo +40,+20 qua 5 bước.
            steps.extend(drag_n(at(40, 70), at(80, 90), 5));
            steps.extend(tail);
            steps.push(live::Step::Ctrl(u16::from(b'S')));
            let (r, ok) = live::drive(&shot, Some(sel), steps);
            assert!(ok);
            r.unwrap().unwrap().shapes
        };
        assert_eq!(rect_ab(&run(vec![])), new);
        assert_eq!(rect_ab(&run(vec![live::Step::Ctrl(u16::from(b'Z'))])), old);
        assert_eq!(
            rect_ab(&run(vec![
                live::Step::Ctrl(u16::from(b'Z')),
                live::Step::Ctrl(u16::from(b'Y')),
            ])),
            new
        );
        let none = run(vec![
            live::Step::Ctrl(u16::from(b'Z')),
            live::Step::Ctrl(u16::from(b'Z')),
        ]);
        assert!(none.is_empty(), "{none:?}");
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored click_select_records_nothing --test-threads=1"]
    fn click_select_records_nothing() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let at = |dx, dy| Point {
            x: sel.x + dx,
            y: sel.y + dy,
        };
        let mut steps = vec![live::Step::Key(u16::from(b'P'))];
        steps.extend(live::drag(at(20, 20), at(120, 80)));
        steps.extend(live::drag(at(20, 100), at(120, 160)));
        steps.push(live::Step::Key(u16::from(b'V')));
        steps.extend(live::click(at(20, 20)));
        steps.push(live::Step::Ctrl(u16::from(b'Z')));
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        let p = r.unwrap().unwrap();
        match p.shapes.as_slice() {
            [Shape::Pen { points, .. }] => assert_eq!(points[0], live::pt(at(20, 20))),
            other => panic!("phải còn đúng nét thứ nhất: {other:?}"),
        }
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored esc_cancels_drag --test-threads=1"]
    fn esc_cancels_drag() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let at = |dx, dy| Point {
            x: sel.x + dx,
            y: sel.y + dy,
        };
        let old = (live::pt(at(40, 40)), live::pt(at(140, 110)));
        let run = |tail: Vec<live::Step>| {
            let mut steps = vec![live::Step::Key(u16::from(b'R'))];
            steps.extend(live::drag(at(40, 40), at(140, 110)));
            steps.push(live::Step::Key(u16::from(b'V')));
            steps.push(live::Step::Move(at(40, 70)));
            steps.push(live::Step::Down(at(40, 70)));
            steps.push(live::Step::Move(at(60, 80)));
            steps.push(live::Step::Move(at(90, 100)));
            steps.push(live::Step::Captured(true));
            steps.push(live::Step::Key(VK_ESCAPE.0));
            steps.push(live::Step::Alive);
            steps.push(live::Step::Captured(false));
            steps.push(live::Step::Up(at(90, 100)));
            steps.extend(tail);
            steps.push(live::Step::Ctrl(u16::from(b'S')));
            let (r, ok) = live::drive(&shot, Some(sel), steps);
            assert!(ok, "Esc phải hủy kéo, nhả chuột, giữ lớp phủ mở");
            r.unwrap().unwrap().shapes
        };
        assert_eq!(rect_ab(&run(vec![])), old);
        let none = run(vec![live::Step::Ctrl(u16::from(b'Z'))]);
        assert!(none.is_empty(), "Esc khi kéo không được ghi sổ: {none:?}");
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored topmost_selected --test-threads=1"]
    fn topmost_selected() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let at = |dx, dy| Point {
            x: sel.x + dx,
            y: sel.y + dy,
        };
        let mut steps = vec![live::Step::Key(u16::from(b'B'))];
        steps.extend(live::drag(at(20, 20), at(140, 120)));
        steps.extend(live::drag(at(80, 60), at(220, 170)));
        steps.push(live::Step::Key(u16::from(b'V')));
        steps.extend(drag_n(at(100, 90), at(130, 100), 3));
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        let p = r.unwrap().unwrap();
        let ab: Vec<(Pt, Pt)> = p
            .shapes
            .iter()
            .map(|s| match s {
                Shape::Blur { a, b, .. } => (*a, *b),
                other => panic!("không phải Blur: {other:?}"),
            })
            .collect();
        assert_eq!(
            ab,
            vec![
                (live::pt(at(20, 20)), live::pt(at(140, 120))),
                (live::pt(at(110, 70)), live::pt(at(250, 180))),
            ]
        );
    }

    /// a.y của các hình Line trong `shapes`.
    fn line_ys(shapes: &[Shape]) -> Vec<f32> {
        shapes
            .iter()
            .map(|s| match s {
                Shape::Line { a, .. } => a.y,
                other => panic!("không phải Line: {other:?}"),
            })
            .collect()
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored delete_and_undo --test-threads=1"]
    fn delete_and_undo() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let at = |dx, dy| Point {
            x: sel.x + dx,
            y: sel.y + dy,
        };
        let y0 = sel.y as f32;
        let run = |tail: Vec<live::Step>| {
            let mut steps = vec![live::Step::Key(u16::from(b'L'))];
            for i in 0..3 {
                steps.extend(live::drag(at(20, 20 + 40 * i), at(200, 30 + 40 * i)));
            }
            steps.push(live::Step::Key(u16::from(b'V')));
            // Giữa đường thứ hai (20,60)–(200,70).
            steps.extend(live::click(at(110, 65)));
            steps.push(live::Step::Key(VK_DELETE.0));
            steps.extend(tail);
            steps.push(live::Step::Ctrl(u16::from(b'S')));
            let (r, ok) = live::drive(&shot, Some(sel), steps);
            assert!(ok);
            r.unwrap().unwrap().shapes
        };
        assert_eq!(line_ys(&run(vec![])), vec![y0 + 20.0, y0 + 100.0]);
        assert_eq!(
            line_ys(&run(vec![live::Step::Ctrl(u16::from(b'Z'))])),
            vec![y0 + 20.0, y0 + 60.0, y0 + 100.0]
        );
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored resize_rect_handle --test-threads=1"]
    fn resize_rect_handle() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let at = |dx, dy| Point {
            x: sel.x + dx,
            y: sel.y + dy,
        };
        let run = |to: Point, shift: bool| {
            let mut steps = vec![live::Step::Key(u16::from(b'R'))];
            steps.extend(live::drag(at(40, 40), at(140, 110)));
            steps.push(live::Step::Key(u16::from(b'V')));
            steps.extend(live::click(at(40, 70)));
            steps.push(live::Step::Shift(shift));
            steps.extend(drag_n(at(140, 110), to, 4));
            steps.push(live::Step::Shift(false));
            steps.push(live::Step::Ctrl(u16::from(b'S')));
            let (r, ok) = live::drive(&shot, Some(sel), steps);
            assert!(ok);
            rect_ab(&r.unwrap().unwrap().shapes)
        };
        assert_eq!(
            run(at(200, 150), false),
            (live::pt(at(40, 40)), live::pt(at(200, 150)))
        );
        // Shift: giữ tỉ lệ 100:70, hệ số lớn hơn (x: 200/100 = 2) thắng.
        assert_eq!(
            run(at(240, 130), true),
            (live::pt(at(40, 40)), live::pt(at(240, 180)))
        );
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored blur_move_save_covered --test-threads=1"]
    fn blur_move_save_covered() {
        use darkshot_core::annotate::{blur_pixel_rect, redact_color};
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let at = |dx, dy| Point {
            x: sel.x + dx,
            y: sel.y + dy,
        };
        let mut steps = vec![live::Step::Key(u16::from(b'B'))];
        steps.extend(live::drag(at(20, 20), at(100, 80)));
        steps.push(live::Step::Key(u16::from(b'V')));
        // Dời +160,+100 (hình vẫn được chọn, khung đang hiện khi Lưu).
        steps.extend(drag_n(at(60, 50), at(220, 150), 5));
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        let p = r.unwrap().unwrap();
        let (a, b) = (live::pt(at(180, 120)), live::pt(at(260, 180)));
        let rect = blur_pixel_rect(a, b);
        let want = redact_color(&shot.image, rect).unwrap();
        assert_eq!(p.shapes, vec![Shape::Blur { a, b, color: want }]);
        let out = crate::picked_image(&shot.image, p.rect, &p.shapes).unwrap();
        let px = |x: i32, y: i32| {
            let i = (((y - sel.y) as u32 * out.width + (x - sel.x) as u32) * 4) as usize;
            Rgb(out.bgra[i + 2], out.bgra[i + 1], out.bgra[i])
        };
        let src = |x: i32, y: i32| {
            let i = ((y as u32 * shot.image.width + x as u32) * 4) as usize;
            Rgb(
                shot.image.bgra[i + 2],
                shot.image.bgra[i + 1],
                shot.image.bgra[i],
            )
        };
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                assert_eq!(px(x, y), want, "({x},{y})");
            }
        }
        // Khung chọn (ACCENT) không vào ảnh ra: điểm ảnh ACCENT chỉ có ở chỗ ảnh gốc đã ACCENT.
        for y in sel.y..sel.bottom() {
            for x in sel.x..sel.right() {
                if px(x, y) == ACCENT {
                    assert_eq!(src(x, y), ACCENT, "({x},{y})");
                }
            }
        }
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored restyle_selected --test-threads=1"]
    fn restyle_selected() {
        use darkshot_core::annotate::{blur_pixel_rect, redact_color};
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let l = live::layout(&shot, sel);
        let at = |dx, dy| Point {
            x: sel.x + dx,
            y: sel.y + dy,
        };
        let dpi = live::dpi_at(&shot, at(20, 20));
        let run = |tail: Vec<live::Step>| {
            // Màu mặc định đặt rõ (ô 0), vẽ Bút vẽ, chọn nét, đổi mức Rất lớn rồi màu ô 5.
            let mut steps = live::click(l.item(SideItem::Color));
            steps.extend(live::click(l.cell(0)));
            steps.push(live::Step::Key(u16::from(b'P')));
            steps.extend(live::drag(at(20, 20), at(120, 80)));
            steps.push(live::Step::Key(u16::from(b'V')));
            steps.extend(live::click(at(20, 20)));
            steps.extend(live::click(l.item(SideItem::Size)));
            steps.extend(live::click(l.size_cell(SizeLevel::Huge)));
            steps.extend(live::click(l.item(SideItem::Color)));
            steps.extend(live::click(l.cell(5)));
            steps.extend(tail);
            steps.push(live::Step::Ctrl(u16::from(b'S')));
            let (r, ok) = live::drive(&shot, Some(sel), steps);
            LAST_SIZE.with(|s| s.set(SizeLevel::Medium));
            assert!(ok);
            match r.unwrap().unwrap().shapes.as_slice() {
                [Shape::Pen { width, color, .. }] => (*width, *color),
                other => panic!("không phải một Pen: {other:?}"),
            }
        };
        assert_eq!(run(vec![]), (8.0 * dpi as f32 / 96.0, PALETTE[5]));
        assert_eq!(
            run(vec![
                live::Step::Ctrl(u16::from(b'Z')),
                live::Step::Ctrl(u16::from(b'Z')),
            ]),
            (dip_to_px(STROKE_DIP, dpi), PALETTE[0])
        );
        // Làm mờ được chọn: đổi màu không đổi hình, không ghi sổ (Ctrl+Z bỏ chính vùng che).
        let blur_run = |tail: Vec<live::Step>| {
            let mut steps = vec![live::Step::Key(u16::from(b'B'))];
            steps.extend(live::drag(at(20, 20), at(120, 80)));
            steps.push(live::Step::Key(u16::from(b'V')));
            steps.extend(live::click(at(60, 50)));
            steps.extend(live::click(l.item(SideItem::Color)));
            steps.extend(live::click(l.cell(9)));
            steps.extend(tail);
            steps.push(live::Step::Ctrl(u16::from(b'S')));
            let (r, ok) = live::drive(&shot, Some(sel), steps);
            assert!(ok);
            r.unwrap().unwrap().shapes
        };
        let want = Shape::Blur {
            a: live::pt(at(20, 20)),
            b: live::pt(at(120, 80)),
            color: redact_color(
                &shot.image,
                blur_pixel_rect(live::pt(at(20, 20)), live::pt(at(120, 80))),
            )
            .unwrap(),
        };
        assert_eq!(blur_run(vec![]), vec![want]);
        assert!(blur_run(vec![live::Step::Ctrl(u16::from(b'Z'))]).is_empty());
        LAST_COLOR.with(|c| c.set(DEFAULT_COLOR));
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored no_tool_behaves_as_before --test-threads=1"]
    fn no_tool_behaves_as_before() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let at = |dx, dy| Point {
            x: sel.x + dx,
            y: sel.y + dy,
        };
        let fresh = Rect::from_points(at(-80, -80), at(70, 40));
        // Không công cụ (cả khi bật rồi tắt Chọn), không hình: kéo ngoài vùng tạo vùng mới.
        for prefix in [vec![], vec![u16::from(b'V'), u16::from(b'V')]] {
            let mut steps: Vec<live::Step> = prefix.into_iter().map(live::Step::Key).collect();
            steps.extend(live::drag(at(-80, -80), at(70, 40)));
            steps.push(live::Step::Ctrl(u16::from(b'S')));
            let (r, ok) = live::drive(&shot, Some(sel), steps);
            assert!(ok);
            assert_eq!(
                r,
                Ok(Some(Picked {
                    action: Action::Save,
                    rect: fresh,
                    shapes: Vec::new(),
                }))
            );
        }
        // Esc đóng lớp phủ.
        let (r, ok) = live::drive(&shot, Some(sel), vec![live::Step::Key(VK_ESCAPE.0)]);
        assert!(ok);
        assert_eq!(r, Ok(None));
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored tool_switch_deselects --test-threads=1"]
    fn tool_switch_deselects() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let at = |dx, dy| Point {
            x: sel.x + dx,
            y: sel.y + dy,
        };
        let old = (live::pt(at(40, 40)), live::pt(at(140, 110)));
        // Chọn hình, đổi sang P rồi về V: không còn hình chọn, Delete không xóa gì.
        let mut steps = vec![live::Step::Key(u16::from(b'R'))];
        steps.extend(live::drag(at(40, 40), at(140, 110)));
        steps.push(live::Step::Key(u16::from(b'V')));
        steps.extend(live::click(at(40, 70)));
        steps.push(live::Step::Key(u16::from(b'P')));
        steps.push(live::Step::Key(u16::from(b'V')));
        steps.push(live::Step::Key(VK_DELETE.0));
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        assert_eq!(rect_ab(&r.unwrap().unwrap().shapes), old);
        // Đối chứng: không đổi công cụ thì Delete xóa; Esc lần đầu chỉ bỏ chọn (lớp phủ còn mở,
        // Delete sau đó không xóa gì).
        let mut steps = vec![live::Step::Key(u16::from(b'R'))];
        steps.extend(live::drag(at(40, 40), at(140, 110)));
        steps.push(live::Step::Key(u16::from(b'V')));
        steps.extend(live::click(at(40, 70)));
        steps.push(live::Step::Key(VK_ESCAPE.0));
        steps.push(live::Step::Alive);
        steps.push(live::Step::Key(VK_DELETE.0));
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        assert_eq!(rect_ab(&r.unwrap().unwrap().shapes), old);
        let mut steps = vec![live::Step::Key(u16::from(b'R'))];
        steps.extend(live::drag(at(40, 40), at(140, 110)));
        steps.push(live::Step::Key(u16::from(b'V')));
        steps.extend(live::click(at(40, 70)));
        steps.push(live::Step::Key(VK_DELETE.0));
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        assert!(r.unwrap().unwrap().shapes.is_empty());
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored off_center_grip_click_records_nothing --test-threads=1"]
    fn off_center_grip_click_records_nothing() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let at = |dx, dy| Point {
            x: sel.x + dx,
            y: sel.y + dy,
        };
        let old = (live::pt(at(40, 40)), live::pt(at(140, 110)));
        let run = |tail: Vec<live::Step>| {
            let mut steps = vec![live::Step::Key(u16::from(b'R'))];
            steps.extend(live::drag(at(40, 40), at(140, 110)));
            steps.push(live::Step::Key(u16::from(b'V')));
            steps.extend(live::click(at(65, 40)));
            steps.extend(tail);
            steps.push(live::Step::Ctrl(u16::from(b'S')));
            let (r, ok) = live::drive(&shot, Some(sel), steps);
            assert!(ok);
            r.unwrap().unwrap().shapes
        };
        // Bấm-thả lệch tâm tay nắm SE (+6,+6): hình không đổi, không ghi sổ.
        assert_eq!(rect_ab(&run(live::click(at(146, 116)))), old);
        let mut tail = live::click(at(146, 116));
        tail.push(live::Step::Ctrl(u16::from(b'Z')));
        let none = run(tail);
        assert!(none.is_empty(), "Ctrl+Z phải bỏ chính hình: {none:?}");
        // Kéo từ điểm lệch tâm: góc dời đúng bằng quãng chuột đi, không giật theo điểm bấm.
        assert_eq!(
            rect_ab(&run(drag_n(at(146, 116), at(166, 136), 4))),
            (live::pt(at(40, 40)), live::pt(at(160, 130)))
        );
        // Đường thẳng: bấm-thả cách đầu mút 6 px theo trục, đầu mút không nhảy.
        let mut steps = vec![live::Step::Key(u16::from(b'L'))];
        steps.extend(live::drag(at(40, 60), at(200, 60)));
        steps.push(live::Step::Key(u16::from(b'V')));
        steps.extend(live::click(at(120, 60)));
        steps.extend(live::click(at(206, 60)));
        steps.push(live::Step::Ctrl(u16::from(b'Z')));
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        let shapes = r.unwrap().unwrap().shapes;
        assert!(shapes.is_empty(), "{shapes:?}");
        // Vòng số nhỏ: bấm-thả đúng tâm (gần tay nắm góc) là di chuyển, bán kính giữ nguyên.
        let mut steps = live::click(live::layout(&shot, sel).item(SideItem::Size));
        steps.extend(live::click(
            live::layout(&shot, sel).size_cell(SizeLevel::Small),
        ));
        steps.push(live::Step::Key(u16::from(b'N')));
        steps.extend(live::click(at(100, 100)));
        steps.push(live::Step::Key(u16::from(b'V')));
        steps.extend(live::click(at(100, 100)));
        steps.extend(live::click(at(100, 100)));
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        LAST_SIZE.with(|s| s.set(SizeLevel::Medium));
        assert!(ok);
        match r.unwrap().unwrap().shapes.as_slice() {
            [Shape::Step { center, radius, .. }] => {
                assert_eq!(*center, live::pt(at(100, 100)));
                assert_eq!(*radius, dip_to_px(10.0, live::dpi_at(&shot, at(100, 100))));
            }
            other => panic!("không phải một Step: {other:?}"),
        }
    }

    /// Loại của từng hình (để so gọn).
    fn kinds(shapes: &[Shape]) -> Vec<&'static str> {
        shapes
            .iter()
            .map(|s| match s {
                Shape::Pen { .. } => "Pen",
                Shape::Line { .. } => "Line",
                Shape::Arrow { .. } => "Arrow",
                Shape::Rect { .. } => "Rect",
                Shape::Ellipse { .. } => "Ellipse",
                Shape::Marker { .. } => "Marker",
                Shape::Blur { .. } => "Blur",
                Shape::Text { .. } => "Text",
                Shape::Step { .. } => "Step",
                Shape::Box { .. } => "Box",
                Shape::Label { .. } => "Label",
            })
            .collect()
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored shapes_button --test-threads=1"]
    fn shapes_button() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let l = live::layout(&shot, sel);
        let at = |dx, dy| Point {
            x: sel.x + dx,
            y: sel.y + dy,
        };
        LAST_SHAPE.with(|c| c.set(Tool::Arrow));
        // 1. Chưa có công cụ, bấm nút Hình: bật Mũi tên (mặc định), không mở bảng nào (kéo vẽ
        // được ngay).
        let mut steps = live::click(l.item(SideItem::Shapes));
        steps.extend(live::drag(at(20, 20), at(120, 40)));
        // 2. Bấm nút Hình lần nữa: bảng Hình mở. 3. Bấm ô Hình elip: Ellipse, bảng đóng; kéo vẽ.
        steps.extend(live::click(l.item(SideItem::Shapes)));
        steps.extend(live::click(l.shape_cell(Tool::Ellipse)));
        steps.push(live::Step::Alive);
        steps.extend(live::drag(at(20, 60), at(120, 100)));
        // 4. P rồi nút Hình: Hình elip (hình nhớ).
        steps.push(live::Step::Key(u16::from(b'P')));
        steps.extend(live::click(l.item(SideItem::Shapes)));
        steps.extend(live::drag(at(150, 20), at(250, 60)));
        // 5. L, P, nút Hình: Đường thẳng.
        steps.push(live::Step::Key(u16::from(b'L')));
        steps.push(live::Step::Key(u16::from(b'P')));
        steps.extend(live::click(l.item(SideItem::Shapes)));
        steps.extend(live::drag(at(150, 120), at(250, 150)));
        // 6. Lưu.
        steps.extend(live::click(l.save()));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        let last = LAST_SHAPE.with(|c| c.get());
        LAST_SHAPE.with(|c| c.set(Tool::Arrow));
        assert!(ok);
        let p = r.unwrap().unwrap();
        assert_eq!((p.action, p.rect), (Action::Save, sel));
        assert_eq!(
            kinds(&p.shapes),
            vec!["Arrow", "Ellipse", "Ellipse", "Line"]
        );
        // Hình nhớ giữ qua các lần mở lớp phủ.
        assert_eq!(last, Tool::Line);
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored shapes_flyout_esc_and_outside --test-threads=1"]
    fn shapes_flyout_esc_and_outside() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let l = live::layout(&shot, sel);
        let at = |dx, dy| Point {
            x: sel.x + dx,
            y: sel.y + dy,
        };
        LAST_SHAPE.with(|c| c.set(Tool::Arrow));
        // Bảng Hình mở, Esc: bảng đóng, lớp phủ còn mở; kéo vẽ được Mũi tên.
        let mut steps = live::click(l.item(SideItem::Shapes));
        steps.extend(live::click(l.item(SideItem::Shapes)));
        steps.push(live::Step::Key(VK_ESCAPE.0));
        steps.push(live::Step::Alive);
        steps.extend(live::drag(at(20, 20), at(120, 40)));
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        let p = r.unwrap().unwrap();
        assert_eq!(kinds(&p.shapes), vec!["Arrow"]);
        // Mở bảng, bấm (kéo) ngoài bảng trong vùng chọn: chỉ đóng bảng, cú bấm bị nuốt: vùng
        // không đổi, không có hình mới.
        let mut steps = live::click(l.item(SideItem::Shapes));
        steps.extend(live::click(l.item(SideItem::Shapes)));
        steps.extend(live::drag(at(20, 20), at(120, 40)));
        steps.push(live::Step::Alive);
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        let p = r.unwrap().unwrap();
        assert_eq!(p.rect, sel);
        assert!(p.shapes.is_empty(), "{:?}", kinds(&p.shapes));
        // Đang Mũi tên, mở bảng Cỡ rồi bấm nút Hình hai lần: lần đầu chỉ đóng bảng Cỡ, lần hai mở
        // bảng Hình (chỉ bảng Hình mở): bấm ô Hình elip được, kéo vẽ elip cỡ Vừa.
        let dpi = live::dpi_at(&shot, at(20, 60));
        let mut steps = vec![live::Step::Key(u16::from(b'A'))];
        steps.extend(live::click(l.item(SideItem::Size)));
        steps.extend(live::click(l.item(SideItem::Shapes)));
        steps.extend(live::click(l.item(SideItem::Shapes)));
        steps.extend(live::click(l.shape_cell(Tool::Ellipse)));
        steps.extend(live::drag(at(20, 60), at(120, 100)));
        steps.push(live::Step::Ctrl(u16::from(b'S')));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        LAST_SHAPE.with(|c| c.set(Tool::Arrow));
        assert!(ok);
        let p = r.unwrap().unwrap();
        match p.shapes.as_slice() {
            [Shape::Ellipse { width, .. }] => assert_eq!(*width, dip_to_px(STROKE_DIP, dpi)),
            other => panic!("không phải một Ellipse: {:?}", kinds(other)),
        }
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored undo_redo_on_action_bar --test-threads=1"]
    fn undo_redo_on_action_bar() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let l = live::layout(&shot, sel);
        let at = |dx, dy| Point {
            x: sel.x + dx,
            y: sel.y + dy,
        };
        let two_strokes = || {
            let mut steps = vec![live::Step::Key(u16::from(b'P'))];
            steps.extend(live::drag(at(20, 20), at(120, 40)));
            steps.extend(live::drag(at(20, 80), at(120, 120)));
            steps
        };
        let run = |tail: Vec<live::Step>| {
            let mut steps = two_strokes();
            steps.extend(tail);
            steps.extend(live::click(l.save()));
            let (r, ok) = live::drive(&shot, Some(sel), steps);
            assert!(ok);
            r.unwrap().unwrap()
        };
        let undo = || live::click(l.bar_item(BarItem::Undo));
        let redo = || live::click(l.bar_item(BarItem::Redo));
        // 1–2. Vẽ 2 nét, Hoàn tác trên thanh ngang: còn 1 hình; lớp phủ không đóng.
        let mut tail = undo();
        tail.push(live::Step::Alive);
        let p = run(tail);
        assert_eq!((p.action, p.shapes.len()), (Action::Save, 1));
        // 3. Làm lại: lại 2 hình.
        let mut tail = undo();
        tail.extend(redo());
        tail.push(live::Step::Alive);
        assert_eq!(run(tail).shapes.len(), 2);
        // 4–5. Hoàn tác 2 lần, bấm thêm (nút mờ) không làm gì, lớp phủ vẫn mở; Lưu.
        let mut tail = undo();
        tail.extend(undo());
        tail.extend(undo());
        tail.push(live::Step::Alive);
        let p = run(tail);
        assert_eq!(p.shapes.len(), 0);
        assert_eq!(p.action, Action::Save);
    }

    /// Số điểm ảnh trong `r` (tọa độ ảnh) của màn hình thật có một kênh màu lệch quá 8 mức so với
    /// ảnh `shot` (`bright`) hoặc nền tối của nó. Chạy ở luồng phụ của `live::drive`: không panic
    /// (lớp phủ toàn màn hình sẽ treo), lỗi trả `Err`.
    fn screen_diff(
        shot: &crate::capture::Screenshot,
        r: Rect,
        bright: bool,
    ) -> Result<usize, String> {
        let now = crate::capture::capture_virtual_screen(false).map_err(|e| e.to_string())?;
        let w = shot.image.width as i32;
        let geo = |s: &crate::capture::Screenshot| (s.image.width, s.image.height, s.origin);
        if geo(&now) != geo(shot) {
            return Err("màn hình ảo đổi kích thước hoặc gốc".to_string());
        }
        let mut n = 0;
        for y in r.y..r.bottom() {
            for x in r.x..r.right() {
                let i = ((y * w + x) * 4) as usize;
                let differs = (0..3).any(|c| {
                    let a = shot.image.bgra[i + c];
                    let want = if bright { a } else { a >> 1 };
                    want.abs_diff(now.image.bgra[i + c]) > 8
                });
                n += usize::from(differs);
            }
        }
        Ok(n)
    }

    /// Bấm (không kéo) chọn ứng viên khi viên nhắc đang hiện: trên MÀN HÌNH THẬT ô viên nhắc phải
    /// trở về nền tối ngay (không đợi lần di chuột kế), vùng ứng viên sáng. `real_cursor`: con trỏ
    /// thật đặt tại điểm bấm (như chuột thật); `false`: con trỏ thật ở góc xa (như demo gửi thông
    /// điệp).
    fn click_clears_hint_on_screen(real_cursor: bool) {
        use std::sync::{Arc, Mutex};
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let mut shot = live_setup();
        let m0 = shot.monitors[0];
        let win = Rect {
            x: m0.x + m0.w / 4,
            y: m0.y + m0.h / 3,
            w: m0.w / 2,
            h: m0.h / 3,
        };
        shot.windows = Some(vec![win]);
        let at = Point {
            x: win.x + win.w / 2,
            y: win.y + win.h / 2,
        };
        let hint = {
            let (w, h) = (shot.image.width as i32, shot.image.height as i32);
            let mut s = State::new(&shot, w, h, None, &CustomColors::default(), false).unwrap();
            s.mouse = at;
            s.hint().expect("viên nhắc hiện khi rê trên ứng viên").0
        };
        let far = Point {
            x: m0.right() - 20,
            y: m0.bottom() - 20,
        };
        let shot = Arc::new(shot);
        let log = Arc::new(Mutex::new(Vec::new()));
        let probe = |label: &'static str, r: Rect, bright: bool| {
            let (sh, lg) = (shot.clone(), log.clone());
            live::Step::Probe(Box::new(move || match screen_diff(&sh, r, bright) {
                Ok(n) => {
                    if let Ok(mut l) = lg.lock() {
                        l.push((label, n));
                    }
                    true
                }
                Err(e) => {
                    eprintln!("probe {label}: {e}");
                    false
                }
            }))
        };
        let inner = inflate(win, -40);
        // Dải nền dưới ứng viên: tối khi lớp phủ đang hiện và xem trước ứng viên `win` (chỉ kiểm khi
        // con trỏ thật ở điểm bấm; với con trỏ thật ở góc xa, WM_MOUSEMOVE hệ thống tại đó có thể đổi
        // xem trước sang ứng viên cả màn hình, đúng kịch bản demo).
        let below = Rect {
            x: win.x,
            y: win.bottom() + 40,
            w: win.w,
            h: 40,
        };
        let steps = vec![
            live::Step::Cursor(if real_cursor { at } else { far }),
            live::Step::Move(at),
            probe("hint before", hint, false),
            probe("hint before bright", hint, true),
            probe("below before", below, false),
            live::Step::Down(at),
            live::Step::Up(at),
            probe("hint after", hint, false),
            probe("window after", inner, true),
            probe("hint later", hint, false),
            live::Step::Alive,
            live::Step::Close,
        ];
        let (r, ok) = live::drive(&shot, None, steps);
        let log = log.lock().unwrap().clone();
        eprintln!("hint {hint:?} win {win:?} log {log:?}");
        // `ok` sai: lớp phủ mất kích hoạt và tự đóng giữa chừng, hoặc một phép đo lỗi (chạy lại).
        assert!(ok, "lớp phủ đóng giữa chừng hoặc phép đo lỗi: {log:?}");
        assert!(matches!(r, Ok(None)));
        let get = |l: &str| log.iter().find(|(k, _)| *k == l).unwrap().1;
        // Tiền đề: lớp phủ đang hiện trên cùng với viên nhắc (ô viên nhắc khác cả ảnh gốc lẫn nền
        // tối). Không thì do môi trường (cửa sổ khác che, chuột thật bị di): chạy lại.
        assert!(
            get("hint before bright") > 50,
            "lớp phủ không hiện: {log:?}"
        );
        assert!(get("hint before") > 50, "{log:?}");
        if real_cursor {
            // Con trỏ thật ở điểm bấm: xem trước đúng ứng viên `win`.
            assert_eq!(
                get("below before"),
                0,
                "xem trước không phải `win`: {log:?}"
            );
        }
        // Vùng ứng viên đã được chọn (sáng).
        assert!(get("window after") < 50, "{log:?}");
        // Viên nhắc đã biến mất ngay sau khi nhả chuột.
        assert_eq!(get("hint after"), 0, "{log:?}");
        assert_eq!(get("hint later"), 0, "{log:?}");
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored click_clears_hint_ --test-threads=1"]
    fn click_clears_hint_real_cursor() {
        click_clears_hint_on_screen(true);
    }

    #[test]
    #[ignore = "mở lớp phủ thật; chạy tay: cargo test -p darkshot-windows -- --ignored click_clears_hint_ --test-threads=1"]
    fn click_clears_hint_posted_only() {
        click_clears_hint_on_screen(false);
    }

    /// Đinh ghim: đầu, thân, mũi kim nghiêng 45° (đầu trên phải, mũi dưới trái), nằm gọn trong
    /// lưới 24×24 với lề như các biểu tượng khác.
    #[test]
    fn pin_glyph_fits_grid_and_tilts() {
        let (outline, needle) = pin_glyph();
        for (x, y) in outline.iter().chain(needle.iter()) {
            assert!(
                (2.0..=22.0).contains(x) && (2.0..=22.0).contains(y),
                "({x},{y})"
            );
        }
        let [base, tip] = needle;
        // Mũi kim chéo 45° xuống trái.
        assert!(tip.0 < base.0 && tip.1 > base.1);
        assert!(((base.0 - tip.0) - (tip.1 - base.1)).abs() < 0.01);
        // Đầu đinh (điểm xa mũi nhất theo hướng kim) ở phía trên phải.
        let reach = |q: (f32, f32)| (q.0 - tip.0) + (tip.1 - q.1);
        let far = outline
            .iter()
            .copied()
            .fold(tip, |a, q| if reach(q) > reach(a) { q } else { a });
        assert!(far.0 > 12.0 && far.1 < 12.0, "{far:?}");
        // Gần cân giữa lưới.
        let all: Vec<(f32, f32)> = outline.iter().chain(needle.iter()).copied().collect();
        let (x0, x1) = all
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), q| (a.min(q.0), b.max(q.0)));
        let (y0, y1) = all
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), q| (a.min(q.1), b.max(q.1)));
        assert!(((x0 + x1) / 2.0 - 12.0).abs() < 1.0, "{x0} {x1}");
        assert!(((y0 + y1) / 2.0 - 12.0).abs() < 1.0, "{y0} {y1}");
    }

    /// Nhận dạng chữ: bốn góc khung (mỗi góc hai nét vuông góc) và chữ "T" ở giữa, trong lưới 24×24
    /// với lề như các biểu tượng khác; khung và chữ không chạm nhau.
    #[test]
    fn ocr_glyph_corners_and_t() {
        let (corners, t_bar, t_stem) = ocr_glyph();
        let inside = |(x, y): GridPt| (2.0..=22.0).contains(&x) && (2.0..=22.0).contains(&y);
        for c in &corners {
            assert!(c.iter().all(|q| inside(*q)), "{c:?}");
            // Góc: hai đoạn, một ngang một dọc, gặp nhau ở điểm giữa.
            let [a, m, b] = *c;
            assert!((a.0 - m.0).abs() < 0.01 || (a.1 - m.1).abs() < 0.01);
            assert!((b.0 - m.0).abs() < 0.01 || (b.1 - m.1).abs() < 0.01);
            assert!(((a.0 - m.0).abs() < 0.01) != ((b.0 - m.0).abs() < 0.01));
        }
        // Bốn góc ở bốn phía của tâm lưới.
        let mut quads: Vec<(bool, bool)> = corners
            .iter()
            .map(|c| (c[1].0 > 12.0, c[1].1 > 12.0))
            .collect();
        quads.sort();
        quads.dedup();
        assert_eq!(quads.len(), 4);
        // Chữ T: gạch ngang trên, thân dọc từ giữa gạch, cân giữa lưới, nằm trong khung.
        assert!((t_bar[0].1 - t_bar[1].1).abs() < 0.01);
        assert!((t_stem[0].0 - t_stem[1].0).abs() < 0.01);
        assert!(((t_bar[0].0 + t_bar[1].0) / 2.0 - t_stem[0].0).abs() < 0.01);
        assert!((t_stem[0].0 - 12.0).abs() < 0.01);
        assert!((t_stem[0].1 - t_bar[0].1).abs() < 0.01 && t_stem[1].1 > t_stem[0].1);
        let (lo, hi) = corners
            .iter()
            .flatten()
            .fold((f32::MAX, f32::MIN), |(a, b), q| {
                (a.min(q.0.min(q.1)), b.max(q.0.max(q.1)))
            });
        for q in t_bar.iter().chain(t_stem.iter()) {
            assert!(q.0 > lo + 1.0 && q.0 < hi - 1.0 && q.1 > lo + 1.0 && q.1 < hi - 1.0);
        }
    }

    #[test]
    #[ignore = "mở lớp phủ thật và một ảnh ghim; chạy tay: cargo test -p darkshot-windows -- --ignored ctrl_p_pins_selection_at_its_corner --test-threads=1"]
    fn ctrl_p_pins_selection_at_its_corner() {
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let shot = live_setup();
        let sel = live::test_sel(&shot);
        let inside = |dx, dy| Point {
            x: sel.x + dx,
            y: sel.y + dy,
        };
        // Có hình đánh dấu, rồi Ctrl+P đang gõ chữ: ô được chốt, ảnh ghim có cả hai hình.
        let mut steps = vec![live::Step::Key(u16::from(b'R'))];
        steps.extend(live::drag(inside(20, 20), inside(120, 90)));
        steps.push(live::Step::Key(u16::from(b'T')));
        steps.extend(live::click(inside(150, 120)));
        steps.extend(live::chars("ab"));
        steps.push(live::Step::Ctrl(0x50));
        let (r, ok) = live::drive(&shot, Some(sel), steps);
        assert!(ok);
        let p = r.unwrap().unwrap();
        assert_eq!((p.action, p.rect, p.shapes.len()), (Action::Pin, sel, 2));
        assert!(matches!(p.shapes[1], Shape::Text { .. }));
        let image = crate::picked_image(&shot.image, p.rect, &p.shapes).unwrap();
        assert_eq!((image.width, image.height), (sel.w as u32, sel.h as u32));
        let at = crate::pin_origin(p.rect, shot.origin);
        assert_eq!(
            at,
            Point {
                x: sel.x + shot.origin.x,
                y: sel.y + shot.origin.y
            }
        );
        crate::pin::close_all();
        crate::pin::open(image, crate::pin::PinPlace::At(at)).unwrap();
        let rects = crate::pin::image_rects();
        crate::pin::close_all();
        assert_eq!(crate::pin::count(), 0);
        assert_eq!(rects.len(), 1);
        assert_eq!((rects[0].x, rects[0].y), (at.x, at.y));
        assert_eq!((rects[0].w, rects[0].h), (sel.w, sel.h));
        // Không vùng chọn: Ctrl+P không làm gì, lớp phủ vẫn mở.
        let steps = vec![live::Step::Ctrl(0x50), live::Step::Alive];
        let (r, _) = live::drive(&shot, None, steps);
        assert!(r.is_ok_and(|p| p.is_none()));
    }

    /// Trạng thái lớp phủ dựng không cần cửa sổ: gợi ý cửa sổ (Task 62).
    mod snap_state {
        use super::*;
        use crate::capture::{Screenshot, GDI_TEST_LOCK};

        const W: i32 = 800;
        const H: i32 = 600;
        const WIN: Rect = Rect {
            x: 100,
            y: 100,
            w: 300,
            h: 200,
        };
        const MON: Rect = Rect {
            x: 0,
            y: 0,
            w: W,
            h: H,
        };

        fn p(x: i32, y: i32) -> Point {
            Point { x, y }
        }

        fn shot(windows: Option<Vec<Rect>>) -> Screenshot {
            Screenshot {
                image: Image {
                    width: W as u32,
                    height: H as u32,
                    bgra: vec![0x80; (W * H * 4) as usize],
                },
                origin: p(0, 0),
                monitors: vec![MON],
                // Góc dưới trái, xa viên nhắc.
                cursor: p(10, H - 10),
                windows,
            }
        }

        fn state(shot: &Screenshot, initial: Option<Rect>) -> State {
            let mut s = State::new(shot, W, H, initial, &CustomColors::default(), false).unwrap();
            // DPI cố định: `State::new` đọc DPI của màn hình THẬT (MonitorFromPoint), nên vị trí và
            // cỡ viên nhắc trong kiểm thử không được phụ thuộc cấu hình màn hình của máy chạy.
            s.dpis = vec![96; s.dpis.len()];
            s
        }

        fn click(s: &mut State, at: Point) {
            s.on_move(at);
            let fx = s.on_down(at);
            assert!(fx.capture);
            s.on_up(at);
        }

        #[test]
        fn hover_then_click_selects_candidate() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(Some(vec![WIN]));
            let mut s = state(&sh, None);
            s.on_move(p(200, 150));
            assert_eq!(s.snap_hover, Some(WIN));
            assert!(s.toolbar().is_none());
            click(&mut s, p(200, 150));
            assert_eq!(s.sel.rect(), Some(WIN));
            assert_eq!(s.snap_hover, None);
            // Như khi kéo xong: có thanh công cụ.
            assert!(s.toolbar().is_some());
        }

        #[test]
        fn new_state_hovers_candidate_under_cursor() {
            let _gdi = GDI_TEST_LOCK.lock();
            // Khung đầu tiên đã có ứng viên dưới con trỏ: không chớp tối trước lần di chuột đầu.
            let mut sh = shot(Some(vec![WIN]));
            sh.cursor = p(200, 150);
            let s = state(&sh, None);
            assert_eq!(s.snap_hover, Some(WIN));
            assert_eq!(s.shown_rect(), Some(WIN));
            // Tọa độ màn hình đổi sang tọa độ ảnh (gốc màn hình ảo âm).
            let mut sh = shot(Some(vec![WIN]));
            sh.origin = p(-1000, -50);
            sh.cursor = p(200 - 1000, 150 - 50);
            assert_eq!(state(&sh, None).snap_hover, Some(WIN));
            // Ngoài cửa sổ: ứng viên là màn hình.
            let mut sh = shot(Some(vec![WIN]));
            sh.cursor = p(600, 500);
            assert_eq!(state(&sh, None).snap_hover, Some(MON));
            // Có khung nhớ sẵn hoặc gợi ý tắt: không xem trước.
            let mut sh = shot(Some(vec![WIN]));
            sh.cursor = p(200, 150);
            let sel = Rect {
                x: 500,
                y: 400,
                w: 100,
                h: 80,
            };
            assert_eq!(state(&sh, Some(sel)).snap_hover, None);
            let mut sh = shot(None);
            sh.cursor = p(200, 150);
            assert_eq!(state(&sh, None).snap_hover, None);
        }

        #[test]
        fn click_on_background_selects_monitor() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(Some(vec![WIN]));
            let mut s = state(&sh, None);
            s.on_move(p(600, 500));
            assert_eq!(s.snap_hover, Some(MON));
            click(&mut s, p(600, 500));
            assert_eq!(s.sel.rect(), Some(MON));
        }

        #[test]
        fn small_jitter_still_counts_as_click() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(Some(vec![WIN]));
            let mut s = state(&sh, None);
            s.on_move(p(200, 150));
            s.on_down(p(200, 150));
            s.on_move(p(202, 151));
            s.on_up(p(202, 151));
            assert_eq!(s.sel.rect(), Some(WIN));
        }

        #[test]
        fn jitter_selects_candidate_at_press_point() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(Some(vec![WIN]));
            let mut s = state(&sh, None);
            // Bấm trong WIN sát mép phải, nhả ngoài WIN 2 px: ứng viên lấy tại điểm bấm.
            s.on_move(p(399, 150));
            s.on_down(p(399, 150));
            s.on_move(p(401, 151));
            s.on_up(p(401, 151));
            assert_eq!(s.sel.rect(), Some(WIN));
        }

        #[test]
        fn flat_drag_selects_nothing() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(Some(vec![WIN]));
            let mut s = state(&sh, None);
            // Kéo 200 px ngang, lệch dọc 2 px: vùng dẹt bị bỏ như đợt 7, không chọn ứng viên.
            let (a, b) = (p(150, 150), p(350, 152));
            s.on_move(a);
            s.on_down(a);
            s.on_move(p(250, 151));
            assert_eq!(s.snap_hover, None);
            s.on_move(b);
            s.on_up(b);
            assert_eq!(s.sel.rect(), None);
        }

        #[test]
        fn press_keeps_preview_without_zero_frame() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(Some(vec![WIN]));
            let mut s = state(&sh, None);
            s.on_move(p(200, 150));
            s.on_down(p(200, 150));
            // Trong lúc chưa rời điểm bấm: hiện khung xem trước, không hiện khung/nhãn 0 × 0.
            assert_eq!(s.shown_rect(), Some(WIN));
            let (_, text, _) = s.label().unwrap();
            assert_eq!(text, wide("300 \u{d7} 200"));
            assert!(s.toolbar().is_none());
            s.on_up(p(200, 150));
            assert_eq!(s.sel.rect(), Some(WIN));
        }

        #[test]
        fn drag_50px_selects_region_not_candidate() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(Some(vec![WIN]));
            let mut s = state(&sh, None);
            let (a, b) = (p(150, 150), p(200, 200));
            s.on_move(a);
            assert_eq!(s.snap_hover, Some(WIN));
            s.on_down(a);
            // Vừa bấm: xem trước còn (không chớp tối); chuột rời điểm bấm >= MIN_SIZE mới thành kéo.
            assert_eq!(s.snap_hover, Some(WIN));
            s.on_move(p(152, 151));
            assert_eq!(s.snap_hover, Some(WIN));
            s.on_move(p(175, 175));
            assert_eq!(s.snap_hover, None);
            s.on_move(b);
            s.on_up(b);
            assert_eq!(s.sel.rect(), Some(Rect::from_points(a, b)));
            assert_eq!(s.snap_hover, None);
        }

        #[test]
        fn remembered_selection_disables_hover() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(Some(vec![WIN]));
            let sel = Rect {
                x: 500,
                y: 400,
                w: 100,
                h: 80,
            };
            let mut s = state(&sh, Some(sel));
            s.on_move(p(200, 150));
            assert_eq!(s.snap_hover, None);
            s.on_key(VK_RETURN.0, false);
            assert_eq!(s.sel.rect(), Some(sel));
            assert!(s.hint().is_none());
        }

        #[test]
        fn click_outside_selection_clears_it_and_hover_returns() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(Some(vec![WIN]));
            let sel = Rect {
                x: 500,
                y: 400,
                w: 100,
                h: 80,
            };
            let mut s = state(&sh, Some(sel));
            click(&mut s, p(200, 150));
            // Như hôm nay: bấm ra ngoài bỏ vùng chọn, không chọn ứng viên.
            assert_eq!(s.sel.rect(), None);
            assert_eq!(s.snap_hover, Some(WIN));
            // Bấm lần nữa thì chọn ứng viên.
            click(&mut s, p(200, 150));
            assert_eq!(s.sel.rect(), Some(WIN));
        }

        #[test]
        fn windows_none_click_selects_nothing() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(None);
            let mut s = state(&sh, None);
            assert!(s.cands.is_empty());
            s.on_move(p(200, 150));
            assert_eq!(s.snap_hover, None);
            click(&mut s, p(200, 150));
            assert_eq!(s.sel.rect(), None);
            s.on_key(VK_RETURN.0, false);
            assert_eq!(s.sel.rect(), None);
            let (_, text, _) = s.hint().unwrap();
            assert_eq!(text, wide(lang::tr(Msg::OverlayHintDrag)));
        }

        #[test]
        fn enter_selects_hovered_candidate() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(Some(vec![WIN]));
            let mut s = state(&sh, None);
            s.on_move(p(200, 150));
            s.on_key(VK_RETURN.0, false);
            assert_eq!(s.sel.rect(), Some(WIN));
            assert_eq!(s.snap_hover, None);
            assert!(s.toolbar().is_some());
        }

        #[test]
        fn enter_with_selection_keeps_it() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(Some(vec![WIN]));
            let mut s = state(&sh, None);
            click(&mut s, p(200, 150));
            assert_eq!(s.sel.rect(), Some(WIN));
            // Rê sang màn hình nền rồi Enter: vùng chọn không đổi.
            s.on_move(p(600, 500));
            assert_eq!(s.snap_hover, None);
            s.on_key(VK_RETURN.0, false);
            assert_eq!(s.sel.rect(), Some(WIN));
        }

        #[test]
        fn hint_shown_hidden_near_cursor_and_with_selection() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(Some(vec![WIN]));
            let mut s = state(&sh, None);
            let (r, text, dpi) = s.hint().unwrap();
            assert_eq!(text, wide(lang::tr(Msg::OverlayHintSnap)));
            let size = s.text_size(&text, dpi);
            assert_eq!(r, snap::hint_rect(MON, size, dpi));
            // Con trỏ ngay dưới viên nhắc: ẩn.
            s.on_move(p(r.x + r.w / 2, r.bottom() + 2));
            assert!(s.hint().is_none());
            s.on_move(p(600, 500));
            assert!(s.hint().is_some());
            // Đang kéo: ẩn.
            s.on_down(p(600, 500));
            s.on_move(p(650, 550));
            assert!(s.hint().is_none());
            s.on_up(p(650, 550));
            // Có vùng chọn: ẩn.
            assert!(s.sel.rect().is_some());
            assert!(s.hint().is_none());
        }

        fn covered(log: &[Rect], r: Rect) -> bool {
            log.iter().any(|c| {
                c.x <= r.x && c.y <= r.y && c.right() >= r.right() && c.bottom() >= r.bottom()
            })
        }

        #[test]
        fn click_select_repaints_old_hint() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(Some(vec![WIN]));
            let mut s = state(&sh, None);
            s.on_move(p(200, 150));
            let (hint, _, _) = s.hint().unwrap();
            s.inval_log.borrow_mut().clear();
            let at = p(200, 150);
            s.on_down(at);
            s.on_up(at);
            assert!(s.sel.rect().is_some());
            let log = s.inval_log.borrow().clone();
            assert!(covered(&log, hint), "{log:?}");
        }

        #[test]
        fn enter_select_repaints_old_hint() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(Some(vec![WIN]));
            let mut s = state(&sh, None);
            s.on_move(p(200, 150));
            let (hint, _, _) = s.hint().unwrap();
            s.inval_log.borrow_mut().clear();
            s.on_key(VK_RETURN.0, false);
            assert!(s.sel.rect().is_some());
            let log = s.inval_log.borrow().clone();
            assert!(covered(&log, hint), "{log:?}");
        }

        /// Một thông điệp tới lớp phủ trong mô hình vẽ của `screen_hint_follows_state`.
        #[derive(Clone, Copy, Debug)]
        enum Ev {
            Move(Point),
            Down(Point),
            /// WM_LBUTTONUP rồi WM_CAPTURECHANGED (wndproc trả chuột sau khi nhả).
            Up(Point),
            Enter,
        }

        /// Mọi điểm của `r` nằm trong hợp các ô của `log`.
        fn union_covers(log: &[Rect], r: Rect) -> bool {
            (r.y..r.bottom())
                .all(|y| (r.x..r.right()).all(|x| log.iter().any(|c| c.contains(p(x, y)))))
        }

        /// Mô hình màn hình: sau mỗi thông điệp, nếu viên nhắc (theo trạng thái) khác viên nhắc
        /// đang vẽ trên màn hình thì cả ô cũ và ô mới phải nằm trong vùng đã invalidate; khi đó
        /// màn hình theo trạng thái. Trả mô tả bước đầu tiên để lại viên nhắc cũ (đọng) hay thiếu.
        fn screen_hint_follows_state(s: &mut State, evs: &[Ev]) -> Result<(), String> {
            let mut screen = s.hint().map(|h| h.0);
            for (i, ev) in evs.iter().enumerate() {
                s.inval_log.borrow_mut().clear();
                match *ev {
                    Ev::Move(at) => s.on_move(at),
                    Ev::Down(at) => {
                        s.on_down(at);
                    }
                    Ev::Up(at) => {
                        s.on_up(at);
                        s.lost_capture();
                    }
                    Ev::Enter => s.on_key(VK_RETURN.0, false),
                }
                let now = s.hint().map(|h| h.0);
                if now != screen {
                    let log = s.inval_log.borrow().clone();
                    for r in [screen, now].into_iter().flatten() {
                        if !union_covers(&log, r) {
                            return Err(format!("bước {i} {ev:?}: ô {r:?} không vẽ lại; {log:?}"));
                        }
                    }
                    screen = now;
                }
            }
            Ok(())
        }

        #[test]
        fn click_paths_never_leave_hint_on_screen() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(Some(vec![WIN]));
            let at = p(200, 150);
            let (hint, _, _) = state(&sh, None).hint().unwrap();
            let far = p(600, 500);
            // Con trỏ thật ngay dưới viên nhắc (viên nhắc ẩn khi chuột gần).
            let near = p(hint.x + hint.w / 2, hint.bottom() + 2);
            let cases: Vec<(&str, Vec<Ev>)> = vec![
                // WM_MOUSEMOVE hệ thống tại con trỏ thật (xa) chen giữa bấm và nhả: thành kéo, nhả
                // tại điểm bấm thì không chọn gì và viên nhắc hiện lại.
                (
                    "demo, chuột thật xa chen giữa bấm và nhả",
                    vec![
                        Ev::Move(at),
                        Ev::Down(at),
                        Ev::Move(far),
                        Ev::Up(at),
                        Ev::Move(far),
                    ],
                ),
                (
                    "chuột thật",
                    vec![Ev::Move(at), Ev::Down(at), Ev::Up(at), Ev::Move(at)],
                ),
                (
                    "rung tay",
                    vec![
                        Ev::Move(at),
                        Ev::Down(at),
                        Ev::Move(p(202, 151)),
                        Ev::Up(p(202, 151)),
                    ],
                ),
                // Demo gửi thông điệp: hệ thống chen WM_MOUSEMOVE tại con trỏ thật (xa) giữa các
                // thông điệp đã gửi, nên lúc bấm `mouse` không phải điểm bấm.
                (
                    "demo, con trỏ thật xa",
                    vec![
                        Ev::Move(at),
                        Ev::Move(far),
                        Ev::Down(at),
                        Ev::Up(at),
                        Ev::Move(far),
                    ],
                ),
                (
                    "demo, con trỏ thật sát viên nhắc",
                    vec![
                        Ev::Move(at),
                        Ev::Move(near),
                        Ev::Down(at),
                        Ev::Up(at),
                        Ev::Move(near),
                    ],
                ),
                (
                    "demo, bấm khi chưa có WM_MOUSEMOVE nào",
                    vec![Ev::Down(at), Ev::Up(at), Ev::Move(far)],
                ),
                ("Enter", vec![Ev::Move(at), Ev::Enter, Ev::Move(at)]),
            ];
            for (name, evs) in cases {
                let mut s = state(&sh, None);
                assert!(s.hint().is_some(), "{name}");
                screen_hint_follows_state(&mut s, &evs).unwrap_or_else(|e| panic!("{name}: {e}"));
                if name.contains("chen giữa") {
                    assert_eq!(s.sel.rect(), None, "{name}");
                    assert!(s.hint().is_some(), "{name}");
                } else {
                    assert_eq!(s.sel.rect(), Some(WIN), "{name}");
                    assert!(s.hint().is_none(), "{name}");
                }
            }
        }

        #[test]
        fn click_with_cursor_on_other_monitor_never_leaves_hint() {
            let _gdi = GDI_TEST_LOCK.lock();
            // Hai màn hình khác DPI (96 và 192): viên nhắc theo màn hình chứa `mouse`, nên con trỏ
            // thật ở màn bên kia làm viên nhắc nhảy màn giữa các thông điệp đã gửi.
            let mon2 = Rect {
                x: W,
                y: 0,
                w: W,
                h: H,
            };
            let mut sh = shot(Some(vec![WIN]));
            sh.image.width = (2 * W) as u32;
            sh.image.bgra = vec![0x80; (2 * W * H * 4) as usize];
            sh.monitors = vec![MON, mon2];
            let mut s = State::new(&sh, 2 * W, H, None, &CustomColors::default(), false).unwrap();
            s.dpis = vec![96, 192];
            let at = p(200, 150);
            let other = p(W + 400, 500);
            // Viên nhắc có ở cả hai màn và nhảy màn theo con trỏ.
            s.on_move(other);
            let on_other = s.hint().expect("viên nhắc ở màn 2").0;
            s.on_move(at);
            let on_at = s.hint().expect("viên nhắc ở màn 1").0;
            assert!(mon2.contains(p(on_other.x, on_other.y)), "{on_other:?}");
            assert!(MON.contains(p(on_at.x, on_at.y)), "{on_at:?}");
            assert_ne!(on_other, on_at);
            let evs = [
                Ev::Move(other),
                Ev::Move(at),
                Ev::Move(other),
                Ev::Down(at),
                Ev::Up(at),
                Ev::Move(other),
            ];
            screen_hint_follows_state(&mut s, &evs).unwrap();
            assert_eq!(s.sel.rect(), Some(WIN));
        }

        #[test]
        fn chrome_covers_hover_and_hint() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(Some(vec![WIN]));
            let mut s = state(&sh, None);
            s.on_move(p(200, 150));
            let (hint, _, _) = s.hint().unwrap();
            let chrome = s.chrome();
            let covers = |r: Rect| {
                chrome.iter().any(|c| {
                    c.x <= r.x && c.y <= r.y && c.right() >= r.right() && c.bottom() >= r.bottom()
                })
            };
            assert!(covers(inflate(WIN, 1)), "{chrome:?}");
            assert!(covers(hint), "{chrome:?}");
            let (label, _, _) = s.label().unwrap();
            assert!(covers(label), "{chrome:?}");
        }

        /// Ctrl giả (`TEST_MODS`) trong một khối; nhả cả khi khẳng định trượt.
        struct CtrlHeld;
        impl CtrlHeld {
            fn hold() -> CtrlHeld {
                TEST_MODS.store(1, std::sync::atomic::Ordering::SeqCst);
                CtrlHeld
            }
        }
        impl Drop for CtrlHeld {
            fn drop(&mut self) {
                TEST_MODS.store(0, std::sync::atomic::Ordering::SeqCst);
            }
        }

        const SEL: Rect = Rect {
            x: 120,
            y: 90,
            w: 200,
            h: 150,
        };

        #[test]
        fn ctrl_p_pins_selection() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(None);
            let mut s = state(&sh, Some(SEL));
            assert_eq!(s.sel.rect(), Some(SEL));
            let _ctrl = CtrlHeld::hold();
            s.on_key(0x50, false);
            let r = s
                .result
                .take()
                .expect("đã kết thúc")
                .expect("không phải Hủy");
            assert_eq!((r.action, r.rect), (Action::Pin, SEL));
            assert!(r.shapes.is_empty());
        }

        #[test]
        fn ctrl_t_recognizes_selection() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(None);
            let mut s = state(&sh, Some(SEL));
            let _ctrl = CtrlHeld::hold();
            s.on_key(0x54, false);
            let r = s
                .result
                .take()
                .expect("đã kết thúc")
                .expect("không phải Hủy");
            assert_eq!((r.action, r.rect), (Action::Ocr, SEL));
            assert!(r.shapes.is_empty());
        }

        #[test]
        fn ctrl_t_without_selection_does_nothing() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(None);
            let mut s = state(&sh, None);
            let _ctrl = CtrlHeld::hold();
            s.on_key(0x54, false);
            assert!(s.result.is_none());
        }

        #[test]
        fn ctrl_t_while_typing_commits_text_then_recognizes() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(None);
            let mut s = state(&sh, Some(SEL));
            // Cửa sổ rỗng trong kiểm thử: không để việc đóng ô đăng ký Esc toàn cục cho luồng này.
            s.esc_registered = true;
            let mut b = TextBox::new(p(150, 120), 96, s.size, s.color);
            b.edit.insert("hi");
            s.text = Some(b);
            let _ctrl = CtrlHeld::hold();
            s.on_key(0x54, false);
            assert!(s.text.is_none());
            let r = s
                .result
                .take()
                .expect("đã kết thúc")
                .expect("không phải Hủy");
            assert_eq!((r.action, r.rect), (Action::Ocr, SEL));
            assert_eq!(r.shapes.len(), 1);
            assert!(matches!(r.shapes[0], Shape::Text { .. }));
        }

        #[test]
        fn ctrl_p_without_selection_does_nothing() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(None);
            let mut s = state(&sh, None);
            let _ctrl = CtrlHeld::hold();
            s.on_key(0x50, false);
            assert!(s.result.is_none());
        }

        #[test]
        fn ctrl_p_while_typing_commits_text_then_pins() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(None);
            let mut s = state(&sh, Some(SEL));
            // Cửa sổ rỗng trong kiểm thử: không để việc đóng ô đăng ký Esc toàn cục cho luồng này.
            s.esc_registered = true;
            let mut b = TextBox::new(p(150, 120), 96, s.size, s.color);
            b.edit.insert("hi");
            s.text = Some(b);
            let _ctrl = CtrlHeld::hold();
            s.on_key(0x50, false);
            assert!(s.text.is_none());
            let r = s
                .result
                .take()
                .expect("đã kết thúc")
                .expect("không phải Hủy");
            assert_eq!((r.action, r.rect), (Action::Pin, SEL));
            assert_eq!(r.shapes.len(), 1);
            assert!(matches!(r.shapes[0], Shape::Text { .. }));
        }

        #[test]
        fn ctrl_n_adds_step_with_session() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(None);
            let mut s = state(&sh, Some(SEL));
            s.steps = true;
            let _ctrl = CtrlHeld::hold();
            s.on_key(0x4E, false);
            let r = s
                .result
                .take()
                .expect("đã kết thúc")
                .expect("không phải Hủy");
            assert_eq!((r.action, r.rect), (Action::AddStep, SEL));
            assert!(r.shapes.is_empty());
        }

        #[test]
        fn ctrl_n_without_session_does_nothing() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(None);
            let mut s = state(&sh, Some(SEL));
            assert!(!s.steps);
            let _ctrl = CtrlHeld::hold();
            s.on_key(0x4E, false);
            assert!(s.result.is_none());
        }

        #[test]
        fn ctrl_n_without_selection_does_nothing() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(None);
            let mut s = state(&sh, None);
            s.steps = true;
            let _ctrl = CtrlHeld::hold();
            s.on_key(0x4E, false);
            assert!(s.result.is_none());
        }

        #[test]
        fn ctrl_n_while_typing_commits_text_then_adds_step() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(None);
            let mut s = state(&sh, Some(SEL));
            s.steps = true;
            // Cửa sổ rỗng trong kiểm thử: không để việc đóng ô đăng ký Esc toàn cục cho luồng này.
            s.esc_registered = true;
            let mut b = TextBox::new(p(150, 120), 96, s.size, s.color);
            b.edit.insert("hi");
            s.text = Some(b);
            let _ctrl = CtrlHeld::hold();
            s.on_key(0x4E, false);
            assert!(s.text.is_none());
            let r = s
                .result
                .take()
                .expect("đã kết thúc")
                .expect("không phải Hủy");
            assert_eq!((r.action, r.rect), (Action::AddStep, SEL));
            assert_eq!(r.shapes.len(), 1);
        }

        #[test]
        fn ctrl_n_while_typing_without_session_keeps_typing() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(None);
            let mut s = state(&sh, Some(SEL));
            s.esc_registered = true;
            let mut b = TextBox::new(p(150, 120), 96, s.size, s.color);
            b.edit.insert("hi");
            s.text = Some(b);
            let _ctrl = CtrlHeld::hold();
            s.on_key(0x4E, false);
            assert!(s.text.is_some());
            assert!(s.result.is_none());
        }

        /// Có phiên: thanh ngang của trạng thái có ô Thêm bước; bấm vào nó kết thúc bằng AddStep.
        #[test]
        fn add_step_button_with_session() {
            let _gdi = GDI_TEST_LOCK.lock();
            let sh = shot(None);
            let mut s = state(&sh, Some(SEL));
            s.steps = true;
            let (bar, dpi) = s.toolbar().unwrap();
            assert_eq!((bar.w, bar.h), bar_layout_with(dpi, true).size);
            let r = bar_item_rect_with(bar, dpi, true, BarItem::Act(Action::AddStep)).unwrap();
            let at = p(r.x + r.w / 2, r.y + r.h / 2);
            assert_eq!(s.bar_item_at(at), Some(BarItem::Act(Action::AddStep)));
            // Nút thanh ngang: nhấn rồi nhả trên cùng ô (không giữ chuột như khi kéo).
            s.on_move(at);
            s.on_down(at);
            s.on_up(at);
            let res = s
                .result
                .take()
                .expect("đã kết thúc")
                .expect("không phải Hủy");
            assert_eq!(res.action, Action::AddStep);
            // Không phiên: thanh 9 ô như cũ.
            let s2 = state(&sh, Some(SEL));
            let (bar2, dpi2) = s2.toolbar().unwrap();
            assert_eq!((bar2.w, bar2.h), bar_layout(dpi2).size);
        }
    }
}
