# Task 101: Chuyển logic thuần của lớp Windows sang core (đợt M, bước M2b)

**Agent:** `builder` · **Model:** sonnet · **Nhánh:** `task/101-core-shared-logic` (từ `feat/macos-port`) · **Rà soát:** `reviewer` (opus) · **Cổng Windows:** job `windows` của `.github/workflows/check.yml` (không có máy Windows, quyết định 130)

Thiết kế: `docs/superpowers/specs/2026-11-21-macos-port-design.md` mục 3 (quyết định kiến trúc 1) và mục 2. Global Constraints: kế hoạch `docs/superpowers/plans/2026-11-21-macos-port.md` (mục cùng tên, chép lại bên dưới).

## Mục tiêu

Bản macOS cần đúng các luật này nên chúng phải nằm ở `darkshot-core` với test, không sao chép. **Đây là phép chuyển, không đổi hành vi.** Mọi test hiện có của các hàm này đi theo, nguyên văn.

## Cần đọc

`crates/darkshot-windows/src/windows_main.rs` (các mục bên dưới, kèm khối `#[cfg(test)]` của nó), `capture.rs`, `paths.rs`, `settings.rs` (`OpenReason`), `crates/darkshot-core/src/{geometry,last_capture,update,config,actions}.rs`, `lib.rs`.

## Phạm vi: chuyển những gì, đến đâu

| Từ (lớp Windows) | Sang (core) | Ghi chú |
|---|---|---|
| `windows_main.rs`: `CaptureGate`, `GateEvent`, `GateAction`, `gate_step`, `CaptureKind`, `kind_of`, `next_kind`, `enumerates_windows` | **module mới** `capture_gate.rs` | `kind_of` dùng `config::HotkeyRole` |
| `windows_main.rs`: `remember_after`, `RepeatPlan`, `repeat_plan`, `last_capture_after`, `read_last_capture`, `write_last_capture`, hằng `LAST_CAPTURE_MAX_BYTES` | `last_capture.rs` | `repeat_plan` gọi `to_screen_coords` (xem dòng dưới) |
| `capture.rs`: `to_image_coords`, `to_screen_coords`, `cursor_draw_origin`, `clip_to_image`; `windows_main.rs`: `full_screen_rect` | `geometry.rs` | `clip_to_image` và `full_screen_rect` đang `fn` riêng tư → thành `pub` |
| `windows_main.rs`: `update_outcome`, `update_after_check` | `update.rs` | |
| `windows_main.rs`: `merge_reason`, `lang_after_save`, `info_windows_stale`, `history_window_stale`; `settings.rs`: `OpenReason` | **module mới** `app_flow.rs` | `OpenReason` có 27 chỗ dùng ở lớp Windows: để lớp Windows giữ tên cũ bằng `pub use darkshot_core::app_flow::OpenReason;` trong `settings.rs` |
| `windows_main.rs`: `Args`, `parse_args` | `app_flow.rs` | |
| `paths.rs`: `PROFILE_ENV`, `Known` (kèm `sub`), `override_from`, `profile_override`, phần "có thư mục cách ly" của `resolve`, `history_dir_in`, `steps_dir_in`, `config_path_in`, `last_capture_path_in` | **module mới** `dirs.rs` | Xem "Interfaces" |

**Giữ nguyên ở lớp Windows (KHÔNG chuyển):** `StartupMode`, `startup_mode`, `short_hash`, `MUTEX_NAME` (tên mutex `Local\…`, WER, khóa Run là chuyện Windows); `tray_click_opens_menu` (nhận mã thông điệp Win32); `AppEvent`; mọi hàm gọi Win32 (`known_folder`, `local_now`, `appdata_dir`…); `remember_last_capture` (dùng `thread_local` và `paths`). Nếu gặp hàm khác trông "thuần" mà không có trong bảng thì **không chuyển**, ghi vào báo cáo để task sau xử lý.

## Files

Tạo `crates/darkshot-core/src/{capture_gate,app_flow,dirs}.rs`. Sửa `lib.rs` (khai báo module, giữ thứ tự chữ cái), `geometry.rs`, `last_capture.rs`, `update.rs`. Lớp Windows chỉ sửa để dùng lại: `windows_main.rs`, `capture.rs`, `paths.rs`, `settings.rs` và nơi nào gọi các hàm đã chuyển (dùng `grep` rồi cập nhật `use`; tránh để lại bản trùng). Không sửa tệp ngoài danh sách này trừ khi trình biên dịch buộc phải (lớp Windows không biên dịch được trên máy này, xem "Cách kiểm chứng").

## Interfaces

Giữ **nguyên chữ ký, tên, tên biến thể, thứ tự biến thể** như trong mã hiện tại; chỉ đổi `pub fn` nơi đang riêng tư. Riêng `dirs.rs`, vì `resolve` hiện trộn logic thuần với Win32, tách đôi:

```rust
// darkshot-core/src/dirs.rs
pub const PROFILE_ENV: &str = "DARKSHOT_PROFILE_DIR";
pub enum Known { Roaming, Local, Pictures }              // Clone, Copy, Debug, PartialEq, Eq
pub fn override_from(v: Option<std::ffi::OsString>) -> Option<PathBuf>;
pub fn profile_override() -> Option<PathBuf>;           // đọc biến môi trường PROFILE_ENV
/// `Some(<dir>/Roaming|Local|Pictures)` khi có thư mục cách ly (không tạo thư mục), ngược lại `None`.
pub fn sandboxed(override_dir: Option<&Path>, known: Known) -> Option<PathBuf>;
pub fn history_dir_in(local: &Path) -> PathBuf;          // <local>/darkshot/history
pub fn steps_dir_in(local: &Path) -> PathBuf;            // <local>/darkshot/steps/current
pub fn config_path_in(appdata: &Path) -> PathBuf;        // <appdata>/darkshot/config.json
pub fn last_capture_path_in(appdata: &Path) -> PathBuf;  // <appdata>/darkshot/last_capture.json
```

`paths::resolve(override_dir, known)` ở lớp Windows thành: `dirs::sandboxed(override_dir, known)` nếu `Some`, còn lại giữ nguyên nhánh hỏi hệ thống như cũ. `paths::history_dir()`, `config_path()`… giữ tên, chỉ gọi `dirs::*_in`.

## Kiểm thử (viết trước)

1. **Mỗi test hiện có của các hàm trong bảng chuyển nguyên văn theo hàm** (tìm trong `mod tests` của `windows_main.rs`, `capture.rs`, `paths.rs`, `settings.rs`); chúng phải xanh ở core. Không viết lại cho "dễ qua".
2. Thêm cho `dirs`: `sandboxed(None, k)` là `None`; `sandboxed(Some(d), k)` ra `d.join("Roaming"|"Local"|"Pictures")`; `override_from` với giá trị rỗng, thiếu, có giá trị; các `*_in` dùng `Path::join` nên đúng trên mọi hệ điều hành (không viết `\` cứng trong test).
3. Test đọc/ghi `last_capture` dùng thư mục tạm riêng, dọn sau khi xong; kiểm tệp lớn hơn `LAST_CAPTURE_MAX_BYTES` trả `None`.
4. Test đường dẫn không dùng ký tự `\` làm dấu phân tách (đã có tiền lệ: Task đợt M, sửa `history::tests::entry_paths`).

TDD: commit `test:` (test mới của `dirs`, test chuyển sang core còn **đỏ vì chưa có hàm**) rồi `feat:` (chuyển mã, test xanh). Nếu việc chuyển thuần túy khiến commit `test:` không có test nào đỏ thì ghi rõ trong báo cáo và gộp một commit.

## Cách kiểm chứng (quan trọng)

Máy làm việc là macOS, **không biên dịch được `darkshot-windows`** (đã thử, `ring` cần MSVC). Do đó:
- Tự kiểm bằng `scripts/check.sh` (fmt, test workspace gồm core, clippy, các luật dự án). Lớp Windows chỉ là stub trên Mac nên cổng này **không** chứng minh lớp Windows còn biên dịch.
- Mọi thay đổi trong `darkshot-windows` phải được viết cực kỳ cẩn thận: soát từng `use`, từng chữ ký, từng chỗ gọi (`grep -n`), `pub`/riêng tư, đường dẫn module. Sau khi soát, **tự đọc lại diff một lần** và liệt kê trong báo cáo từng tệp Windows đã sửa kèm lý do.
- Phiên điều phối đẩy nhánh để job `windows` chạy `check.ps1`; chỉ coi là xong khi job đó xanh. Không tự đẩy.

## Global Constraints

Như mục cùng tên của `docs/superpowers/plans/2026-11-21-macos-port.md` (chép đủ ở đây):
- Core không phụ thuộc crate `windows`, `native-tls`, `schannel`, `rustls-platform-verifier`; không thêm phụ thuộc mới; mọi chữ người dùng thấy qua `darkshot_core::i18n`.
- Không đổi hành vi Windows. Không sửa spec đã duyệt.
- Không panic trên đường chạy thật (`panic = "abort"` ở release). Ghi tệp qua tệp tạm rồi đổi tên.
- Thời gian máy do nền tảng đưa vào; core không gọi API thời gian hệ điều hành.
- Repo công khai: không khóa, không cấu hình thật, không dữ liệu cá nhân; không đọc, ghi hay in khóa R2.
- QA không giả lập chuột/phím thật.
- Commit kết thúc bằng dòng `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.

## Hoàn tất

`cargo fmt --all`; `./scripts/check.sh` → `ALL CHECKS PASSED`. Báo cáo: danh sách hàm đã chuyển, tệp Windows đã sửa, hàm "trông thuần" nhưng giữ lại, và số token. Không sửa `memory-bank/` (phiên điều phối làm).
