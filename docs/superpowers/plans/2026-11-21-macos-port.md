# Đợt M: bản macOS — kế hoạch

Thiết kế: `docs/superpowers/specs/2026-11-21-macos-port-design.md`. Nhánh tích hợp `feat/macos-port`; mỗi task một nhánh `task/1NN-…` tách từ và gộp lại vào nhánh tích hợp (quyết định 127). Global Constraints: xem mục cùng tên bên dưới. Model chọn theo độ rủi ro: bảo mật hoặc toàn vẹn dữ liệu không dùng `haiku`; rà soát bởi agent khác, model mạnh bằng hoặc hơn.

| Bước | Task | Nội dung | Agent / model | Rủi ro |
|---|---|---|---|---|
| M0 | (phiên chính) | Cài Rust, test core trên Mac | xong | thấp |
| M1 | (phiên chính) | Thiết kế, kế hoạch, quyết định 127–129 | xong | thấp |
| M2a | 100 | Workspace dùng `cfg` theo hệ điều hành; crate `darkshot-macos` khung; `scripts/check.sh`; `check.yml` | phiên chính (làm trực tiếp) | trung bình |
| M2b | 101 | Chuyển logic thuần từ `main.rs`, `capture.rs`, `paths.rs` sang core (giữ test) | builder / sonnet; rà soát specialist | trung bình (chạm Windows) |
| M2c-1 | 102a | `overlay.rs`: bố cục thanh công cụ, bảng chọn màu, nhập số (module mới `overlay_layout`, `color_picker`) | builder / sonnet; rà soát reviewer / opus | trung bình (97 test, chạm Windows) |
| M2c-2 | 102b | `settings.rs`: biểu mẫu Tùy chọn (`settings_form`) | builder / sonnet | trung bình |
| M2c-3 | 102c | `hotkey.rs` và bắt phím của `settings.rs`: tên phím, luật phím hợp lệ (`hotkey_keys`) | builder / sonnet | thấp |
| M2c-4 | 102d | `tray.rs` và `app.rs`: bố cục menu, hàng đợi thông báo, sổ tải lên (`tray_logic`, `app_state`) | builder / sonnet | trung bình |
| M2c-5 | 102e | Hàm thuần còn lại của `windows_main.rs` (`notices`/`app_flow`) | builder / sonnet | thấp |
| (hoãn) | – | `history_layout`, `history_service`, `snap`, `pin`, `ocr*`, `steps_*`, `annotate_draw`, `theme`: chuyển đúng lúc làm M5–M7 (quyết định 133) | – | – |
| M3a | 103 | `darkshot-macos` nền móng: `cfg` phụ thuộc, `paths.rs`, `coords.rs` (thuần, test), `timeutil.rs`, `secret.rs` (Keychain, quyết định 136), `clipboard.rs` (`NSPasteboard`) | builder / opus; rà soát reviewer / fable | cao (khóa bí mật, FFI) |
| M3b | 104 | Vỏ ứng dụng: `.app` + ký ổn định (`scripts/bundle-macos.sh`), `NSStatusItem` + menu từ `tray_logic`, thông báo, chụp từng màn hình bằng ScreenCaptureKit, kiểm quyền, nối cấu hình, `scripts/demo-macos.sh` | builder / opus; rà soát reviewer / fable | cao (quyền hệ thống, luồng chính) |
| M3c | 105 | Lớp phủ chọn vùng (mỗi màn hình một cửa sổ), `Selection` của core, thanh Lưu/Chép/Hủy, `run_action` | builder / opus; rà soát reviewer / fable | cao (vẽ, sự kiện, Retina) |
| M4a | 106 | Phím tắt toàn cục (Carbon `RegisterEventHotKey`), bảng đổi phím macOS ↔ VK, bộ phím mặc định riêng cho Mac (quyết định 139) | builder / opus; rà soát reviewer / opus | trung bình |
| M4b | 107 | Cửa sổ Tùy chọn AppKit (4 tab, giao diện tối), dùng `settings_form` và bắt phím của core, ô bắt phím, Kiểm tra kết nối R2, khóa vào Keychain | builder / opus; rà soát reviewer / opus | cao (khóa bí mật, bố cục lớn) |
| M4c | 108 | Nút Tải lên R2, link có hạn, sổ lượt tải lên và an toàn khi thoát (`PendingUploads`, `save_pending`) | builder / opus; rà soát reviewer / opus | cao (không được mất ảnh) |
| M4d | 109 | Tài liệu và hoàn thiện: README (phần macOS), `.icns`, `scripts/demo-macos.sh` hoàn chỉnh, memory bank; tối ưu tốc độ và dung lượng ảnh nếu chủ dự án yêu cầu; KHÔNG sửa website cho đến khi phát hành (quy ước website) | scaffolder / sonnet | thấp |
| M5–M7 | 109+ | Đánh dấu, Ghim, Lịch sử, bắt cửa sổ, OCR, Chụp từng bước, tự khởi động, kiểm tra bản mới: tách từng task sau khi M-A được duyệt | — | — |

Cổng mỗi task: `scripts/check.sh` (Mac) phải in `ALL CHECKS PASSED`; task chạm lớp Windows (100, 101, 102a–102e) phải qua job `windows` của `.github/workflows/check.yml` (chạy `scripts/check.ps1` trên runner `windows-latest`) trước khi gộp vào `main`, vì chủ dự án không có máy Windows (2026-11-21). Không gộp `main` khi job đó chưa xanh; cần đẩy nhánh lên `origin` để chạy.

## Global Constraints

Như mục cùng tên của `docs/superpowers/plans/2026-11-18-step-capture.md` (kế thừa chuỗi các đợt trước), cộng:
- `darkshot-core` không phụ thuộc crate `windows`, `native-tls`, `schannel`, `rustls-platform-verifier`, và không thêm crate theo hệ điều hành. Không TLS của hệ điều hành.
- Logic chung nằm ở core có test; lớp nền tảng chỉ vẽ, nhận sự kiện và gọi core. Không sao chép logic giữa hai lớp nền tảng (quyết định 128).
- Task chuyển mã **không đổi hành vi Windows**; test hiện có đi theo nguyên văn.
- Khóa R2 chỉ qua `SecretStore` (Keychain trên Mac); không vào log, tệp cấu hình hay lời gọi ngoài. Không đưa chứng chỉ, Team ID hay profile ký vào repo công khai.
- Mọi API Objective-C bọc trong module nhỏ, mỗi khối `unsafe` có `// SAFETY:`. Ghim phiên bản chính xác các crate `objc2*`.
- Lớp Windows không biên dịch được trên Mac; cổng của nó là job `windows` trong `.github/workflows/check.yml` (quyết định 130). Agent không tự đẩy nhánh; phiên điều phối đẩy.
- Bản dựng Mac chỉ nhắm `aarch64-apple-darwin`, macOS 13 trở lên; không phát hành, không notarize (quyết định 129).
- QA không giả lập chuột/phím thật (quyết định 111, 114, 126).

## Review Focus

1. Không đổi hành vi Windows: mỗi hàm chuyển giữ nguyên chữ ký, biến thể, thứ tự, và test đi theo nguyên văn.
2. Lớp Windows còn biên dịch: mọi `use`, `pub`, đường dẫn module, chỗ gọi đã cập nhật; không còn bản trùng. Kiểm bằng đọc diff và bằng job `windows` trên CI.
3. Core vẫn sạch: không crate cấm, không `cfg` theo hệ điều hành, không API thời gian hệ điều hành, không `\` cứng trong test.
4. Phạm vi: không chuyển hàm ngoài bảng của gói task; hàm gắn Win32 giữ nguyên chỗ.
5. Ghi tệp nguyên tử, không panic, không chuỗi tiếng Việt viết cứng.

## Ghi chú M3 (2026-11-21)

- Chủ dự án muốn hoàn thiện trên macOS trước, **chưa gộp `main`** (nhánh `feat/macos-port` ở lại). Mọi gói M3 tách `task/10N-…` từ `feat/macos-port` và gộp lại vào đó.
- Gói 103–105 làm tuần tự (103 → 104 → 105). Ba gói đều cần người dùng thử thật trên máy (quyền Screen Recording, chụp, lớp phủ); QA không giả lập chuột/phím (quyết định 126).
- Reviewer dùng `fable` (mạnh hơn builder `opus`) vì khóa bí mật, FFI `unsafe` và hành vi không kiểm tự động được.
