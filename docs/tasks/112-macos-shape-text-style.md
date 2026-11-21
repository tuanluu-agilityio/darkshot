# Task 112: Kiểu cho Hình và Chữ trên bản macOS (nền, viền, độ trong suốt, căn lề)

**Agent:** `builder` · **Model:** sonnet · **Nhánh:** `task/112-macos-shape-text-style` (từ `feat/macos-port`) · **Rà soát:** hoãn (cùng đợt opus cuối với 107–111) · **Cổng:** `scripts/check.sh`, `scripts/check-windows-cross.sh`, CI (windows + macOS)

Yêu cầu của chủ dự án (2026-11-21). Làm trên nền Task 110–111 (`annotate_draw.rs`, `annotate_ui.rs`, `annotate.rs` của core).

## Mục tiêu

1. **Hình (Khung, Elip):** chỉnh được **màu nền** (hoặc không nền), **màu viền**, **độ dày viền**, **độ trong suốt** (nền và viền; có thể một thanh độ trong suốt chung hoặc riêng nền/viền, chọn phương án đơn giản, ghi vào báo cáo).
2. **Chữ:** như Hình, thêm **căn trái / giữa / phải**; **nền** có hoặc không (mặc định không, như hiện tại); **màu viền** và **độ dày viền** (viền khung quanh khối chữ); độ trong suốt.
3. Áp dụng cho hình **đang chọn** (đổi là cập nhật hình đó, một bước hoàn tác) và làm **mặc định cho hình vẽ tiếp theo** (giống cách màu/cỡ hiện giờ).
4. Z-order giữ nguyên quy tắc (hình dùng sau nằm trước; sửa hình giữ lớp).

## Ràng buộc kiến trúc (quan trọng)

- `Shape` ở core dùng chung với Windows (89 chỗ). **Không đổi chữ ký hay nghĩa các biến thể hiện có** và không đổi hành vi Windows. Hướng đề xuất: thêm kiểu `Style` (fill: Option<Rgb>, fill_opacity, stroke: Rgb, stroke_width, stroke_opacity, và cho chữ `align`) **thuần, có test, ở core** và biến thể mới cộng thêm (ví dụ `Shape::Box`, `Shape::Oval`, `Shape::Label` mang `Style`) — Windows chỉ được thêm nhánh `match` tối thiểu để biên dịch (không bao giờ tạo ra chúng), hoặc nếu tìm được cách không chạm Windows thì tốt hơn. Builder được chọn thiết kế khác nếu giữ đúng ràng buộc này; ghi lựa chọn vào báo cáo.
- Hộp bao, chọn, di chuyển, đổi cỡ, hoàn tác/làm lại, `burn` (đốt vào ảnh khi Lưu/Chép/Tải lên) phải hỗ trợ kiểu mới. Chữ nhiều dòng + căn lề: đo chiều rộng từng dòng bằng CoreText; khối chữ có hộp rộng theo dòng dài nhất (hoặc hộp người dùng kéo nếu đã có).
- Chuỗi giao diện mới chỉ qua `darkshot_core::i18n` (cả hai ngôn ngữ). Không chuỗi tiếng Việt cứng.
- Giao diện: một bảng thuộc tính (flyout như bảng Cỡ/Màu hiện có) hiện khi công cụ Hình/Chữ đang chọn hoặc đang chọn một hình loại đó. Dùng lại bảng màu 48 màu cho ô màu; thanh trượt hoặc bước cho độ dày và độ trong suốt; nút "Không nền".
- Không thêm phụ thuộc. Đường thoát (Esc, Hủy, Cmd+Tab) không được hỏng khi bảng mới mở.

## Kiểm thử (viết trước, TDD)

Test thuần (core hoặc `annotate_ui.rs`): kiểu mặc định; áp kiểu cho hình đang chọn tạo đúng một bước hoàn tác; hoàn tác/làm lại khôi phục kiểu; hộp bao tính đúng với viền dày và với chữ căn trái/giữa/phải; nhiều dòng căn lề cho x bắt đầu từng dòng đúng; độ trong suốt kẹp trong 0..=1; z-order không đổi khi đổi kiểu. Test pixel (như `annotate_sample`): nền có opacity 50% trộn đúng với ảnh nền; viền đúng màu/độ dày; chữ căn giữa nằm giữa hộp. Mở rộng ảnh mẫu để xem bằng mắt.

## Global Constraints

Như Task 110–111: core không phụ thuộc crate `windows|native-tls|schannel|rustls-platform-verifier`; không đọc/ghi/in khóa R2; repo công khai; không `unwrap` trên đường chạy thật; QA không giả lập chuột/phím thật. Cổng: `cargo fmt --all`, `./scripts/check.sh` → `ALL CHECKS PASSED`, `./scripts/check-windows-cross.sh` đạt. Commit kết thúc bằng `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`. Không sửa `memory-bank/` (phiên điều phối làm). Không tự đẩy.

## Báo cáo

Phương án thiết kế đã chọn, danh sách tệp (kèm tệp Windows nếu có), cách dùng giao diện, hạn chế còn lại, số token.
