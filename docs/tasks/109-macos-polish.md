# Task 109: thoát an toàn ngoài menu, tài liệu và hoàn thiện macOS (đợt M, bước M4d)

**Agent:** `builder` · **Model:** sonnet · **Nhánh:** `task/109-macos-polish` (từ `feat/macos-port`) · **Rà soát:** gom vào lượt rà soát opus cuối (chủ dự án, quyết định 2026-11-21) · **Cổng:** job `macos` và `windows` của `.github/workflows/check.yml`

Thiết kế: `docs/superpowers/specs/2026-11-21-macos-port-design.md`. Quyết định 127–141. Tiền lệ: Task 103–108.

## Mục tiêu

(A) Lấp chỗ hở an toàn dữ liệu còn lại của Task 108: lượt tải lên đang dở KHÔNG được mất khi ứng dụng bị thoát ngoài menu "Thoát". (B) Tài liệu cho người dùng và cho lần làm việc sau, và (C) biểu tượng ứng dụng.

## Phần A: thoát an toàn ngoài menu (làm TRƯỚC, quan trọng nhất)

Hiện chỉ đường `quit` (menu) lưu các lượt dở (`upload::save_on_exit`, trong `EXIT_BUDGET`). Đăng xuất, tắt máy, khởi động lại, `SIGTERM`, `killall darkshot` đi đường khác nên mất lượt đang dở (Windows có `WM_ENDSESSION`).
1. Cài delegate `NSApplication` (lớp `define_class!` duy nhất, tên không chứa chuỗi cấm) với `applicationShouldTerminate:` (trả `NSTerminateLater` khi còn lượt dở rồi `replyToApplicationShouldTerminate:` sau khi lưu xong, trong hạn chót chung `EXIT_BUDGET`) hoặc `applicationWillTerminate:` (đơn giản hơn nếu đủ); chọn cách an toàn hơn và nêu lý do. Dùng ĐÚNG chuỗi bước của `quit` (tách thành một hàm dùng chung có test, không nhân đôi): hủy lớp phủ, gỡ phím tắt, `save_on_exit` (không mạng), chờ tới hạn chót.
2. Bắt `SIGTERM` và `SIGINT` (nguồn `DispatchSource` loại signal trên hàng đợi chính, và bỏ xử lý mặc định của tín hiệu đó) rồi chạy cùng đường thoát. Không dùng trình xử lý tín hiệu kiểu C làm việc phức tạp (chỉ gọi hàm async-signal-safe).
3. Thoát chỉ một lần dù nhiều đường cùng kích hoạt (cờ nguyên tử); không lưu hai lần một ảnh (core `save_on_exit` đã bảo đảm 0 lần ở lượt gọi thứ hai).
4. Phần thuần có test (quyết định "còn lượt dở không → trả Later/Now", cờ một lần, thứ tự bước). Test với `PendingUploads` và thư mục tạm riêng.
5. Không chặn luồng chính quá hạn chót; hạn chót lấy từ `EXIT_BUDGET`/`remaining` của core.

## Phần B: tài liệu (tiếng Việt, theo quy ước dự án)

1. `README.md`: thêm mục "macOS (Apple silicon)" ngắn: yêu cầu macOS 14+, tính năng hiện có (chụp vùng bằng lớp phủ, phím tắt toàn cục, lưu/chép/tải lên R2, Tùy chọn, Keychain), cách build và chạy (`scripts/bundle-macos.sh --release`, ký cục bộ, xem `docs/testing/macos-local-signing.md`), quyền cần cấp (Screen Recording, thông báo), giới hạn đã biết (chưa có: ghim, lịch sử, OCR, bắt cửa sổ, chụp từng bước, đánh dấu, lặp lại lần chụp trước, tự khởi động, kiểm tra cập nhật; chưa phát hành). KHÔNG sửa mục "Tải về" hay số phiên bản (chưa phát hành).
2. `docs/testing/macos-manual-checklist.md` (mới): gom danh sách QA tay của Task 103–108 (đã nằm trong các gói `docs/tasks/103…108` và trong lịch sử trao đổi) thành một checklist đánh dấu được, ghi rõ mục nào ĐÃ đạt (103–106, chủ dự án 2026-11-21) và mục nào CHƯA (107, 108, 109A), kèm bước chạy (demo cách ly, hồ sơ cố định `/tmp/ds-qa` và JSON mẫu đã kiểm cho phím tắt, dọn Keychain).
3. `memory-bank/`: `techContext.md` (môi trường macOS, lệnh `check.sh`, `check-windows-cross.sh`, bundle, ký, phiên bản crate objc2 đã ghim, ghi chú macOS 14+), `systemPatterns.md` (mục "Lớp macOS": các module `darkshot-macos`, mẫu luồng chính/luồng nền, hàm thuần có test, hồ sơ cách ly và Keychain theo hồ sơ, bundle + ký), `progress.md` (dòng Task 109), `activeContext.md` (cập nhật trạng thái; giữ nguyên mục "CHƯA RÀ SOÁT / CHƯA QA" và cập nhật danh sách việc còn lại). Mỗi tệp giữ khoảng một trang.
4. KHÔNG sửa `website/` và `CHANGELOG.md` (chưa phát hành; quy ước website: chỉ khi phát hành). Không đưa khóa, cấu hình thật, đường dẫn cá nhân hay ảnh màn hình thật vào repo (repo công khai).

## Phần C: biểu tượng

1. `scripts/make-icns.sh`: tạo `target/macos/darkshot.icns` từ ảnh nguồn trong repo (`docs/icon-preview.png` hoặc `crates/darkshot-windows/res/darkshot.ico` nếu hợp hơn; chọn ảnh nguồn lớn nhất có sẵn và nêu lý do), bằng `sips` + `iconutil` (có sẵn trên macOS), `set -eu`, không chạm ngoài `target/macos`. Nếu ảnh nguồn nhỏ hơn 1024 px thì ghi rõ và tạo các cỡ tới mức có thể (không phóng to xấu).
2. `scripts/bundle-macos.sh`: nếu có `.icns` thì chép vào `Contents/Resources/` và đặt `CFBundleIconFile`; không có thì bỏ qua không lỗi.
3. Không chạy bundle/ứng dụng (xem an toàn).

## An toàn cho máy của chủ dự án (như mọi gói trước)

KHÔNG chạy ứng dụng hay bundle (không `open`, không thực thi `target/macos/darkshot.app/...`, không `cargo run`), KHÔNG chạy `scripts/demo-macos.sh`, KHÔNG chạy test `#[ignore]`, KHÔNG gọi thật Keychain/pasteboard/thông báo/ScreenCaptureKit/mạng, KHÔNG đăng ký phím tắt thật, KHÔNG gửi tín hiệu thật tới tiến trình, không đặt `DARKSHOT_SIGN_IDENTITY`, không tạo chứng chỉ. `make-icns.sh` được phép chạy được (chỉ `sips`/`iconutil` trên tệp ảnh trong repo, ghi vào `target/macos`) để kiểm `.icns` hợp lệ bằng `iconutil`/`file`, nhưng `bundle-macos.sh` thì chỉ `bash -n`. Chỉ build, test thường, clippy.

## Global Constraints

Như mục cùng tên của `docs/superpowers/plans/2026-11-21-macos-port.md`. Đặc biệt: mọi mã macOS dưới `cfg(target_os = "macos")` (job Windows vẫn biên dịch crate khung); không `unwrap`/`expect`/`panic` trên đường chạy thật; mọi `unsafe` có `// SAFETY:`; không chuỗi tiếng Việt cứng trong mã chạy thật; tên không chứa "shot"+"me" khi không phân biệt hoa thường; import chỉ dùng trong test chuyển vào `mod tests`; chỉ sửa `i18n.rs` để thêm chuỗi (đủ hai bản); khóa R2 không vào log; commit kết thúc bằng `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`; không tự đẩy.

## Hoàn tất

`cargo fmt --all`; `./scripts/check.sh` → `ALL CHECKS PASSED`. Báo cáo ngắn: phần A (cách chọn `ShouldTerminate` hay `WillTerminate` và lý do, các đường thoát được phủ, test), phần B (tệp đã tạo/sửa), phần C (ảnh nguồn và kết quả `iconutil`), mọi chỗ lệch gói, điều không kiểm được tự động (QA tay), rủi ro còn lại, số token.
