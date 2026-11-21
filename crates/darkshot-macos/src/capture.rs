//! Chụp từng màn hình bằng ScreenCaptureKit (macOS 14+, quyết định 134, 135) và quyền Screen
//! Recording.
//!
//! Phần thuần (đổi bộ đệm `CGImage` sang BGRA, kích thước và màn hình chứa một điểm) có kiểm
//! thử chạy trên máy không có GUI.

use crate::coords::{pt_to_px, RectF};
use darkshot_core::i18n::{t, Lang, Msg};
use darkshot_core::image::Image;

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
}
