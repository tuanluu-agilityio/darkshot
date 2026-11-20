# Task 102b: chuyển logic biểu mẫu Tùy chọn của `settings.rs` sang core (đợt M, M2c, 2/5)

**Agent:** `builder` · **Model:** sonnet · **Nhánh:** `task/102b-settings-form-logic` (từ `feat/macos-port`) · **Rà soát:** `reviewer` (opus) · **Cổng Windows:** job `windows` của `.github/workflows/check.yml` (quyết định 130)

Thiết kế: `docs/superpowers/specs/2026-11-21-macos-port-design.md` mục 3 (quyết định kiến trúc 1). Tiền lệ: `docs/tasks/101-core-shared-logic.md`. Quyết định 128, 131, 132.

## Mục tiêu

Cửa sổ Tùy chọn trên Mac (M4b) kiểm tra và áp biểu mẫu bằng cùng một luật; nếu không dùng chung thì hai bản sẽ lệch. **Phép chuyển, không đổi hành vi Windows.**

## Cần đọc

`crates/darkshot-windows/src/settings.rs` (phần luật ở đầu tệp, mục "Cửa sổ" trở đi để biết nơi gọi; cả khối test), `crates/darkshot-core/src/{config,app_flow,i18n}.rs`, `lib.rs`.

## Phạm vi: chuyển những gì

| Từ `settings.rs` | Sang core |
|---|---|
| `Form` (kèm `impl Debug` che khóa bí mật), `SettingsResult`, `FormError`, `Field`, `Tab` (+ `impl`), `tab_of`, `initial_tab`, `quality_enabled`, `tabs_fit`, `apply_form`, `is_account_char`, `is_bucket_char` | **module mới** `settings_form.rs` |
| `KeyRoute`, `key_route` | `settings_form.rs` |

Ghi chú: `OpenReason` đã ở `app_flow` (Task 101). `apply_form` đã dùng `i18n`; giữ nguyên thông điệp lỗi từng chữ. Kiểm `is_account_char`/`is_bucket_char` có trùng với kiểm tra ký tự ở `config.rs` không: **nếu trùng hãy dùng lại một bản và nêu trong báo cáo, nhưng không đổi hành vi**.

**Ở lại lớp Windows (và KHÔNG chuyển trong gói này):** mọi `ID_*`, `TBM_*`, `WM_APP_TEST_DONE`, `TEST_*`, `ws`, `build_*`, `frame_proc`, vòng thông điệp, `show`. **Các mục bắt phím** (`Capture`, `KeyEv`, `CaptureState`, `allowed_bare`, `is_modifier_vk`, `capture_hotkey`) thuộc Task 102c, không động tới ở đây.

## Quy tắc chọn cái để chuyển

Một mục (hàm, kiểu, hằng, `impl`) được chuyển khi **đủ cả bốn**:
1. Chữ ký và thân không dùng kiểu hay hàm của crate `windows` (`HWND`, `HDC`, `RECT`, `POINT`, `COLORREF`, `GetDpiForWindow`…) và không dùng `thread_local!`/`static` có trạng thái.
2. Không có tác dụng phụ ngoài `std` (tệp tạm cùng thư mục cho phép).
3. Là luật, bố cục, máy trạng thái hoặc phép tính mà một giao diện khác (macOS) cũng cần giống hệt, tính theo pixel logic và `dpi` (96 = 100%) hoặc theo `Rect`/`Point` của core.
4. Có test hoặc viết thêm test được.

Mục có phụ thuộc Win32 nhưng phần lõi thuần thì **tách đôi**: lõi sang core, lớp mỏng ở lại. Mục không chắc thì **không chuyển**, ghi vào báo cáo.

## Files

Tạo module core nêu ở bảng; sửa `crates/darkshot-core/src/lib.rs` (khai báo module, giữ thứ tự chữ cái). Lớp Windows chỉ sửa để dùng lại mục đã chuyển (`use darkshot_core::…`, cập nhật chỗ gọi; để lại `pub use` khi nhiều chỗ dùng tên cũ). `Form` không được mang kiểu Win32; nếu đang mang thì tách.

## Interfaces

Giữ **nguyên chữ ký, tên, tên biến thể, thứ tự biến thể, giá trị hằng**; chỉ đổi mục riêng tư thành `pub` khi lớp Windows hoặc test cần.

## Kiểm thử (viết trước)

Mọi test hiện có của các mục chuyển (46 test trong `settings.rs`, gồm `error_order_is_savedir_hotkeys_r2`, `errors_and_debug_never_contain_secret`, `first_error_wins_in_tab_order`, `form_error_message_follows_language`, `hotkey_conflict_is_a_form_error`…) đi theo nguyên văn. Test `Debug` không bao giờ in khóa bí mật phải xanh ở core. Test dùng đường dẫn thì dựng bằng `Path::join`, không `\` cứng. TDD: nếu phép chuyển không tạo test đỏ thì gộp một commit `feat:` và ghi rõ trong báo cáo; test mới (nếu có) viết trước và thấy đỏ rồi mới viết mã.

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
