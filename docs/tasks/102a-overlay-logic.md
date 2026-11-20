# Task 102a: chuyển logic thuần của `overlay.rs` sang core (đợt M, M2c, 1/5)

**Agent:** `builder` · **Model:** sonnet · **Nhánh:** `task/102a-overlay-logic` (từ `feat/macos-port`) · **Rà soát:** `reviewer` (opus) · **Cổng Windows:** job `windows` của `.github/workflows/check.yml` (quyết định 130)

Thiết kế: `docs/superpowers/specs/2026-11-21-macos-port-design.md` mục 3 (quyết định kiến trúc 1). Tiền lệ: `docs/tasks/101-core-shared-logic.md`. Quyết định 128, 131, 132.

## Mục tiêu

Bản Mac dùng cùng bố cục thanh công cụ, bảng chọn màu và luật nhập số của lớp phủ chọn vùng (M3c, M5). `overlay.rs` có 97 test; phần lớn là bố cục theo `dpi`. **Phép chuyển, không đổi hành vi Windows.**

## Cần đọc

`crates/darkshot-windows/src/overlay.rs` (cả các khối `#[cfg(test)]`), `crates/darkshot-core/src/{geometry,annotate,color,edit,i18n}.rs`, `lib.rs`.

## Phạm vi: chuyển những gì

| Từ `overlay.rs` | Sang core |
|---|---|
| `BarItem` (+ `impl`), `BAR_CELLS`, `bar_cells`, `BarLayout`, `bar_layout(_with)`, `bar_item_rect(_with)`, `bar_item_at(_with)`, `bars_for` | **module mới** `overlay_layout.rs` |
| `SideItem` (+ `impl`), `SIDE_ITEMS`, `SIDE_SEPS`, `SideLayout`, `side_index`, `side_layout_for`, `side_item_rect`, `side_item_at`, `is_shape`, `SHAPE_TOOLS` | `overlay_layout.rs` |
| Hằng bố cục: `BUTTON`, `GAP`, `HANDLE_HIT`, `ICON`, `SIDE_SEP`, `LUM_ARROW`, `CROSS_*`, `SWATCH`, `MARK`, `TEXT_FRAME_PAD`, `TEXT_EMPTY_W`, `TEXT_DASH*` | `overlay_layout.rs` (`pub const`) |
| `size_for_key`, `size_dot`, `PickColor` (+ `impl`), `FIELD_MAX`, `FIELD_COUNT`, `FIELD_DIGITS`, `field_value`, `with_field`, `FieldEdit` (+ `impl`), bố cục và hit-test của bảng chọn màu (`PickerLayout`, `picker_hit`, `lum_hit_rect`, `at`, …), `spectrum_image`, `lum_image` | **module mới** `color_picker.rs` |
| Còn lại trong `overlay.rs` thỏa quy tắc dưới đây (ví dụ hàm tính khung chữ, tay nắm, con trỏ theo vị trí) | module hợp lý trong core (đặt tên nêu trong báo cáo) |

**Ở lại lớp Windows:** `ESC_HOTKEY_ID`, `FOCUS_*`, `CARET_TIMER_ID`, `MAX_FOCUS_RETRIES` (ID/hẹn giờ Win32); `ICON_ARGB`, `ICON_HOVER_ARGB`, `ICON_DISABLED_ARGB`, `argb`, `cr` (đổi màu sang định dạng GDI); `shape_tip`/`size_tip` nếu chỉ ánh xạ sang `Msg` (cân nhắc: chuyển nếu thuần); `Glyph` và mọi hàm vẽ, `WndProc`, trạng thái lớp phủ (`thread_local!`), mọi thứ gọi Win32.

## Quy tắc chọn cái để chuyển

Một mục (hàm, kiểu, hằng, `impl`) được chuyển khi **đủ cả bốn**:
1. Chữ ký và thân không dùng kiểu hay hàm của crate `windows` (`HWND`, `HDC`, `RECT`, `POINT`, `COLORREF`, `GetDpiForWindow`…) và không dùng `thread_local!`/`static` có trạng thái.
2. Không có tác dụng phụ ngoài `std` (tệp tạm cùng thư mục cho phép).
3. Là luật, bố cục, máy trạng thái hoặc phép tính mà một giao diện khác (macOS) cũng cần giống hệt, tính theo pixel logic và `dpi` (96 = 100%) hoặc theo `Rect`/`Point` của core.
4. Có test hoặc viết thêm test được.

Mục có phụ thuộc Win32 nhưng phần lõi thuần thì **tách đôi**: lõi sang core, lớp mỏng ở lại. Mục không chắc thì **không chuyển**, ghi vào báo cáo.

## Files

Tạo module core nêu ở bảng; sửa `crates/darkshot-core/src/lib.rs` (khai báo module, giữ thứ tự chữ cái). Lớp Windows chỉ sửa để dùng lại mục đã chuyển (`use darkshot_core::…`, cập nhật chỗ gọi; để lại `pub use` khi nhiều chỗ dùng tên cũ). Tên module `overlay_layout.rs`, `color_picker.rs` do gói này chọn; giữ nguyên tên mục. Hằng và hàm từng riêng tư thành `pub` khi cần để lớp Windows gọi.

## Interfaces

Giữ **nguyên chữ ký, tên, tên biến thể, thứ tự biến thể, giá trị hằng**; chỉ đổi mục riêng tư thành `pub` khi lớp Windows hoặc test cần.

## Kiểm thử (viết trước)

Mọi test hiện có của các mục chuyển đi theo nguyên văn (`overlay.rs` có 97 test; nhiều test kiểm bố cục theo `dpi` 96/144/192, hit-test, `FieldEdit`, `PickColor`). Test nào gọi hàm vẫn ở lại Windows thì ở lại. Thêm: `bar_layout` ở `dpi` 96, 120, 144, 192 không tràn ngoài `Rect` màn hình ở bốn góc (đã có tiền lệ thì chỉ cần giữ). Test dùng đường dẫn thì dựng bằng `Path::join`, không `\` cứng. TDD: nếu phép chuyển không tạo test đỏ thì gộp một commit `feat:` và ghi rõ trong báo cáo; test mới (nếu có) viết trước và thấy đỏ rồi mới viết mã.

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

Task –.

## Hoàn tất

`cargo fmt --all`; `./scripts/check.sh` → `ALL CHECKS PASSED`. Báo cáo ngắn: danh sách mục đã chuyển (sang module nào), tệp Windows đã sửa kèm lý do, mục "trông thuần" nhưng giữ lại và vì sao, kết quả check.sh, điều không chắc về việc lớp Windows còn biên dịch, số token.
