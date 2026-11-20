# Task 102d: chuyển logic menu khay và trạng thái ứng dụng (`tray.rs`, `app.rs`) sang core (đợt M, M2c, 4/5)

**Agent:** `builder` · **Model:** sonnet · **Nhánh:** `task/102d-tray-app-logic` (từ `feat/macos-port`) · **Rà soát:** `reviewer` (opus) · **Cổng Windows:** job `windows` của `.github/workflows/check.yml` (quyết định 130)

Thiết kế: `docs/superpowers/specs/2026-11-21-macos-port-design.md` mục 3 (quyết định kiến trúc 1). Tiền lệ: `docs/tasks/101-core-shared-logic.md`. Quyết định 128, 131, 132.

## Mục tiêu

Menu thanh trạng thái trên Mac (M3b) dùng cùng bố cục mục menu và hàng đợi thông báo; luật khởi động (có cần mở Tùy chọn không, thông báo cấu hình) và sổ tải lên chưa xong dùng chung. **Phép chuyển, không đổi hành vi Windows.**

## Cần đọc

`crates/darkshot-windows/src/tray.rs`, `app.rs` (cả khối test), `crates/darkshot-core/src/{actions,config,upload,image,i18n}.rs`.

## Phạm vi: chuyển những gì

| Từ | Sang core |
|---|---|
| `tray.rs`: `MenuEntry`, `menu_layout`, `Notice`, `MAX_QUEUED_NOTICES`, `enqueue_notice`, `take_notice_to_show`, `MAX_ADD_RETRIES`, `should_retry`; `MenuCommand` (chỉ kiểu enum, không kèm ID Win32) | **module mới** `tray_logic.rs` |
| `app.rs`: `ConfigNotice` (+ `impl`), `should_sync_autostart`, `needs_settings_on_start`, `PendingUpload`, `PendingUploads` (+ `impl`), `save_pending`, nếu thỏa quy tắc | **module mới** `app_state.rs` |

**Ở lại lớp Windows:** `Tray` (cửa sổ khay, `Shell_NotifyIconW`), `TrayNotifier` nếu chạm Win32, `fill_wide`, `WM_TRAY`/`WM_NOTICE`, `ID_*`, `SilentNotifier`/`NoClipboard` nếu là chỗ chứa trait Win32 (nếu thuần thì có thể chuyển, nêu rõ), `AppState`/`startup` nếu gọi `paths`/Win32, `UploadJob`, `spawn_upload`, `OutcomeHook`.

Ghi chú: `PendingUploads` dùng `Mutex`; được chuyển khi không có `thread_local!`; giữ nguyên thứ tự `drain`/hàng ghi (quyết định liên quan thoát, Task 88).

## Quy tắc chọn cái để chuyển

Một mục (hàm, kiểu, hằng, `impl`) được chuyển khi **đủ cả bốn**:
1. Chữ ký và thân không dùng kiểu hay hàm của crate `windows` (`HWND`, `HDC`, `RECT`, `POINT`, `COLORREF`, `GetDpiForWindow`…) và không dùng `thread_local!`/`static` có trạng thái.
2. Không có tác dụng phụ ngoài `std` (tệp tạm cùng thư mục cho phép).
3. Là luật, bố cục, máy trạng thái hoặc phép tính mà một giao diện khác (macOS) cũng cần giống hệt, tính theo pixel logic và `dpi` (96 = 100%) hoặc theo `Rect`/`Point` của core.
4. Có test hoặc viết thêm test được.

Mục có phụ thuộc Win32 nhưng phần lõi thuần thì **tách đôi**: lõi sang core, lớp mỏng ở lại. Mục không chắc thì **không chuyển**, ghi vào báo cáo.

## Files

Tạo module core nêu ở bảng; sửa `crates/darkshot-core/src/lib.rs` (khai báo module, giữ thứ tự chữ cái). Lớp Windows chỉ sửa để dùng lại mục đã chuyển (`use darkshot_core::…`, cập nhật chỗ gọi; để lại `pub use` khi nhiều chỗ dùng tên cũ). `save_pending` ghi tệp: giữ nguyên thứ tự và ghi nguyên tử như cũ; không đổi tên tệp.

## Interfaces

Giữ **nguyên chữ ký, tên, tên biến thể, thứ tự biến thể, giá trị hằng**; chỉ đổi mục riêng tư thành `pub` khi lớp Windows hoặc test cần.

## Kiểm thử (viết trước)

`tray.rs` có 10 test, `app.rs` có 22: các test của mục chuyển đi theo nguyên văn. Test dùng đường dẫn thì dựng bằng `Path::join`, không `\` cứng. TDD: nếu phép chuyển không tạo test đỏ thì gộp một commit `feat:` và ghi rõ trong báo cáo; test mới (nếu có) viết trước và thấy đỏ rồi mới viết mã.

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

Task 101.

## Hoàn tất

`cargo fmt --all`; `./scripts/check.sh` → `ALL CHECKS PASSED`. Báo cáo ngắn: danh sách mục đã chuyển (sang module nào), tệp Windows đã sửa kèm lý do, mục "trông thuần" nhưng giữ lại và vì sao, kết quả check.sh, điều không chắc về việc lớp Windows còn biên dịch, số token.
