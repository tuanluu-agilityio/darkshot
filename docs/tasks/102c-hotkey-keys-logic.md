# Task 102c: chuyển logic phím tắt (`hotkey.rs`, bắt phím của `settings.rs`) sang core (đợt M, M2c, 3/5)

**Agent:** `builder` · **Model:** sonnet · **Nhánh:** `task/102c-hotkey-keys-logic` (từ `feat/macos-port`) · **Rà soát:** `reviewer` (opus) · **Cổng Windows:** job `windows` của `.github/workflows/check.yml` (quyết định 130)

Thiết kế: `docs/superpowers/specs/2026-11-21-macos-port-design.md` mục 3 (quyết định kiến trúc 1). Tiền lệ: `docs/tasks/101-core-shared-logic.md`. Quyết định 128, 131, 132.

## Mục tiêu

`Hotkey.vk` trong cấu hình là mã VK của Windows (quyết định 131). Bản Mac giữ nguyên định dạng đó và chỉ thêm bảng đổi mã phím macOS ↔ VK; tên phím, luật phím hợp lệ và cảnh báo phím bị chiếm phải dùng chung. **Phép chuyển, không đổi hành vi Windows.**

## Cần đọc

`crates/darkshot-windows/src/hotkey.rs`, `settings.rs` (mục `Capture`, `KeyEv`, `CaptureState`, `allowed_bare`, `is_modifier_vk`, `capture_hotkey` và test của chúng), `crates/darkshot-core/src/{config,i18n}.rs`.

## Phạm vi: chuyển những gì

| Từ | Sang core |
|---|---|
| `hotkey.rs`: `id_of`, `role_of`, `HelpKeys`/`help_keys`, `taken_combos`, `key_name`, `describe` | **module mới** `hotkey_keys.rs` |
| `settings.rs`: `Capture`, `KeyEv`, `CaptureState` (+ `impl`), `allowed_bare`, `is_modifier_vk`, `capture_hotkey` | `hotkey_keys.rs` |

**Ở lại lớp Windows:** `register`/`unregister` (gọi `RegisterHotKey`), mọi thứ chạm `HWND`.

Ghi chú thiết kế (không đổi hành vi): `describe` trả `"Ctrl+Shift+S"` theo mã VK; Mac có thể hiển thị khác (`⌘⇧S`) nhưng đó là việc của lớp Mac ở Task sau, không làm ở đây. Mã VK (`0x2C`, `0x70..=0x87`…) đã là số nên không cần hằng của crate `windows`; nếu có chỗ dùng `VK_*` thì thay bằng cùng giá trị số và ghi chú.

## Quy tắc chọn cái để chuyển

Một mục (hàm, kiểu, hằng, `impl`) được chuyển khi **đủ cả bốn**:
1. Chữ ký và thân không dùng kiểu hay hàm của crate `windows` (`HWND`, `HDC`, `RECT`, `POINT`, `COLORREF`, `GetDpiForWindow`…) và không dùng `thread_local!`/`static` có trạng thái.
2. Không có tác dụng phụ ngoài `std` (tệp tạm cùng thư mục cho phép).
3. Là luật, bố cục, máy trạng thái hoặc phép tính mà một giao diện khác (macOS) cũng cần giống hệt, tính theo pixel logic và `dpi` (96 = 100%) hoặc theo `Rect`/`Point` của core.
4. Có test hoặc viết thêm test được.

Mục có phụ thuộc Win32 nhưng phần lõi thuần thì **tách đôi**: lõi sang core, lớp mỏng ở lại. Mục không chắc thì **không chuyển**, ghi vào báo cáo.

## Files

Tạo module core nêu ở bảng; sửa `crates/darkshot-core/src/lib.rs` (khai báo module, giữ thứ tự chữ cái). Lớp Windows chỉ sửa để dùng lại mục đã chuyển (`use darkshot_core::…`, cập nhật chỗ gọi; để lại `pub use` khi nhiều chỗ dùng tên cũ). Giữ `Hotkey { ctrl, alt, shift, win, vk }` của `config.rs` như cũ.

## Interfaces

Giữ **nguyên chữ ký, tên, tên biến thể, thứ tự biến thể, giá trị hằng**; chỉ đổi mục riêng tư thành `pub` khi lớp Windows hoặc test cần.

## Kiểm thử (viết trước)

`hotkey.rs` có 4 test và `settings.rs` có test của `capture_hotkey`/`CaptureState`: đi theo nguyên văn. Thêm bảng test cho `key_name` (PrintScreen, F1..F24, chữ, số, mã lạ `0x..`) và `capture_hotkey` (phím bổ trợ một mình bị bỏ qua, Alt+F4 đi qua, Shift một mình cần thêm phím bổ trợ, PrintScreen đứng một mình hợp lệ) nếu chưa có. Test dùng đường dẫn thì dựng bằng `Path::join`, không `\` cứng. TDD: nếu phép chuyển không tạo test đỏ thì gộp một commit `feat:` và ghi rõ trong báo cáo; test mới (nếu có) viết trước và thấy đỏ rồi mới viết mã.

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

Task 102b.

## Hoàn tất

`cargo fmt --all`; `./scripts/check.sh` → `ALL CHECKS PASSED`. Báo cáo ngắn: danh sách mục đã chuyển (sang module nào), tệp Windows đã sửa kèm lý do, mục "trông thuần" nhưng giữ lại và vì sao, kết quả check.sh, điều không chắc về việc lớp Windows còn biên dịch, số token.
