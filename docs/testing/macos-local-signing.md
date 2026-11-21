# Ký bản macOS bằng chứng chỉ tự ký cục bộ

Quyết định 137. Bản dùng nội bộ của darkshot cho macOS không notarize. macOS gắn quyền **Ghi màn hình** (Screen Recording) và quyền đọc mục **Keychain** với danh tính ký của ứng dụng. Ký ad-hoc (`-`) tạo danh tính mới sau mỗi lần build, nên hệ thống hỏi lại quyền mỗi lần. Một chứng chỉ tự ký "Code Signing" tạo một lần trên máy giữ danh tính ổn định giữa các lần build.

Chứng chỉ chỉ nằm trong Keychain đăng nhập của máy bạn. **Không** đưa chứng chỉ, tệp `.p12`, tên chứng chỉ thật hay Team ID vào repo (repo công khai).

## 1. Tạo chứng chỉ (một lần)

1. Mở **Keychain Access** (Truy cập Chuỗi khóa).
2. Menu **Keychain Access → Certificate Assistant → Create a Certificate…** (Trợ lý Chứng nhận → Tạo Chứng nhận…).
3. Điền:
   - **Name**: một tên tùy ý, ví dụ `darkshot local signing` (dưới đây gọi là `<tên chứng chỉ>`).
   - **Identity Type**: `Self-Signed Root`.
   - **Certificate Type**: `Code Signing`.
4. Bấm **Create**, rồi **Done**. Chứng chỉ và khóa riêng nằm trong Keychain `login`.

Kiểm tra chứng chỉ đã dùng được để ký (chứng chỉ tự ký không được hệ thống tin cậy nên có thể kèm ghi chú `CSSMERR_TP_NOT_TRUSTED`; `codesign` vẫn ký được):

```sh
security find-identity -p codesigning
```

## 2. Build và ký

```sh
export DARKSHOT_SIGN_IDENTITY="<tên chứng chỉ>"
scripts/bundle-macos.sh            # bản debug
scripts/bundle-macos.sh --release  # bản release
```

Kết quả: `target/macos/darkshot.app`. Lần ký đầu, macOS có thể hỏi cho `codesign` dùng khóa riêng: chọn **Always Allow** (Luôn cho phép) để các lần build sau không hỏi.

Không đặt `DARKSHOT_SIGN_IDENTITY` thì script ký ad-hoc và in cảnh báo: quyền Ghi màn hình và Keychain sẽ bị hỏi lại sau mỗi lần build.

Không ghi `DARKSHOT_SIGN_IDENTITY` vào tệp nào trong repo; đặt trong shell hoặc trong tệp cấu hình shell riêng của bạn.

## 3. Kiểm tra chữ ký

```sh
codesign -dv --verbose=4 target/macos/darkshot.app
```

Cần thấy `Identifier=app.darkshot.macos` và một dòng `Authority=<tên chứng chỉ>` (ký ad-hoc thì thấy `Signature=adhoc`). Kiểm tra gói còn nguyên vẹn:

```sh
codesign --verify --strict --verbose=2 target/macos/darkshot.app
```

## 4. Quyền và cách đặt lại

- **Ghi màn hình**: lần đầu chọn "Chụp màn hình", macOS hỏi quyền. Cấp ở **System Settings → Privacy & Security → Screen Recording** (macOS 15: **Screen & System Audio Recording**), rồi mở lại darkshot. Đặt lại quyền (ví dụ sau khi đổi chứng chỉ):

  ```sh
  tccutil reset ScreenCapture app.darkshot.macos
  ```

- **Keychain**: khi đọc khóa R2 đã lưu, macOS có thể hỏi "darkshot muốn dùng thông tin bí mật…". Chọn **Always Allow**. Với cùng chứng chỉ, build lại không hỏi lại; đổi chứng chỉ hoặc ký ad-hoc thì hỏi lại. Bấm **Deny** (Từ chối) thì darkshot coi như mất khóa và báo cần nhập lại, không crash.

- **Thông báo**: quyền gửi thông báo gắn với bundle id `app.darkshot.macos`; đổi ở **System Settings → Notifications → darkshot**.

## 5. Đổi hoặc xóa chứng chỉ

Xóa chứng chỉ trong Keychain Access (mục **My Certificates**). Sau khi đổi chứng chỉ, chạy `tccutil reset ScreenCapture app.darkshot.macos` và cấp lại quyền.
