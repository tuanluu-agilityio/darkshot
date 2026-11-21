//! Kiểm thử chạy tay lớp phủ chọn vùng (Task 105). Hiện cửa sổ thật che mọi màn hình nên luôn
//! `#[ignore]`; chủ dự án chạy:
//!
//!   cargo test -p darkshot-macos --test overlay_gui -- --ignored
//!
//! Lớp phủ hiện ảnh dải màu (xanh dương tăng dần sang phải, xanh lá tăng dần xuống dưới); nhấn
//! `Esc` để đóng. Không cần quyền Ghi màn hình (không chụp thật). Lưới an toàn tự đóng sau 120 s.
#![cfg(target_os = "macos")]

use std::process::Command;

#[test]
#[ignore = "hiện lớp phủ thật che màn hình; chạy tay rồi nhấn Esc"]
fn overlay_shows_synthetic_shot_and_closes_with_esc() {
    let out = Command::new(env!("CARGO_BIN_EXE_darkshot-macos"))
        .arg("--overlay-selftest")
        .output()
        .expect("chạy darkshot-macos --overlay-selftest");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout} {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("overlay: cancelled"), "{stdout}");
}
