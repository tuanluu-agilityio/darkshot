//! Điểm vào của darkshot cho macOS (Apple silicon). Khung rỗng của Task 100;
//! các phần nền tảng được thêm ở Task 103 trở đi (thiết kế đợt M).

// Các module nền tảng chưa được vòng lặp ứng dụng gọi tới; `allow(dead_code)` chỉ đặt trên
// từng khai báo module, gỡ khi đã nối.
#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod capture;
#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod clipboard;
#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod coords;
#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod lang;
#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod notify;
#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod paths;
#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod secret;
#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod timeutil;
#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod tray;

#[cfg(target_os = "macos")]
fn main() {
    println!("darkshot for macOS: skeleton only (Task 100).");
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("darkshot-macos runs on macOS only.");
    std::process::exit(1);
}
