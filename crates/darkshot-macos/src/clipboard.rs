//! Clipboard của macOS (`NSPasteboard` chung): chép link (chuỗi) và ảnh (PNG).
//!
//! Luồng: `NSPasteboard` không được AppKit đánh dấu chỉ-luồng-chính (tiêu đề `NSPasteboard.h`
//! không có `NS_SWIFT_UI_ACTOR`, và `objc2-app-kit` không gắn `MainThreadOnly` cho lớp này), nên
//! ta ghi ngay trên luồng gọi, trong một autorelease pool riêng. Không chuyển về luồng chính nên
//! không thể deadlock dù gọi từ luồng chính hay luồng nền. Một khóa trong tiến trình giữ cho
//! cặp `clearContents` + ghi không bị xen giữa hai lần chép đồng thời.
//!
//! Mã hóa PNG (việc nặng) chạy trên luồng gọi trước khi khóa: bên gọi không nên chép ảnh lớn
//! từ luồng chính.

use darkshot_core::actions::Clipboard;
use darkshot_core::image::{encode_png, Image};
use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSPasteboard, NSPasteboardType, NSPasteboardTypePNG, NSPasteboardTypeString};
use objc2_foundation::{NSData, NSString};
use std::sync::Mutex;

/// Khóa ghi clipboard trong tiến trình.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Clipboard thật của hệ thống. Ghi định dạng `public.utf8-plain-text` cho chữ và
/// `public.png` cho ảnh (chỉ PNG, không thêm TIFF: các ứng dụng hiện nay đều đọc PNG).
#[derive(Debug, Clone, Copy, Default)]
pub struct PasteboardClipboard;

impl Clipboard for PasteboardClipboard {
    fn set_text(&self, s: &str) -> Result<(), String> {
        // SAFETY: hằng `NSPasteboardTypeString` do AppKit định nghĩa, sống suốt tiến trình và
        // không bao giờ bị ghi.
        let ty = unsafe { NSPasteboardTypeString };
        replace_with(ty, |pb, ty| {
            pb.setString_forType(&NSString::from_str(s), ty)
        })
    }

    fn set_image(&self, img: &Image) -> Result<(), String> {
        if !img.is_valid() {
            return Err("clipboard: invalid image".into());
        }
        let png = encode_png(img).map_err(|e| format!("clipboard: PNG encode failed ({e:?})"))?;
        // SAFETY: như `NSPasteboardTypeString` ở trên.
        let ty = unsafe { NSPasteboardTypePNG };
        replace_with(ty, |pb, ty| {
            pb.setData_forType(Some(&NSData::with_bytes(&png)), ty)
        })
    }
}

/// Chữ thuần trên clipboard chung (cho ô chữ của lớp phủ đánh dấu); `None` khi không có chữ.
/// Chỉ đọc, không đổi clipboard.
pub fn paste_text() -> Option<String> {
    autoreleasepool(|_| {
        let pb = NSPasteboard::generalPasteboard();
        // SAFETY: như `NSPasteboardTypeString` ở trên.
        let ty = unsafe { NSPasteboardTypeString };
        pb.stringForType(ty).map(|s| s.to_string())
    })
}

/// Xóa clipboard chung rồi ghi một định dạng bằng `write`.
fn replace_with(
    ty: &NSPasteboardType,
    write: impl FnOnce(&NSPasteboard, &NSPasteboardType) -> bool,
) -> Result<(), String> {
    let _g = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    autoreleasepool(|_| {
        let pb = NSPasteboard::generalPasteboard();
        // `clearContents` biến tiến trình này thành chủ clipboard; trả số đếm thay đổi mới.
        pb.clearContents();
        if write(&pb, ty) {
            Ok(())
        } else {
            Err("clipboard: NSPasteboard write failed".to_string())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn invalid_image_is_rejected_without_touching_pasteboard() {
        let bad = Image {
            width: 2,
            height: 2,
            bgra: vec![0; 3],
        };
        assert!(PasteboardClipboard.set_image(&bad).is_err());
    }

    fn pbpaste() -> String {
        let out = Command::new("pbpaste").output().expect("pbpaste");
        String::from_utf8(out.stdout).expect("utf-8")
    }

    #[test]
    #[ignore = "ghi đè clipboard thật của máy; chạy tay: cargo test -p darkshot-macos --bin darkshot-macos -- --ignored --test-threads=1"]
    fn pasteboard_text_round_trip() {
        let link = "https://img.example.com/d30/Ab12.png";
        PasteboardClipboard.set_text(link).unwrap();
        assert_eq!(pbpaste(), link);
        let vi = "Vi\u{1ec7}t d\u{f2}ng 2 \u{1f600}";
        PasteboardClipboard.set_text(vi).unwrap();
        assert_eq!(pbpaste(), vi);
    }

    #[test]
    #[ignore = "ghi đè clipboard thật của máy; chạy tay: cargo test -p darkshot-macos --bin darkshot-macos -- --ignored --test-threads=1"]
    fn pasteboard_image_is_png() {
        let img = Image {
            width: 3,
            height: 2,
            bgra: vec![
                0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255, //
                255, 255, 255, 255, 0, 0, 0, 255, 128, 128, 128, 255,
            ],
        };
        PasteboardClipboard.set_image(&img).unwrap();
        let types: Vec<String> = autoreleasepool(|_| {
            NSPasteboard::generalPasteboard()
                .types()
                .map(|a| a.to_vec().iter().map(|t| t.to_string()).collect())
                .unwrap_or_default()
        });
        assert!(types.iter().any(|t| t == "public.png"), "{types:?}");
        // Dữ liệu PNG đọc lại bắt đầu bằng chữ ký PNG.
        let data = autoreleasepool(|_| {
            // SAFETY: hằng AppKit, sống suốt tiến trình.
            let ty = unsafe { NSPasteboardTypePNG };
            NSPasteboard::generalPasteboard()
                .dataForType(ty)
                .map(|d| d.to_vec())
        })
        .expect("PNG data");
        assert_eq!(&data[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    #[ignore = "ghi đè clipboard thật của máy; chạy tay: cargo test -p darkshot-macos --bin darkshot-macos -- --ignored --test-threads=1"]
    fn pasteboard_writes_from_background_thread() {
        let h = std::thread::spawn(|| PasteboardClipboard.set_text("darkshot-bg-thread"));
        h.join().unwrap().unwrap();
        assert_eq!(pbpaste(), "darkshot-bg-thread");
    }
}
