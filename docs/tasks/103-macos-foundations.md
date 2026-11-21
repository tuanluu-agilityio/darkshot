# Task 103: nền móng lớp macOS (đợt M, bước M3a)

**Agent:** `builder` · **Model:** opus · **Nhánh:** `task/103-macos-foundations` (từ `feat/macos-port`) · **Rà soát:** `reviewer` (fable) · **Cổng:** job `macos` và `windows` của `.github/workflows/check.yml` (Windows chỉ biên dịch phần khung của crate Mac) + QA tay của chủ dự án (dưới)

Thiết kế: `docs/superpowers/specs/2026-11-21-macos-port-design.md` (mục 3, 4, 5 và 9). Quyết định 127–137 (đặc biệt 134–137). Tiền lệ cách làm: `docs/tasks/101-core-shared-logic.md`.

## Mục tiêu

Dựng các viên gạch nền tảng nhỏ, tách bạch, mà Task 104–105 và M4 dựa vào: tọa độ Retina, đường dẫn dữ liệu, giờ địa phương, kho khóa bí mật (Keychain) và clipboard. **Chưa có cửa sổ, chưa có menu, chưa chụp màn hình.**

## Cần đọc

`crates/darkshot-core/src/{config,naming,image,actions,dirs,geometry}.rs` (`SecretStore`, `LocalTime`, `Image`, `encode_png`, `Clipboard`, `dirs::*_in`, `Rect`/`Point`), `crates/darkshot-windows/src/{paths,secret,clipboard}.rs` (cách Windows làm, để giữ cùng hành vi và thông điệp lỗi), `crates/darkshot-macos/` (khung từ Task 100).

## Files

Sửa `crates/darkshot-macos/Cargo.toml` (phụ thuộc theo `cfg(target_os = "macos")`: `darkshot-core`, `objc2`, `objc2-foundation`, `objc2-app-kit`, `dispatch2`, `security-framework`, `libc`; ghim `=`), `src/main.rs` (khai báo module dưới `cfg`). Tạo `src/{coords,paths,timeutil,secret,clipboard}.rs`.

## Interfaces

**`coords.rs` (thuần, không gọi Objective-C):** đổi giữa hệ Cocoa (điểm, gốc dưới trái màn hình chính, Y hướng lên) và hệ của core (pixel, gốc trên trái, Y hướng xuống).
```rust
pub struct RectF { pub x: f64, pub y: f64, pub w: f64, pub h: f64 }   // Clone, Copy, Debug, PartialEq
/// Khung màn hình Cocoa (gốc dưới trái) sang toạ độ toàn cục gốc trên trái, đơn vị điểm.
pub fn cocoa_to_top_left(frame: RectF, primary_height: f64) -> RectF;
/// Điểm trong view (đơn vị điểm, gốc trên trái của màn hình đó) sang pixel của ảnh màn hình đó.
pub fn pt_to_px(v: f64, scale: f64) -> i32;                 // làm tròn gần nhất
/// Vùng chọn (điểm) sang vùng pixel của ảnh `img_w`×`img_h`: gốc làm tròn xuống, cạnh xa làm tròn lên, kẹp vào ảnh; rỗng thì `None`.
pub fn selection_to_pixels(sel: RectF, scale: f64, img_w: i32, img_h: i32) -> Option<Rect>;
pub fn px_to_pt(v: i32, scale: f64) -> f64;
```
**`paths.rs`:** cùng bộ hàm như `darkshot-windows/src/paths.rs` nhưng cho macOS, dùng `darkshot_core::dirs` (`profile_override`, `sandboxed`, `*_in`): `appdata_dir()`/`local_appdata_dir()` đều là `~/Library/Application Support`, `pictures_dir()` là `~/Pictures`, `config_path()`, `last_capture_path()`, `history_dir()`, `steps_dir()`, `default_save_dir()` (= `~/Pictures/darkshot`). `HOME` thiếu thì dùng `std::env::temp_dir()`. `DARKSHOT_PROFILE_DIR` ghi đè (cách ly, giống Windows).
**`timeutil.rs`:** `pub fn local_now() -> LocalTime` (bằng `libc::localtime_r`, không panic; thất bại trả `LocalTime` 1970-01-01 00:00:00 hoặc tương đương và không crash).
**`secret.rs`:** `pub struct KeychainStore { ... }` implements `darkshot_core::config::SecretStore` (quyết định 136).
- `KeychainStore::new()` dùng service `darkshot`, tài khoản cố định `r2-secret-access-key`; `KeychainStore::with_service(service: &str)` cho test.
- `protect(plain)`: ghi `plain` vào Keychain (ghi đè mục cũ), trả `b"keychain:v1".to_vec()`; **không bao giờ** trả hay chứa khóa trong giá trị trả về.
- `unprotect(cipher)`: `cipher` khác dấu `keychain:v1` thì `Err`; đọc Keychain; thiếu mục hoặc bị từ chối thì `Err` (để luồng "mất khóa" của core xử lý).
- Lỗi là `String` không chứa khóa bí mật. Logic dấu/kiểm phải tách khỏi Keychain bằng một trait nhỏ `KeyBackend { get/set }` để test không chạm Keychain thật.
**`clipboard.rs`:** `pub struct PasteboardClipboard;` implements `darkshot_core::actions::Clipboard`: `set_text` ghi `NSPasteboardTypeString`; `set_image` kiểm `Image::is_valid()`, mã hóa PNG bằng core (`image::encode_png`), ghi `NSPasteboardTypePNG` (ghi thêm TIFF nếu cần để ứng dụng khác dán được; nêu lựa chọn trong báo cáo). Gọi được từ bất kỳ luồng nào (trait `Send + Sync`): nếu `NSPasteboard` cần luồng chính thì chuyển về luồng chính (đừng deadlock khi đã ở luồng chính).

## Kiểm thử (viết trước)

- `coords`: Retina 2x và 1x; màn hình thứ hai đặt trên/dưới/trái của màn hình chính (gốc Cocoa âm); chiều cao màn hình chính khác nhau; `selection_to_pixels` với biên lẻ (`x=10.25`, `w=100.5`) không mất pixel cạnh xa, kẹp ở mép, vùng rỗng/âm thành `None`; khứ hồi `px_to_pt`/`pt_to_px`.
- `paths`: các hàm `*_in` và nhánh `DARKSHOT_PROFILE_DIR` (cách ly) bằng thư mục tạm, dựng bằng `Path::join`, không `\` cứng.
- `timeutil`: trường trong khoảng hợp lệ (tháng 1–12, ngày 1–31, giờ 0–23…).
- `secret` (qua `KeyBackend` giả): khứ hồi; `unprotect` từ chối dấu lạ và dữ liệu rỗng; mục bị thiếu trả `Err`; lỗi không chứa nội dung khóa; `protect` ghi đè.
- Test chạm hệ thống `#[ignore]` (chạy tay: `cargo test -p darkshot-macos --bin darkshot-macos -- --ignored --test-threads=1`): `KeychainStore::with_service("darkshot-test-<pid>")` khứ hồi rồi **xóa mục thử**; `PasteboardClipboard` ghi văn bản rồi đọc lại bằng `pbpaste`, ghi ảnh nhỏ rồi kiểm UTI `public.png` có trên pasteboard.

## QA tay (chủ dự án)

1. `cargo test -p darkshot-macos --bin darkshot-macos -- --ignored --test-threads=1` trên máy thật: có thể có hộp thoại Keychain hỏi quyền; bấm Cho phép. Mọi test phải qua.
2. Mở Keychain Access, tìm `darkshot-test-…`: phải không còn sót mục thử.

## Review Focus

1. Khóa bí mật không bao giờ lọt vào giá trị trả về của `protect`, thông báo lỗi, log, `Debug`.
2. Công thức `selection_to_pixels` không làm mất hoặc dư pixel ở Retina và mép màn hình; lật trục Y đúng khi màn hình phụ nằm trên/dưới màn hình chính.
3. Không panic trên đường thật; mọi `unsafe` có `// SAFETY:`; phụ thuộc đều dưới `cfg(target_os = "macos")`, job `windows` vẫn xanh.
4. Clipboard không deadlock khi gọi từ luồng chính, không gọi AppKit từ luồng nền.

## Global Constraints

Như mục cùng tên của `docs/superpowers/plans/2026-11-21-macos-port.md` (chép đủ ở đây):
- `darkshot-core` không phụ thuộc crate `windows`, `native-tls`, `schannel`, `rustls-platform-verifier`, không `cfg` theo hệ điều hành. Sửa core chỉ để thêm chuỗi i18n (đủ hai bản Việt và English) hoặc một trừu tượng thật sự cần; mọi chữ người dùng thấy qua `darkshot_core::i18n`, không viết cứng.
- **Mọi phụ thuộc theo macOS nằm dưới `[target.'cfg(target_os = "macos")'.dependencies]` và mọi mã Objective-C/FFI dưới `#[cfg(target_os = "macos")]`**: job `windows` của CI vẫn biên dịch crate `darkshot-macos` ở dạng khung và không được hỏng. Ghim phiên bản chính xác (`=x.y.z`) cho mọi crate `objc2*`, `block2`, `dispatch2`, `security-framework`, `libc`.
- Mỗi API Objective-C/C bọc trong module nhỏ; mỗi khối `unsafe` có `// SAFETY:` nêu bất biến. Không `unwrap`/`expect`/`panic!` trên đường chạy thật (`panic = "abort"`); mọi lỗi là `Result` hoặc thông báo cho người dùng.
- Không bao giờ chặn luồng chính bằng việc lâu (chụp màn hình, mã hóa PNG, ghi tệp, mạng); việc nền báo về luồng chính qua hàng đợi chính. Không gọi AppKit từ luồng nền.
- Khóa R2 chỉ qua `SecretStore`; không vào log, thông báo lỗi, tệp cấu hình, tham số dòng lệnh, kết quả test. Test không dùng khóa thật hay tên Keychain thật của ứng dụng. Không đọc, ghi hay in khóa R2 của chủ dự án.
- Repo công khai: không đưa chứng chỉ, Team ID, profile, đường dẫn cá nhân hay ảnh chụp màn hình thật vào repo.
- Logic thuần (tọa độ, ánh xạ, bố cục, luật) viết trước bằng test (TDD) và nằm ở nơi test chạy được trên máy không có GUI; phần cần GUI kiểm bằng test `#[ignore]` chạy tay và danh sách QA tay bên dưới. **QA không giả lập chuột/phím thật** khi chủ dự án dùng máy (quyết định 126): agent không tự bấm chuột, không tự cấp quyền hệ thống.
- Chạy `rustfmt`/`cargo fmt --all`, `./scripts/check.sh` phải in `ALL CHECKS PASSED`. Không sửa `memory-bank/` (phiên điều phối làm). Commit kết thúc bằng dòng `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`; không tự đẩy.

## Phụ thuộc

Task 101, 102a–102e (đã xong).

## Hoàn tất

`cargo fmt --all`; `./scripts/check.sh` → `ALL CHECKS PASSED`. Báo cáo ngắn: mục đã làm và nơi đặt; chuỗi i18n đã thêm; API Objective-C/C dùng và phiên bản đã ghim; mọi chỗ lệch gói và lý do; điều không kiểm được tự động (để QA tay); số token. Chưa tự đẩy nhánh.
