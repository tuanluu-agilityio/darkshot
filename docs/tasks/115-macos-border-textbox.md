# Task 115: Viền nằm ngoài vùng nền; Chữ là khung có kích thước tự do (phản hồi QA, 2026-11-21)

**Agent:** `builder` · **Model:** sonnet · **Nhánh:** `task/115-macos-border-textbox` (từ `feat/macos-port`) · **Rà soát:** hoãn · **Cổng:** `scripts/check.sh`, `scripts/check-windows-cross.sh`, CI

Nền: 112–114 (`Shape::Box`, `Shape::Label`, `Style`, `annotate_style.rs`, `annotate_ui.rs`, `annotate_draw.rs`, core `annotate.rs`/`edit.rs`). `Box` và `Label` chỉ bản Mac tạo, nên được đổi cấu trúc; không đổi hành vi hay biến thể cũ của Windows (Windows chỉ giữ nhánh biên dịch).

## 1. Viền nằm NGOÀI vùng nền

Hiện viền vẽ căn giữa đường biên nên nửa trong đè lên nền: tăng độ dày viền làm vùng nền bị thu lại/đổi màu. Yêu cầu: vùng nền (và khung người dùng kéo) là hình chữ nhật/elip `a..b` **cố định**; viền là vành bao **bên ngoài** nó, dày `stroke_width`. Tăng/giảm độ dày viền không đổi vùng nền, không đổi chữ trong khung. Áp cho Hình chữ nhật, Elip và Chữ, kể cả bo góc (bán kính viền ngoài = bán kính trong + độ dày; giữ nghĩa "bo góc px" của nền). Với độ trong suốt < 100%, vùng nền và vùng viền không chồng nhau (không có chỗ trộn hai lần). Hộp bao, chọn trúng (bấm cạnh viền), tay nắm đổi cỡ, `burn` và cắt theo vùng chọn đều tính viền ngoài. Tay nắm đổi cỡ thay đổi `a..b` (vùng nền), không phải đường ngoài của viền.

## 2. Chữ là khung có kích thước tự do, chữ luôn ở giữa theo chiều dọc

- `Label` có **khung** (hai góc `a`, `b`) do người dùng định: tạo bằng kéo chuột (nhấp đơn tạo khung kích thước mặc định hợp lý theo cỡ chữ). Kéo tay nắm đổi **rộng và cao tùy ý** (ngang, dọc, góc); **cỡ chữ KHÔNG đổi theo khung** (hiện tay nắm đổi cỡ đang đổi cỡ chữ: bỏ). Cỡ chữ chỉ đổi bằng bảng Cỡ/phím `[` `]` như hiện nay.
- Căn **ngang** trái/giữa/phải trong khung (có đệm trong), có tác dụng thật vì khung rộng hơn chữ.
- Căn **dọc luôn ở giữa** khung: khối chữ (tất cả dòng) căn giữa theo chiều dọc như PowerPoint; nhiều dòng thì cả khối ở giữa, mỗi dòng căn ngang theo lựa chọn.
- Chữ dài ngắt dòng theo chiều rộng khung (ngắt theo từ; từ quá dài thì ngắt ký tự). Nếu khối chữ cao hơn khung thì chữ tràn đối xứng trên/dưới và bị cắt theo khung khi vẽ; KHÔNG tự đổi kích thước khung (như PowerPoint khi tắt autofit). Giữ nhấp đúp để sửa, nền, viền ngoài, bo góc, độ trong suốt từ 112–114.
- Khi đang gõ, khung hiện rõ (viền mảnh nét đứt hoặc tay nắm) để biết kích thước; caret nằm trong khối đã căn giữa.
- Z-order, hoàn tác (đổi khung = một bước), chọn/di chuyển, `burn`, IME giữ nguyên.

## Kiểm thử (TDD, test thuần trước)

Core: hộp bao với viền ngoài (vùng nền không đổi khi đổi độ dày); `Label` khung chuẩn hóa (kéo ngược chiều); bố cục chữ: ngắt dòng theo chiều rộng, y bắt đầu khối = giữa khung − nửa chiều cao khối với 1, 2, 5 dòng; x từng dòng theo trái/giữa/phải; đổi khung không đổi cỡ chữ; tràn đối xứng. Pixel macOS: tăng độ dày viền không đổi pixel vùng nền (so sánh trong lõi); viền không chồng nền khi opacity 50%; góc bo ngoài; chữ một dòng và nhiều dòng ở giữa dọc; khung rộng + căn phải đặt chữ sát phải; cắt theo khung. Mở rộng `annotate_sample`. Các test của 112–114 đổi theo hành vi mới thì sửa kèm lý do trong báo cáo.

## Global Constraints, Báo cáo

Như Task 112 (không `unwrap` trên đường chạy thật, i18n, không thêm phụ thuộc, không sửa `memory-bank/`, không đẩy, commit có `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`; Esc/Hủy/Cmd+Tab không hỏng). Cổng: `cargo fmt --all`, `./scripts/check.sh` ALL CHECKS PASSED, `./scripts/check-windows-cross.sh`. Báo cáo tiếng Việt: từng mục làm thế nào, tệp, hạn chế, token.
