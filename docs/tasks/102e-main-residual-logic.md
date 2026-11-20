# Task 102e: chuyển logic thuần còn lại của `windows_main.rs` sang core (đợt M, M2c, 5/5)

**Agent:** `builder` · **Model:** sonnet · **Nhánh:** `task/102e-main-residual-logic` (từ `feat/macos-port`) · **Rà soát:** `reviewer` (opus) · **Cổng Windows:** job `windows` của `.github/workflows/check.yml` (quyết định 130)

Thiết kế: `docs/superpowers/specs/2026-11-21-macos-port-design.md` mục 3 (quyết định kiến trúc 1). Tiền lệ: `docs/tasks/101-core-shared-logic.md`. Quyết định 128, 131, 132.

## Mục tiêu

Builder Task 101 ghi nhận một số hàm thuần ngoài bảng cũ; Mac cần cùng luật thông báo, hẹn giờ và nhớ khung chụp. **Phép chuyển, không đổi hành vi Windows.**

## Cần đọc

`crates/darkshot-windows/src/windows_main.rs` (cả khối test), `crates/darkshot-core/src/{app_flow,capture_gate,actions,config,color,i18n}.rs`.

## Phạm vi: chuyển những gì

Ứng viên (đã nhận diện, soát từng hàm theo quy tắc, mục nào còn dính Win32 thì tách lõi hoặc giữ lại):
`hotkeys_taken_notice`, `step_error_notice`, `pin_origin`, `pin_failure_notice`, `hotkey_defers`, `timer_should_wait`, `without_frame`, `custom_colors_need_save`, `config_ok_at_start`, `picked_image`, `remaining`.

Đích: `app_flow.rs` (luật điều phối, hẹn giờ) hoặc module mới `notices.rs` (nội dung và loại thông báo); nêu lựa chọn trong báo cáo.

**Ở lại lớp Windows (đã chốt ở Task 101):** `StartupMode`, `startup_mode`, `short_hash`, `MUTEX_NAME`, `tray_click_opens_menu`, `AppEvent`, `remember_last_capture`, mọi `WndProc`, trạng thái `thread_local!`.

## Quy tắc chọn cái để chuyển

Một mục (hàm, kiểu, hằng, `impl`) được chuyển khi **đủ cả bốn**:
1. Chữ ký và thân không dùng kiểu hay hàm của crate `windows` (`HWND`, `HDC`, `RECT`, `POINT`, `COLORREF`, `GetDpiForWindow`…) và không dùng `thread_local!`/`static` có trạng thái.
2. Không có tác dụng phụ ngoài `std` (tệp tạm cùng thư mục cho phép).
3. Là luật, bố cục, máy trạng thái hoặc phép tính mà một giao diện khác (macOS) cũng cần giống hệt, tính theo pixel logic và `dpi` (96 = 100%) hoặc theo `Rect`/`Point` của core.
4. Có test hoặc viết thêm test được.

Mục có phụ thuộc Win32 nhưng phần lõi thuần thì **tách đôi**: lõi sang core, lớp mỏng ở lại. Mục không chắc thì **không chuyển**, ghi vào báo cáo.

## Files

Tạo module core nêu ở bảng; sửa `crates/darkshot-core/src/lib.rs` (khai báo module, giữ thứ tự chữ cái). Lớp Windows chỉ sửa để dùng lại mục đã chuyển (`use darkshot_core::…`, cập nhật chỗ gọi; để lại `pub use` khi nhiều chỗ dùng tên cũ). Hàm nào chỉ ánh xạ sang `Msg`/văn bản qua `lang::tr` (lớp Windows giữ ngôn ngữ hiện hành) thì chuyển phần chọn `Msg`/tham số, để `lang::tr` ở lại.

## Interfaces

Giữ **nguyên chữ ký, tên, tên biến thể, thứ tự biến thể, giá trị hằng**; chỉ đổi mục riêng tư thành `pub` khi lớp Windows hoặc test cần.

## Kiểm thử (viết trước)

Mọi test hiện có của các mục chuyển đi theo nguyên văn (các test `step_error_titles`… trong `windows_main.rs`). Test dùng đường dẫn thì dựng bằng `Path::join`, không `\` cứng. TDD: nếu phép chuyển không tạo test đỏ thì gộp một commit `feat:` và ghi rõ trong báo cáo; test mới (nếu có) viết trước và thấy đỏ rồi mới viết mã.

## Cách kiểm chứng

Máy làm việc là macOS, `darkshot-windows` chỉ là stub nên **không biên dịch được ở đây**. Tự kiểm bằng `scripts/check.sh`; nó không chứng minh lớp Windows còn biên dịch. Vì vậy, sau mỗi lần chuyển: `grep -n` mọi chỗ gọi hàm đã chuyển, soát từng `use` (**import chỉ còn dùng trong test phải chuyển vào `mod tests`**, bài học Task 101: CI Windows chạy clippy `-D warnings` bắt `unused_imports` và `dead_code`), `pub`/riêng tư, đường dẫn module; rồi tự đọc lại `git diff` các tệp Windows và liệt kê từng tệp kèm lý do. Phiên điều phối đẩy nhánh để job `windows` của `.github/workflows/check.yml` chạy `check.ps1`; chỉ xong khi job đó xanh. Không tự đẩy.

## Global Constraints

Như mục cùng tên của `docs/superpowers/plans/2026-11-21-macos-port.md` (chép đủ ở đây):
- Core không phụ thuộc crate `windows`, `native-tls`, `schannel`, `rustls-platform-verifier`; không thêm phụ thuộc mới; không `cfg` theo hệ điều hành trong core; mọi chữ người dùng thấy qua `darkshot_core::i18n`.
- **Không đổi hành vi Windows.** Phép chuyển giữ nguyên chữ ký, tên, tên biến thể, thứ tự biến thể, giá trị hằng; test cũ đi theo nguyên văn (chỉ được đổi đường dẫn module; đổi giá trị đầu vào chỉ khi buộc phải bỏ `\` cứng, và phải nêu trong báo cáo).
- Không panic trên đường chạy thật (`panic = "abort"`). Ghi tệp qua tệp tạm rồi đổi tên.
- Repo công khai: không khóa, không cấu hình thật, không dữ liệu cá nhân; không đọc, ghi hay in khóa R2.
- QA không giả lập chuột/phím thật.
- Commit kết thúc bằng dòng `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`. Không sửa `memory-bank/` (phiên điều phối làm).

## Phụ thuộc

Task 101, 102a–102d (tránh xung đột `lib.rs`).

## Hoàn tất

`cargo fmt --all`; `./scripts/check.sh` → `ALL CHECKS PASSED`. Báo cáo ngắn: danh sách mục đã chuyển (sang module nào), tệp Windows đã sửa kèm lý do, mục "trông thuần" nhưng giữ lại và vì sao, kết quả check.sh, điều không chắc về việc lớp Windows còn biên dịch, số token.
