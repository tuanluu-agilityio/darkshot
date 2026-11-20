//! Điểm vào của darkshot cho Windows. Toàn bộ mã nằm trong `windows_main.rs`;
//! trên hệ điều hành khác crate chỉ biên dịch một `main` báo lỗi, để
//! `cargo test --workspace` chạy được ở mọi nơi (quyết định 128).
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
include!("windows_main.rs");

#[cfg(not(windows))]
fn main() {
    eprintln!("darkshot-windows runs on Windows only.");
    std::process::exit(1);
}
