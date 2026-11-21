# Task 116: Hàng "Cỡ chữ" trong bảng thuộc tính của Chữ (phản hồi QA, 2026-11-22)

**Agent:** `builder` · **Model:** sonnet · **Nhánh:** `task/116-macos-text-font-size` (từ `feat/macos-port`) · **Rà soát:** hoãn · **Cổng:** `scripts/check.sh`, `scripts/check-windows-cross.sh`, CI

Nền: 112–115 (`annotate_style.rs`, `annotate_ui.rs`, `overlay_view.rs`, core `annotate.rs`, `i18n.rs`). Hiện cỡ chữ chỉ đổi qua bảng Cỡ (mức rời rạc) và `[` `]`; bảng thuộc tính của Chữ không có điều khiển cỡ chữ.

## Yêu cầu

1. Bảng của Chữ có hàng **Cỡ chữ**: hiện số cỡ hiện tại (px ảnh), nút −/+ (bước 2), kẹp trong khoảng hợp lý (ví dụ 8..=200, hằng trong core). Có thể thêm ô gõ số nếu đơn giản; không bắt buộc.
2. Áp cho: Chữ đang chọn (một bước hoàn tác, đổi cỡ không đổi khung `a..b`), ô chữ đang gõ, và làm mặc định cho chữ tạo sau (nhớ qua `Prefs`).
3. Đồng bộ hai chiều với bảng Cỡ và phím `[` `]`: đổi mức ở đó thì số trong hàng mới đổi theo; đổi số trong hàng mới (không khớp mức nào) thì bảng Cỡ không có mức nào sáng, không lỗi.
4. Cỡ chữ vẫn độc lập với khung (Task 115). Chuỗi mới qua `darkshot_core::i18n` (Vi/En).

## Kiểm thử

Test thuần: bước/kẹp, áp cho Chữ đang chọn là một bước hoàn tác và giữ khung, áp cho ô đang gõ, mặc định nhớ qua Prefs, đồng bộ với mức Cỡ, ô bấm trúng đúng ở 96/144/192 DPI và không chồng nhau, bảng vẫn nằm trong màn hình và né vùng chọn (quy tắc Task 114). Pixel/bố cục: đổi cỡ làm chữ to nhỏ đúng, vẫn căn giữa dọc trong khung.

## Global Constraints, Báo cáo

Như Task 112 (không `unwrap` trên đường chạy thật, không thêm phụ thuộc, không sửa `memory-bank/`, không đẩy, commit có `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`). Cổng: `cargo fmt --all`, `./scripts/check.sh` ALL CHECKS PASSED, `./scripts/check-windows-cross.sh`. Báo cáo tiếng Việt, kèm token nếu đọc được.
