# Đợt M: bản macOS — kế hoạch

Thiết kế: `docs/superpowers/specs/2026-11-21-macos-port-design.md`. Nhánh tích hợp `feat/macos-port`; mỗi task một nhánh `task/1NN-…` tách từ và gộp lại vào nhánh tích hợp (quyết định 127). Global Constraints: xem mục cùng tên bên dưới. Model chọn theo độ rủi ro: bảo mật hoặc toàn vẹn dữ liệu không dùng `haiku`; rà soát bởi agent khác, model mạnh bằng hoặc hơn.

| Bước | Task | Nội dung | Agent / model | Rủi ro |
|---|---|---|---|---|
| M0 | (phiên chính) | Cài Rust, test core trên Mac | xong | thấp |
| M1 | (phiên chính) | Thiết kế, kế hoạch, quyết định 127–129 | xong | thấp |
| M2a | 100 | Workspace dùng `cfg` theo hệ điều hành; crate `darkshot-macos` khung; `scripts/check.sh`; `check.yml` | phiên chính (làm trực tiếp) | trung bình |
| M2b | 101 | Chuyển logic thuần từ `main.rs`, `capture.rs`, `paths.rs` sang core (giữ test) | builder / sonnet; rà soát specialist | trung bình (chạm Windows) |
| M2c | 102 | Chuyển logic thuần từ `overlay.rs`, `history_layout.rs`, `steps_*` sang core | builder / sonnet | trung bình |
| M3a | 103 | `coords.rs`, `Dirs`, đường dẫn dữ liệu, `SecretStore` bằng Keychain, `Clipboard`, `Notifier` | specialist / opus (bảo mật khóa) | cao |
| M3b | 104 | Thanh menu, ScreenCaptureKit, kiểm quyền Screen Recording | specialist / opus | cao |
| M3c | 105 | Lớp phủ chọn vùng một màn hình Retina, lưu PNG, chép clipboard | specialist / opus | cao |
| M4a | 106 | Phím tắt toàn cục, đa màn hình, tải lên R2, link có hạn | builder / sonnet | trung bình |
| M4b | 107 | Cửa sổ Tùy chọn (3 tab), i18n, giao diện tối | builder / sonnet | trung bình |
| M4c | 108 | Đóng gói `.app`, ký ad-hoc, `scripts/demo-macos.sh`, README, website, memory bank | scaffolder / sonnet | thấp |
| M5–M7 | 109+ | Đánh dấu, Ghim, Lịch sử, bắt cửa sổ, OCR, Chụp từng bước, tự khởi động, kiểm tra bản mới: tách từng task sau khi M-A được duyệt | — | — |

Cổng mỗi task: `scripts/check.sh` (Mac) phải in `ALL CHECKS PASSED`; task chạm lớp Windows (100, 101, 102) phải qua job `windows` của `.github/workflows/check.yml` (chạy `scripts/check.ps1` trên runner `windows-latest`) trước khi gộp vào `main`, vì chủ dự án không có máy Windows (2026-11-21). Không gộp `main` khi job đó chưa xanh; cần đẩy nhánh lên `origin` để chạy.

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
