//! Chụp từng màn hình bằng ScreenCaptureKit (macOS 14+, quyết định 134, 135) và quyền Screen
//! Recording.
//!
//! Phần thuần (đổi bộ đệm `CGImage` sang BGRA, kích thước và màn hình chứa một điểm) có kiểm
//! thử chạy trên máy không có GUI.

use crate::coords::{pt_to_px, RectF};
use block2::RcBlock;
use darkshot_core::i18n::{t, Lang, Msg};
use darkshot_core::image::Image;
use objc2::rc::{autoreleasepool, Retained};
use objc2::AnyThread;
use objc2_core_graphics::{
    kCGColorSpaceSRGB, CGDataProvider, CGDisplayBounds, CGEvent, CGImage, CGMainDisplayID,
    CGPreflightScreenCaptureAccess, CGRequestScreenCaptureAccess,
};
use objc2_foundation::{NSArray, NSError};
use objc2_screen_capture_kit::{
    SCContentFilter, SCDisplay, SCScreenshotManager, SCShareableContent, SCStreamConfiguration,
    SCStreamErrorCode, SCWindow,
};
use std::ptr::NonNull;
use std::sync::mpsc;
use std::time::Duration;

/// Ảnh của một màn hình.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayShot {
    /// BGRA, từ trên xuống, pixel vật lý.
    pub image: Image,
    /// Khung màn hình trong toạ độ toàn cục gốc trên trái (gốc là góc trên trái màn hình chính),
    /// đơn vị điểm.
    pub frame_pt: RectF,
    /// Số pixel mỗi điểm (2 trên màn hình Retina).
    pub scale: f64,
    /// `CGDirectDisplayID` của màn hình.
    pub display_id: u32,
}

/// Lỗi chụp. `Failed` mang chữ kỹ thuật cho nhật ký, không hiện cho người dùng.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptureError {
    PermissionDenied,
    NoDisplay,
    Failed(String),
}

/// Tiêu đề và nội dung thông báo cho một lỗi chụp (theo `lang`).
pub fn capture_error_notice(lang: Lang, e: &CaptureError) -> (&'static str, &'static str) {
    let body = match e {
        CaptureError::PermissionDenied => {
            return (
                t(lang, Msg::MacScreenPermissionTitle),
                t(lang, Msg::MacScreenPermissionBody),
            )
        }
        CaptureError::NoDisplay => Msg::MacNoDisplayBody,
        CaptureError::Failed(_) => Msg::MacCaptureFailedBody,
    };
    (t(lang, Msg::CaptureFailedTitle), t(lang, body))
}

/// Thứ tự thành phần trong một pixel 32 bit (theo từ 32 bit, trước khi xét thứ tự byte).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlphaKind {
    /// ARGB, màu đã nhân alpha.
    PremultipliedFirst,
    /// RGBA, màu đã nhân alpha.
    PremultipliedLast,
    /// ARGB, màu chưa nhân alpha.
    First,
    /// RGBA, màu chưa nhân alpha.
    Last,
    /// xRGB, không có alpha.
    NoneSkipFirst,
    /// RGBx, không có alpha.
    NoneSkipLast,
}

/// Bố cục bộ đệm của một `CGImage` 8 bit mỗi thành phần, 32 bit mỗi pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelLayout {
    pub alpha: AlphaKind,
    /// Từ 32 bit lưu little-endian (`kCGBitmapByteOrder32Little`): byte trong bộ nhớ đảo
    /// ngược so với thứ tự thành phần.
    pub little_endian: bool,
}

/// Bố cục từ các giá trị thô của Core Graphics (`CGImageGetAlphaInfo`, `CGImageGetBitmapInfo`,
/// bit mỗi thành phần, bit mỗi pixel). Định dạng khác 8/32 bit nguyên thì `None`.
pub fn layout_from_cg(
    alpha_info: u32,
    bitmap_info: u32,
    bits_per_component: usize,
    bits_per_pixel: usize,
) -> Option<PixelLayout> {
    const ALPHA_MASK: u32 = 0x1F;
    const FLOAT_COMPONENTS: u32 = 1 << 8;
    const ORDER_MASK: u32 = 0x7000;
    const ORDER_DEFAULT: u32 = 0;
    const ORDER_32_LITTLE: u32 = 2 << 12;
    const ORDER_32_BIG: u32 = 4 << 12;
    if bits_per_component != 8 || bits_per_pixel != 32 || bitmap_info & FLOAT_COMPONENTS != 0 {
        return None;
    }
    let alpha = match alpha_info & ALPHA_MASK {
        1 => AlphaKind::PremultipliedLast,
        2 => AlphaKind::PremultipliedFirst,
        3 => AlphaKind::Last,
        4 => AlphaKind::First,
        5 => AlphaKind::NoneSkipLast,
        6 => AlphaKind::NoneSkipFirst,
        _ => return None,
    };
    let little_endian = match bitmap_info & ORDER_MASK {
        ORDER_32_LITTLE => true,
        ORDER_DEFAULT | ORDER_32_BIG => false,
        _ => return None,
    };
    Some(PixelLayout {
        alpha,
        little_endian,
    })
}

/// Đổi bộ đệm pixel của `CGImage` (mỗi hàng `bytes_per_row` byte, từ trên xuống) sang `Image`
/// BGRA chưa nhân alpha. Không có alpha thì alpha = 255.
pub fn to_bgra(
    buf: &[u8],
    width: usize,
    height: usize,
    bytes_per_row: usize,
    layout: PixelLayout,
) -> Result<Image, String> {
    if width == 0 || height == 0 {
        return Err("empty image".into());
    }
    let row_bytes = width
        .checked_mul(4)
        .ok_or_else(|| "image too large".to_string())?;
    if bytes_per_row < row_bytes {
        return Err("row stride shorter than the image width".into());
    }
    // Hàng cuối chỉ cần đủ pixel, không cần phần đệm.
    let needed = (height - 1)
        .checked_mul(bytes_per_row)
        .and_then(|n| n.checked_add(row_bytes))
        .ok_or_else(|| "image too large".to_string())?;
    if buf.len() < needed {
        return Err("pixel buffer is shorter than the image".into());
    }
    let total = row_bytes
        .checked_mul(height)
        .ok_or_else(|| "image too large".to_string())?;
    let (w, h) = match (u32::try_from(width), u32::try_from(height)) {
        (Ok(w), Ok(h)) => (w, h),
        _ => return Err("image too large".into()),
    };
    // Vị trí của A, R, G, B (theo thứ tự thành phần trong từ 32 bit); `None` là không có alpha.
    let (a, r, g, b) = match layout.alpha {
        AlphaKind::PremultipliedFirst | AlphaKind::First => (Some(0), 1, 2, 3),
        AlphaKind::NoneSkipFirst => (None, 1, 2, 3),
        AlphaKind::PremultipliedLast | AlphaKind::Last => (Some(3), 0, 1, 2),
        AlphaKind::NoneSkipLast => (None, 0, 1, 2),
    };
    // Little-endian: byte thứ i trong bộ nhớ là thành phần thứ 3 - i.
    let at = |i: usize| if layout.little_endian { 3 - i } else { i };
    let premultiplied = matches!(
        layout.alpha,
        AlphaKind::PremultipliedFirst | AlphaKind::PremultipliedLast
    );
    let mut bgra = Vec::with_capacity(total);
    for row in buf.chunks(bytes_per_row).take(height) {
        for px in row[..row_bytes].as_chunks::<4>().0 {
            let alpha = a.map_or(255, |i| px[at(i)]);
            let mut c = [px[at(b)], px[at(g)], px[at(r)]];
            if premultiplied && alpha < 255 {
                for v in &mut c {
                    *v = unpremultiply(*v, alpha);
                }
            }
            bgra.extend_from_slice(&[c[0], c[1], c[2], alpha]);
        }
    }
    Ok(Image {
        width: w,
        height: h,
        bgra,
    })
}

/// Bỏ nhân alpha: `c * 255 / a`, làm tròn, kẹp ở 255; `a = 0` cho 0.
fn unpremultiply(c: u8, a: u8) -> u8 {
    if a == 0 {
        return 0;
    }
    let (c, a) = (u32::from(c), u32::from(a));
    ((c * 255 + a / 2) / a).min(255) as u8
}

/// Kích thước pixel vật lý của màn hình có khung `frame_pt` (điểm) và hệ số `scale`.
pub fn pixel_size(frame_pt: RectF, scale: f64) -> Option<(u32, u32)> {
    let ok = |v: f64| v.is_finite() && v > 0.0;
    if !ok(scale) || !ok(frame_pt.w) || !ok(frame_pt.h) {
        return None;
    }
    let (w, h) = (pt_to_px(frame_pt.w, scale), pt_to_px(frame_pt.h, scale));
    match (u32::try_from(w), u32::try_from(h)) {
        (Ok(w), Ok(h)) if w > 0 && h > 0 => Some((w, h)),
        _ => None,
    }
}

/// Dựng `DisplayShot` từ ảnh và khung màn hình. Ảnh khác kích thước dự kiến (ví dụ hệ số báo sai)
/// thì hệ số lấy theo ảnh thật để toạ độ điểm ↔ pixel vẫn đúng.
pub fn shot_from_frame(
    image: Image,
    frame_pt: RectF,
    scale: f64,
    display_id: u32,
) -> Result<DisplayShot, CaptureError> {
    if !image.is_valid() {
        return Err(CaptureError::Failed("invalid display image".into()));
    }
    if !(frame_pt.w.is_finite() && frame_pt.w > 0.0 && frame_pt.h.is_finite() && frame_pt.h > 0.0) {
        return Err(CaptureError::Failed("invalid display frame".into()));
    }
    let scale = if pixel_size(frame_pt, scale) == Some((image.width, image.height)) {
        scale
    } else {
        f64::from(image.width) / frame_pt.w
    };
    Ok(DisplayShot {
        image,
        frame_pt,
        scale,
        display_id,
    })
}

/// Chỉ số màn hình chứa điểm `(x, y)` (toạ độ toàn cục gốc trên trái, điểm); ngoài mọi màn
/// hình thì màn hình gần nhất; danh sách rỗng thì `None`.
pub fn display_at(frames: &[RectF], x: f64, y: f64) -> Option<usize> {
    let inside = |f: &RectF| x >= f.x && x < f.x + f.w && y >= f.y && y < f.y + f.h;
    if let Some(i) = frames.iter().position(inside) {
        return Some(i);
    }
    // Bình phương khoảng cách tới khung; NaN coi như rất xa.
    let dist = |f: &RectF| {
        let dx = (f.x - x).max(0.0).max(x - (f.x + f.w));
        let dy = (f.y - y).max(0.0).max(y - (f.y + f.h));
        let d = dx * dx + dy * dy;
        if d.is_nan() {
            f64::INFINITY
        } else {
            d
        }
    };
    frames
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| dist(a).total_cmp(&dist(b)))
        .map(|(i, _)| i)
}

/// Màn hình để lưu: chứa (hoặc gần nhất) vị trí con trỏ lúc yêu cầu chụp; không đọc được con
/// trỏ thì màn hình chính (`main_id`, theo `CGMainDisplayID`); không thấy màn hình chính thì màn
/// hình đầu tiên. `displays` là `(display_id, frame_pt)`; rỗng thì `None`.
pub fn pick_display(
    displays: &[(u32, RectF)],
    cursor: Option<(f64, f64)>,
    main_id: u32,
) -> Option<usize> {
    if displays.is_empty() {
        return None;
    }
    if let Some((x, y)) = cursor {
        let frames: Vec<RectF> = displays.iter().map(|d| d.1).collect();
        return display_at(&frames, x, y);
    }
    Some(displays.iter().position(|d| d.0 == main_id).unwrap_or(0))
}

/// `CGDirectDisplayID` của màn hình chính (có thanh menu).
pub fn main_display_id() -> u32 {
    CGMainDisplayID()
}

/// Thời gian chờ tối đa cho mỗi lời gọi ScreenCaptureKit.
const SCK_TIMEOUT: Duration = Duration::from_secs(10);

/// `kCVPixelFormatType_32BGRA` ('BGRA').
const PIXEL_FORMAT_BGRA: u32 = u32::from_be_bytes(*b"BGRA");

/// Ứng dụng đã có quyền Screen Recording chưa (không hỏi, không hiện gì).
pub fn screen_capture_allowed() -> bool {
    CGPreflightScreenCaptureAccess()
}

/// Hỏi quyền Screen Recording (hệ thống hiện hộp thoại hoặc mở Cài đặt). Bên gọi chỉ gọi một
/// lần mỗi lần chạy, trên luồng chính.
pub fn request_screen_capture_access() -> bool {
    CGRequestScreenCaptureAccess()
}

/// Vị trí con trỏ trong toạ độ toàn cục gốc trên trái, đơn vị điểm (như `CGDisplayBounds`).
/// Gọi được từ mọi luồng.
pub fn cursor_location() -> Option<(f64, f64)> {
    let ev = CGEvent::new(None)?;
    let p = CGEvent::location(Some(&ev));
    Some((p.x, p.y))
}

/// Chụp mọi màn hình, mỗi màn hình một ảnh ở pixel vật lý. Chặn luồng gọi tới khi xong (tối đa
/// vài giây mỗi màn hình): **không gọi trên luồng chính**.
pub fn capture_displays(include_cursor: bool) -> Result<Vec<DisplayShot>, CaptureError> {
    if !screen_capture_allowed() {
        return Err(CaptureError::PermissionDenied);
    }
    autoreleasepool(|_| {
        let content = shareable_content()?;
        // SAFETY: `content` là đối tượng hợp lệ do ScreenCaptureKit trả về.
        let displays = unsafe { content.0.displays() };
        if displays.is_empty() {
            return Err(CaptureError::NoDisplay);
        }
        let mut shots = Vec::with_capacity(displays.len());
        for display in displays.to_vec() {
            shots.push(capture_one(&display, include_cursor)?);
        }
        Ok(shots)
    })
}

/// `SCShareableContent` chuyển từ hàng đợi của khối hoàn tất (ScreenCaptureKit gọi khối trên
/// hàng đợi riêng của nó) sang luồng chụp đang chờ.
struct SendContent(Retained<SCShareableContent>);
// SAFETY: `SCShareableContent` là ảnh chụp bất biến của danh sách màn hình/cửa sổ sau khi tạo,
// ScreenCaptureKit không gắn nó với luồng hay hàng đợi nào (Apple dùng nó từ các hàng đợi khác
// nhau). Giá trị chỉ được chuyển một lần qua kênh và chỉ một luồng giữ nó tại một thời điểm.
// Chỉ cài cho đúng kiểu này, không cho `Retained` bất kỳ.
unsafe impl Send for SendContent {}

fn sck_error(err: *mut NSError, what: &str) -> CaptureError {
    // SAFETY: ScreenCaptureKit trao `NSError` hợp lệ hoặc null; `retain` nhận null.
    let code = unsafe { Retained::retain(err) }.map(|e| e.code());
    match code {
        Some(c) if c == SCStreamErrorCode::UserDeclined.0 => CaptureError::PermissionDenied,
        Some(c) => CaptureError::Failed(format!("{what} failed (SCStreamError {c})")),
        None => CaptureError::Failed(format!("{what} returned nothing")),
    }
}

fn shareable_content() -> Result<SendContent, CaptureError> {
    let (tx, rx) = mpsc::channel();
    let block = RcBlock::new(move |content: *mut SCShareableContent, err: *mut NSError| {
        // SAFETY: con trỏ hợp lệ hoặc null, do ScreenCaptureKit trao; `retain` giữ nó sống.
        let r = match unsafe { Retained::retain(content) } {
            Some(c) => Ok(SendContent(c)),
            None => Err(sck_error(err, "SCShareableContent")),
        };
        let _ = tx.send(r);
    });
    // SAFETY: khối có đúng chữ ký mà phương thức đòi; ScreenCaptureKit giữ khối tới khi gọi xong.
    unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&block) };
    rx.recv_timeout(SCK_TIMEOUT)
        .map_err(|_| CaptureError::Failed("SCShareableContent timed out".into()))?
}

fn capture_one(display: &SCDisplay, include_cursor: bool) -> Result<DisplayShot, CaptureError> {
    // SAFETY: `display` hợp lệ (lấy từ `SCShareableContent`).
    let id = unsafe { display.displayID() };
    let b = CGDisplayBounds(id);
    let frame_pt = RectF {
        x: b.origin.x,
        y: b.origin.y,
        w: b.size.width,
        h: b.size.height,
    };
    let excluded = NSArray::<SCWindow>::new();
    // SAFETY: `alloc` mới, `display` và mảng rỗng hợp lệ.
    let filter = unsafe {
        SCContentFilter::initWithDisplay_excludingWindows(
            SCContentFilter::alloc(),
            display,
            &excluded,
        )
    };
    // SAFETY: `filter` hợp lệ; `infoForFilter` có từ macOS 14 (yêu cầu tối thiểu).
    let scale = f64::from(unsafe { SCShareableContent::infoForFilter(&filter).pointPixelScale() });
    let (w, h) = pixel_size(frame_pt, scale)
        .ok_or_else(|| CaptureError::Failed(format!("bad display geometry (scale {scale})")))?;
    // SAFETY: cấu hình mới, các giá trị hợp lệ; `kCGColorSpaceSRGB` là hằng sống suốt tiến trình.
    let config = unsafe {
        let c = SCStreamConfiguration::new();
        c.setWidth(w as usize);
        c.setHeight(h as usize);
        c.setShowsCursor(include_cursor);
        c.setPixelFormat(PIXEL_FORMAT_BGRA);
        c.setColorSpaceName(kCGColorSpaceSRGB);
        c
    };
    let (tx, rx) = mpsc::channel();
    let block = RcBlock::new(move |img: *mut CGImage, err: *mut NSError| {
        let r = match NonNull::new(img) {
            // SAFETY: `img` khác null và hợp lệ trong khối; đổi ngay sang bộ đệm của ta.
            Some(p) => image_from_cg(unsafe { p.as_ref() }).map_err(CaptureError::Failed),
            None => Err(sck_error(err, "SCScreenshotManager")),
        };
        let _ = tx.send(r);
    });
    // SAFETY: bộ lọc, cấu hình và khối hợp lệ; ScreenCaptureKit giữ khối tới khi gọi xong.
    unsafe {
        SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
            &filter,
            &config,
            Some(&block),
        )
    };
    let image = rx
        .recv_timeout(SCK_TIMEOUT)
        .map_err(|_| CaptureError::Failed("SCScreenshotManager timed out".into()))??;
    shot_from_frame(image, frame_pt, scale, id)
}

/// Đọc pixel của một `CGImage` 8 bit, 32 bit mỗi pixel sang `Image` BGRA.
fn image_from_cg(img: &CGImage) -> Result<Image, String> {
    let i = Some(img);
    let layout = layout_from_cg(
        CGImage::alpha_info(i).0,
        CGImage::bitmap_info(i).0,
        CGImage::bits_per_component(i),
        CGImage::bits_per_pixel(i),
    )
    .ok_or_else(|| "unsupported CGImage pixel format".to_string())?;
    let provider = CGImage::data_provider(i).ok_or_else(|| "CGImage has no data".to_string())?;
    let data = CGDataProvider::data(Some(&provider))
        .ok_or_else(|| "CGImage data copy failed".to_string())?;
    // SAFETY: `CGDataProviderCopyData` trả một `CFData` bất biến mới, không ai sửa nó trong lúc
    // ta đọc.
    let bytes = unsafe { data.as_bytes_unchecked() };
    to_bgra(
        bytes,
        CGImage::width(i),
        CGImage::height(i),
        CGImage::bytes_per_row(i),
        layout,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Hằng thô của Core Graphics (CGImage.h).
    const ALPHA_PREMUL_LAST: u32 = 1;
    const ALPHA_PREMUL_FIRST: u32 = 2;
    const ALPHA_LAST: u32 = 3;
    const ALPHA_FIRST: u32 = 4;
    const ALPHA_SKIP_LAST: u32 = 5;
    const ALPHA_SKIP_FIRST: u32 = 6;
    const ALPHA_NONE: u32 = 0;
    const ORDER_DEFAULT: u32 = 0;
    const ORDER_32_LITTLE: u32 = 2 << 12;
    const ORDER_32_BIG: u32 = 4 << 12;
    const ORDER_16_LITTLE: u32 = 1 << 12;
    const FLOAT: u32 = 1 << 8;

    fn r(x: f64, y: f64, w: f64, h: f64) -> RectF {
        RectF { x, y, w, h }
    }

    fn lay(alpha: AlphaKind, little_endian: bool) -> PixelLayout {
        PixelLayout {
            alpha,
            little_endian,
        }
    }

    #[test]
    fn screencapturekit_bgra_layout_is_recognised() {
        // SCScreenshotManager (SDR): kCGImageAlphaPremultipliedFirst | kCGBitmapByteOrder32Little.
        assert_eq!(
            layout_from_cg(
                ALPHA_PREMUL_FIRST,
                ALPHA_PREMUL_FIRST | ORDER_32_LITTLE,
                8,
                32
            ),
            Some(lay(AlphaKind::PremultipliedFirst, true))
        );
        assert_eq!(
            layout_from_cg(ALPHA_SKIP_FIRST, ALPHA_SKIP_FIRST | ORDER_32_LITTLE, 8, 32),
            Some(lay(AlphaKind::NoneSkipFirst, true))
        );
        assert_eq!(
            layout_from_cg(ALPHA_PREMUL_LAST, ALPHA_PREMUL_LAST | ORDER_DEFAULT, 8, 32),
            Some(lay(AlphaKind::PremultipliedLast, false))
        );
        assert_eq!(
            layout_from_cg(ALPHA_LAST, ALPHA_LAST | ORDER_32_BIG, 8, 32),
            Some(lay(AlphaKind::Last, false))
        );
        assert_eq!(
            layout_from_cg(ALPHA_FIRST, ALPHA_FIRST, 8, 32),
            Some(lay(AlphaKind::First, false))
        );
        assert_eq!(
            layout_from_cg(ALPHA_SKIP_LAST, ALPHA_SKIP_LAST | ORDER_32_BIG, 8, 32),
            Some(lay(AlphaKind::NoneSkipLast, false))
        );
    }

    #[test]
    fn unsupported_layouts_are_rejected() {
        // 16 bit mỗi thành phần (HDR), số thực, 24 bit, không alpha/chỉ alpha, thứ tự 16 bit.
        assert_eq!(
            layout_from_cg(ALPHA_PREMUL_FIRST, ORDER_32_LITTLE, 16, 64),
            None
        );
        assert_eq!(
            layout_from_cg(ALPHA_PREMUL_LAST, ALPHA_PREMUL_LAST | FLOAT, 32, 128),
            None
        );
        assert_eq!(
            layout_from_cg(ALPHA_PREMUL_FIRST, ALPHA_PREMUL_FIRST | FLOAT, 8, 32),
            None
        );
        assert_eq!(layout_from_cg(ALPHA_NONE, ALPHA_NONE, 8, 24), None);
        assert_eq!(layout_from_cg(7, 7, 8, 8), None);
        assert_eq!(
            layout_from_cg(
                ALPHA_PREMUL_FIRST,
                ALPHA_PREMUL_FIRST | ORDER_16_LITTLE,
                8,
                32
            ),
            None
        );
    }

    #[test]
    fn little_endian_premultiplied_first_is_already_bgra() {
        // Hai pixel đục: đỏ và xanh lá, trong bộ nhớ B,G,R,A.
        let buf = [0, 0, 255, 255, 0, 255, 0, 255];
        let img = to_bgra(&buf, 2, 1, 8, lay(AlphaKind::PremultipliedFirst, true)).unwrap();
        assert_eq!(img.width, 2);
        assert_eq!(img.height, 1);
        assert_eq!(img.bgra, buf);
    }

    #[test]
    fn channel_orders_map_to_bgra() {
        // Một pixel B=10, G=20, R=30, A=255 trong từng bố cục.
        let want = vec![10, 20, 30, 255];
        let cases: [(PixelLayout, [u8; 4]); 8] = [
            (lay(AlphaKind::PremultipliedFirst, true), [10, 20, 30, 255]),
            (lay(AlphaKind::PremultipliedFirst, false), [255, 30, 20, 10]),
            (lay(AlphaKind::PremultipliedLast, false), [30, 20, 10, 255]),
            (lay(AlphaKind::PremultipliedLast, true), [255, 10, 20, 30]),
            (lay(AlphaKind::First, false), [255, 30, 20, 10]),
            (lay(AlphaKind::Last, false), [30, 20, 10, 255]),
            (lay(AlphaKind::NoneSkipFirst, true), [10, 20, 30, 0]),
            (lay(AlphaKind::NoneSkipLast, false), [30, 20, 10, 7]),
        ];
        for (layout, px) in cases {
            let img = to_bgra(&px, 1, 1, 4, layout).unwrap();
            assert_eq!(img.bgra, want, "{layout:?}");
        }
    }

    #[test]
    fn premultiplied_alpha_is_undone() {
        // Trắng 50% đã nhân alpha: 128,128,128,128 → 255,255,255,128.
        let img = to_bgra(
            &[128, 128, 128, 128],
            1,
            1,
            4,
            lay(AlphaKind::PremultipliedFirst, true),
        )
        .unwrap();
        assert_eq!(img.bgra, vec![255, 255, 255, 128]);
        // Đỏ 25%: R = 64 → 255 (làm tròn), không vượt 255.
        let img = to_bgra(
            &[0, 0, 64, 64],
            1,
            1,
            4,
            lay(AlphaKind::PremultipliedFirst, true),
        )
        .unwrap();
        assert_eq!(img.bgra, vec![0, 0, 255, 64]);
        // Giá trị hỏng (màu > alpha) bị kẹp ở 255.
        let img = to_bgra(
            &[200, 0, 0, 100],
            1,
            1,
            4,
            lay(AlphaKind::PremultipliedFirst, true),
        )
        .unwrap();
        assert_eq!(img.bgra, vec![255, 0, 0, 100]);
        // Alpha 0: trong suốt hẳn, màu 0.
        let img = to_bgra(
            &[9, 9, 9, 0],
            1,
            1,
            4,
            lay(AlphaKind::PremultipliedFirst, true),
        )
        .unwrap();
        assert_eq!(img.bgra, vec![0, 0, 0, 0]);
        // Alpha chưa nhân thì giữ nguyên màu.
        let img = to_bgra(&[0, 0, 64, 64], 1, 1, 4, lay(AlphaKind::First, true)).unwrap();
        assert_eq!(img.bgra, vec![0, 0, 64, 64]);
    }

    #[test]
    fn row_padding_is_skipped_and_rows_stay_top_down() {
        // 2×2, mỗi hàng 12 byte (4 byte đệm cuối hàng).
        let buf = [
            1, 2, 3, 255, 4, 5, 6, 255, 0xEE, 0xEE, 0xEE, 0xEE, //
            7, 8, 9, 255, 10, 11, 12, 255, 0xEE, 0xEE, 0xEE, 0xEE,
        ];
        let img = to_bgra(&buf, 2, 2, 12, lay(AlphaKind::PremultipliedFirst, true)).unwrap();
        assert_eq!(
            img.bgra,
            vec![1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]
        );
        assert!(img.is_valid());
        // Hàng cuối không cần phần đệm.
        let short = &buf[..20];
        assert!(to_bgra(short, 2, 2, 12, lay(AlphaKind::PremultipliedFirst, true)).is_ok());
    }

    #[test]
    fn bad_buffers_are_errors_not_panics() {
        let l = lay(AlphaKind::PremultipliedFirst, true);
        assert!(to_bgra(&[0; 4], 0, 1, 4, l).is_err());
        assert!(to_bgra(&[0; 4], 1, 0, 4, l).is_err());
        // Hàng ngắn hơn width × 4.
        assert!(to_bgra(&[0; 8], 2, 1, 4, l).is_err());
        // Bộ đệm thiếu.
        assert!(to_bgra(&[0; 7], 2, 1, 8, l).is_err());
        assert!(to_bgra(&[0; 12], 2, 2, 8, l).is_err());
        // Tràn số.
        assert!(to_bgra(&[0; 4], usize::MAX, 2, usize::MAX, l).is_err());
    }

    #[test]
    fn pixel_size_follows_scale() {
        assert_eq!(
            pixel_size(r(0.0, 0.0, 1512.0, 982.0), 2.0),
            Some((3024, 1964))
        );
        assert_eq!(
            pixel_size(r(-1920.0, 0.0, 1920.0, 1080.0), 1.0),
            Some((1920, 1080))
        );
        assert_eq!(
            pixel_size(r(0.0, 0.0, 1280.0, 800.0), 1.5),
            Some((1920, 1200))
        );
        assert_eq!(pixel_size(r(0.0, 0.0, 0.0, 800.0), 2.0), None);
        assert_eq!(pixel_size(r(0.0, 0.0, 100.0, -1.0), 2.0), None);
        assert_eq!(pixel_size(r(0.0, 0.0, 100.0, 100.0), 0.0), None);
        assert_eq!(pixel_size(r(0.0, 0.0, f64::NAN, 100.0), 1.0), None);
    }

    fn img(w: u32, h: u32) -> Image {
        Image {
            width: w,
            height: h,
            bgra: vec![0; (w * h * 4) as usize],
        }
    }

    #[test]
    fn shot_keeps_frame_and_scale_when_sizes_match() {
        let f = r(0.0, -1440.0, 2560.0, 1440.0);
        let s = shot_from_frame(img(2560, 1440), f, 1.0, 7).unwrap();
        assert_eq!(s.frame_pt, f);
        assert_eq!(s.scale, 1.0);
        assert_eq!(s.display_id, 7);
        let f = r(0.0, 0.0, 1512.0, 982.0);
        let s = shot_from_frame(img(3024, 1964), f, 2.0, 1).unwrap();
        assert_eq!((s.image.width, s.image.height), (3024, 1964));
        assert_eq!(s.scale, 2.0);
        // Kích thước ảnh = khung × hệ số.
        assert_eq!(
            pixel_size(s.frame_pt, s.scale),
            Some((s.image.width, s.image.height))
        );
    }

    #[test]
    fn shot_scale_follows_real_image_when_reported_scale_is_wrong() {
        let f = r(0.0, 0.0, 1512.0, 982.0);
        let s = shot_from_frame(img(3024, 1964), f, 1.0, 1).unwrap();
        assert_eq!(s.scale, 2.0);
    }

    #[test]
    fn shot_rejects_invalid_image_or_frame() {
        let f = r(0.0, 0.0, 10.0, 10.0);
        let bad = Image {
            width: 2,
            height: 2,
            bgra: vec![0; 3],
        };
        assert!(matches!(
            shot_from_frame(bad, f, 1.0, 1),
            Err(CaptureError::Failed(_))
        ));
        assert!(matches!(
            shot_from_frame(img(10, 10), r(0.0, 0.0, 0.0, 10.0), 1.0, 1),
            Err(CaptureError::Failed(_))
        ));
    }

    #[test]
    fn display_at_picks_containing_then_nearest() {
        let frames = [
            r(0.0, 0.0, 1512.0, 982.0),
            r(1512.0, -98.0, 1920.0, 1080.0),
            r(-1440.0, 82.0, 1440.0, 900.0),
        ];
        assert_eq!(display_at(&frames, 10.0, 10.0), Some(0));
        assert_eq!(display_at(&frames, 1600.0, -50.0), Some(1));
        assert_eq!(display_at(&frames, -1.0, 500.0), Some(2));
        // Cạnh trái/trên thuộc màn hình, cạnh phải/dưới thuộc màn hình kế.
        assert_eq!(display_at(&frames, 1512.0, 10.0), Some(1));
        assert_eq!(display_at(&frames, 0.0, 0.0), Some(0));
        // Ngoài mọi màn hình: gần nhất.
        assert_eq!(display_at(&frames, 700.0, 2000.0), Some(0));
        assert_eq!(display_at(&frames, 5000.0, 0.0), Some(1));
        assert_eq!(display_at(&[], 0.0, 0.0), None);
        assert_eq!(display_at(&frames[..1], f64::NAN, 0.0), Some(0));
    }

    #[test]
    fn pick_display_uses_cursor_then_main_display() {
        // Màn hình phụ bên trái (gốc âm) và phía trên (y âm); màn hình chính không đứng đầu.
        let displays = [
            (5, r(-1440.0, 82.0, 1440.0, 900.0)),
            (9, r(0.0, -1440.0, 2560.0, 1440.0)),
            (1, r(0.0, 0.0, 1512.0, 982.0)),
        ];
        assert_eq!(pick_display(&displays, Some((-10.0, 500.0)), 1), Some(0));
        assert_eq!(pick_display(&displays, Some((100.0, -5.0)), 1), Some(1));
        assert_eq!(pick_display(&displays, Some((100.0, 100.0)), 1), Some(2));
        // Con trỏ ngoài mọi màn hình: gần nhất, không phải màn hình chính.
        assert_eq!(pick_display(&displays, Some((-3000.0, 500.0)), 1), Some(0));
        // Không có con trỏ: màn hình chính, không phải chỉ số 0.
        assert_eq!(pick_display(&displays, None, 1), Some(2));
        assert_eq!(pick_display(&displays, None, 9), Some(1));
        // Màn hình chính không có trong danh sách: màn hình đầu tiên.
        assert_eq!(pick_display(&displays, None, 42), Some(0));
        assert_eq!(pick_display(&[], None, 1), None);
        assert_eq!(pick_display(&[], Some((0.0, 0.0)), 1), None);
    }

    #[test]
    fn capture_errors_have_localised_notices() {
        use darkshot_core::i18n::NOTICE_TITLES;
        for l in Lang::ALL {
            let (title, body) = capture_error_notice(l, &CaptureError::PermissionDenied);
            assert_eq!(title, t(l, Msg::MacScreenPermissionTitle));
            assert_eq!(body, t(l, Msg::MacScreenPermissionBody));
            let (title, body) = capture_error_notice(l, &CaptureError::NoDisplay);
            assert_eq!(title, t(l, Msg::CaptureFailedTitle));
            assert_eq!(body, t(l, Msg::MacNoDisplayBody));
            // Chữ kỹ thuật của lỗi không bao giờ hiện cho người dùng.
            let (title, body) =
                capture_error_notice(l, &CaptureError::Failed("SCStreamError -3801".into()));
            assert_eq!(title, t(l, Msg::CaptureFailedTitle));
            assert_eq!(body, t(l, Msg::MacCaptureFailedBody));
            assert!(!body.contains("-3801"));
        }
        assert!(NOTICE_TITLES.contains(&Msg::CaptureFailedTitle));
    }

    // Chạm hệ thống thật: chủ dự án chạy tay (quyền Screen Recording do Terminal giữ khi chạy
    // ngoài gói .app).
    #[test]
    #[ignore = "chụp màn hình thật; chạy tay: cargo test -p darkshot-macos --bin darkshot-macos -- --ignored --test-threads=1"]
    fn captures_every_display_at_physical_size() {
        let shots = capture_displays(false).expect("cần quyền Screen Recording");
        assert!(!shots.is_empty());
        for s in &shots {
            assert!(s.image.is_valid(), "{}", s.display_id);
            // Kích thước ảnh = khung (điểm) × hệ số.
            assert_eq!(
                pixel_size(s.frame_pt, s.scale),
                Some((s.image.width, s.image.height)),
                "{:?} @ {}",
                s.frame_pt,
                s.scale
            );
            // Ảnh màn hình đục: không có pixel trong suốt.
            assert!(s.image.bgra.chunks(4).all(|p| p[3] == 255));
        }
        let ids: std::collections::HashSet<u32> = shots.iter().map(|s| s.display_id).collect();
        assert_eq!(ids.len(), shots.len(), "mỗi màn hình một ảnh");
        let (x, y) = cursor_location().expect("vị trí con trỏ");
        let frames: Vec<RectF> = shots.iter().map(|s| s.frame_pt).collect();
        assert!(display_at(&frames, x, y).is_some());
    }

    #[test]
    #[ignore = "đọc quyền Screen Recording thật; chạy tay: cargo test -p darkshot-macos --bin darkshot-macos -- --ignored --test-threads=1"]
    fn missing_permission_is_an_error_not_an_empty_image() {
        if screen_capture_allowed() {
            assert!(capture_displays(true).is_ok());
        } else {
            assert_eq!(capture_displays(true), Err(CaptureError::PermissionDenied));
        }
    }
}
