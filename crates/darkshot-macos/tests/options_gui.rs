//! Kiểm thử chạy tay cửa sổ Tùy chọn (Task 107). Hiện cửa sổ thật nên luôn `#[ignore]`; chủ dự án
//! chạy riêng (lệnh kiểm thử chạy tay khác dùng `--bin darkshot-macos` để không chạy tệp này):
//!
//!   cargo test -p darkshot-macos --test options_gui -- --ignored
//!
//! Cửa sổ hiện với cấu hình MẪU (không đọc cấu hình thật, không Keychain, không ghi tệp): bốn tab
//! tối; bấm Hủy (hoặc `Esc`, hoặc nút đỏ) để đóng. Bấm "Kiểm tra kết nối" sẽ gọi mạng tới địa chỉ
//! mẫu không tồn tại và báo lỗi: đó là hành vi đúng.
#![cfg(target_os = "macos")]

use std::process::Command;

#[test]
#[ignore = "hiện cửa sổ thật; chạy tay rồi bấm Hủy"]
fn options_window_opens_and_cancels() {
    let out = Command::new(env!("CARGO_BIN_EXE_darkshot-macos"))
        .arg("--options-selftest")
        .output()
        .expect("chạy darkshot-macos --options-selftest");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout} {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("options: cancelled"), "{stdout}");
}
