//! Điểm vào của darkshot cho macOS (Apple silicon). Khung rỗng của Task 100;
//! các phần nền tảng được thêm ở Task 103 trở đi (thiết kế đợt M).

#[cfg(target_os = "macos")]
fn main() {
    println!("darkshot for macOS: skeleton only (Task 100).");
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("darkshot-macos runs on macOS only.");
    std::process::exit(1);
}
