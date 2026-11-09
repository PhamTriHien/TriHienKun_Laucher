# Hướng Dẫn Đóng Góp & Phát Triển — TriHienKun Laucher

Tài liệu này quy định quy trình thiết lập môi trường phát triển, tiêu chuẩn chất lượng mã nguồn, quy tắc đồng bộ đa ngôn ngữ và quy trình phát hành tự động cho **TriHienKun Laucher**.

---

## 1. Yêu Cầu Môi Trường Phát Triển

- **Rust Toolchain**: Dùng `stable` hiện hành, cài đặt kèm `clippy` và `rustfmt`. Mốc kiểm chứng bản `v1.0.1` trên Windows ngày 09/11/2026 dùng `rustc 1.99.1`; chưa có MSRV được khai báo/kiểm thử cho toàn bộ dependency:
  ```bash
  rustup update stable
  rustup component add clippy rustfmt
  ```
- **Hệ điều hành & Thư viện hệ thống**:
  - **Windows 10 / 11 (`x86_64-pc-windows-msvc`)**: Cài đặt *Visual Studio Build Tools (Desktop development with C++)* và Windows 10/11 SDK.
  - **Ubuntu / Debian Linux (`x86_64-unknown-linux-gnu` / `aarch64-unknown-linux-gnu`)**:
    ```bash
    sudo apt-get update
    sudo apt-get install -y \
      pkg-config \
      libasound2-dev \
      libgtk-3-dev \
      libayatana-appindicator3-dev \
      libxdo-dev \
      libxcb-render0-dev \
      libxcb-shape0-dev \
      libxcb-xfixes0-dev \
      libxkbcommon-dev \
      libssl-dev
    ```
  - **macOS (`x86_64-apple-darwin` / `aarch64-apple-darwin`)**: Xcode Command Line Tools (`xcode-select --install`).
  - **Android (`aarch64-linux-android` / `arm64-v8a`, biên dịch chéo trên Windows)**: JDK 17+, Android SDK Platform `android-35`+, Build-Tools `35.0.0`+ (`zipalign -P 16`, `apksigner`), Android NDK `28.2.13676358`+ và mục tiêu Rust `rustup target add aarch64-linux-android`.

---

## 2. Các Lệnh Kiểm Thử & Biên Dịch Chuẩn

Mọi thay đổi mã nguồn trước khi tạo Commit hoặc Pull Request **bắt buộc** vượt qua trọn vẹn các bước kiểm tra sau với **0 lỗi, 0 cảnh báo, 0 sai lệch định dạng**:

```bash
# 1. Kiểm tra định dạng mã nguồn toàn workspace
cargo fmt --all -- --check

# 2. Chạy kiểm thử workspace (dùng --test-threads=1 cho 22 ảnh golden D3D10 WARP & PowerShell helper trên Windows)
cargo test --workspace -- --test-threads=1

# 3. Phân tích tĩnh nghiêm ngặt với Clippy (coi mọi cảnh báo là lỗi biên dịch)
cargo clippy --workspace --all-targets -- -D warnings

# 4. Biên dịch bản tối ưu hóa Release trên Desktop
cargo build --release -p thk-app

# 5. Đóng gói APK Android arm64-v8a căn chỉnh trang nhớ 16 KB (trên Windows có Android SDK + NDK)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-android-apk.ps1 -Profile release
```

Quy trình chi tiết, lệnh live Modrinth và phạm vi xác minh nằm trong [docs/TESTING.md](docs/TESTING.md). Tại mốc `v1.0.1` ngày 09/11/2026, Windows đạt **183 passed, 0 failed, 1 ignored** trên 10 crates (bao gồm 22 ảnh golden `egui_kittest` khớp `0 failed pixels`), `cargo fmt --all -- --check` đạt `0 diff` toàn workspace, doc-tests, Clippy (cả host lẫn `--target aarch64-linux-android --lib -p thk-android-ffi -p thk-app`) và build Release Desktop / Android APK đều đạt. Test bị ignore cần mạng, chạy riêng; số lượng test không phải bằng chứng mọi tính năng đã chạy đầu-cuối.

- Kiểm tra `git -c core.autocrlf=false diff --check` trước khi gửi thay đổi.
- Chạy `cargo fmt --all -- --check` để đảm bảo toàn bộ tệp `.rs` trong workspace tuân thủ `rustfmt` chuẩn.
- Golden UI chỉ chạy trên Windows bằng `egui_kittest =0.37.0` (`wgpu`, `snapshot`), với threshold `0.0` và `max_failed_pixels(0)`. Không cập nhật golden để chấp nhận giao diện lỗi hoặc nới tolerance cho test xanh; giữ `VISUAL_RENDER_LOCK` khi render snapshot.
- Regression dùng SQLite/filesystem cô lập. Không dùng cơ sở dữ liệu cá nhân, OS Keyring của người dùng hoặc tài khoản Microsoft thật làm fixture. Dữ liệu tổng hợp trong test không được trình bày như phản hồi dịch vụ thật.
- Với thay đổi UI/runtime, ghi thêm kết quả desktop native và tên bản executable đã chạy. Build thành công hoặc helper tạo được process không thay thế kiểm tra app thực sự hoạt động. Login Microsoft và chấp thuận EULA phải do người dùng chủ động thực hiện.

---

## 3. Quy Chuẩn Kiến Trúc & An Toàn Mã Nguồn

1. **Thuần Rust (`0` tệp C/C++) & Phân tách biên `unsafe`**: Toàn dự án tuyệt đối không chứa mã C/C++ (`0` tệp `.c`, `.h`, `.cpp`, `.hpp`). Giữ nguyên `#![deny(unsafe_code)]` ở đầu toàn bộ **9 crate cốt lõi** (`thk-core`, `thk-auth`, `thk-java`, `thk-mods`, `thk-launcher`, `thk-network`, `thk-server`, `thk-renderer3d`, `thk-app`; chỉ miễn trừ duy nhất chữ ký xuất điểm vào `#[cfg(target_os = "android")] #[allow(unsafe_code)] #[no_mangle] pub fn android_main` không chứa khối `unsafe` trong `crates/thk-app/src/lib.rs`). Toàn bộ tương tác FFI/JNI/Bionic trên Android được cô lập vào crate chuyên trách `crates/thk-android-ffi` (mọi hàm `extern "system"` bắt buộc bọc `std::panic::catch_unwind`, kiểm tra `NULL` và kiểm tra/xóa ngoại lệ tồn đọng qua `ExceptionCheck` / `ExceptionClear` cho Android `CheckJNI`), đồng thời thư mục `android/java/` chỉ giữ vỏ `.dex` mỏng chuyển tiếp lời gọi `native` và Android UI Looper SDK.
2. **Tách biệt tầng nghiệp vụ và giao diện**:
   - Logic nghiệp vụ, giao thức mạng, giải nén, cài đặt nội dung theo giao dịch (`thk-mods::install`) và cơ sở dữ liệu phải đặt trong các crate nền (`thk-core`, `thk-auth`, `thk-java`, `thk-mods`, `thk-launcher`, `thk-server`, `thk-network`, `thk-renderer3d`) kèm bài kiểm thử đơn vị độc lập.
   - `thk-app` điều phối trạng thái giao diện `egui`, âm thanh, khay hệ thống và kết nối sự kiện `tokio`; các module chuyên trách `pack_install` và `updater` (cùng `apply_update.ps1`) xử lý giao dịch Profile/modpack và helper cập nhật ứng dụng. Không rải logic filesystem của các luồng này vào hàm vẽ UI.
3. **Không chặn luồng giao diện (`eframe::App::ui`)**:
   - Không gọi mạng đồng bộ, không giải nén tệp lớn hoặc chờ tiến trình con trực tiếp trên luồng vẽ UI. Dùng runtime `Handle` của ứng dụng cho tác vụ async và `spawn_blocking` cho công việc đồng bộ nặng, gửi kết quả qua `AppAsyncEvent`; không dựa vào runtime ngầm định trên luồng UI. Khi hủy tác vụ giải nén (`extract_zip_prefix_cancellable`), caller phải chờ `spawn_blocking` kết thúc trước khi dọn thư mục staging. Updater áp dụng qua `UpdateApplyCompleted`, không đóng launcher khi chuẩn bị helper thất bại.
4. **An toàn đường dẫn tệp tin**:
   - Mọi thao tác đọc, ghi, xóa hoặc giải nén dựa trên tên tệp bên ngoài (từ gói `.zip`/`.mrpack`, phản hồi API hoặc ô nhập liệu) bắt buộc đi qua `thk_core::SafePath::resolve_child` hoặc `SafePath::validate_single_name`.
5. **Đồng bộ Đa ngôn ngữ (`vi` / `en`)**:
   - Khi thêm bất kỳ khóa chuỗi nào vào `assets/locales/vi.json`, bắt buộc thêm khóa tương ứng vào `assets/locales/en.json`.
   - Bài kiểm thử `thk_core::tests::i18n_keys_synchronized` sẽ tự động kiểm chứng sự đồng nhất 100% giữa hai tệp ngôn ngữ.

---

## 4. Quy Trình Phát Hành Tự Động (CI/CD GitHub Actions)

Quy trình `.github/workflows/release.yml` tự động vận hành qua 4 job để kiểm định và xuất bản đầy đủ **6 nền tảng phát hành** khớp với `updater::expected_asset_name_for`:

1. **`quality-gate`**: Chạy song song trên cả **`ubuntu-24.04`** và **`windows-latest`** với các cổng bắt buộc: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` và `cargo test --workspace -- --test-threads=1` (trên runner Windows Server dùng `--skip views::servers::visual_tests::visual_` để thực thi toàn bộ 179 bài kiểm thử đơn vị/giao dịch/PowerShell self-updater/`egui_kittest` bounds/AccessKit và tách riêng 4 hàm so khớp pixel `threshold(0.0)` của 22 ảnh golden vốn phụ thuộc bản sửa đổi font hệ thống/GPU driver máy trạm cục bộ theo `crates/thk-app/tests/README.md`).
2. **`build-matrix`** *(5 nền tảng Desktop)*: Sau khi vượt qua `quality-gate`, biên dịch song song các tệp thực thi tối ưu hóa Release:
   - `trihienkun-laucher-windows-x64.exe` (`x86_64-pc-windows-msvc` trên `windows-latest`)
   - `trihienkun-laucher-linux-x64` (`x86_64-unknown-linux-gnu` trên `ubuntu-24.04`)
   - `trihienkun-laucher-linux-arm64` (`aarch64-unknown-linux-gnu` trên `ubuntu-24.04-arm`)
   - `trihienkun-laucher-macos-x64` (`x86_64-apple-darwin` trên `macos-latest`)
   - `trihienkun-laucher-macos-arm64` (`aarch64-apple-darwin` trên `macos-latest`)
3. **`build-android`** *(Nền tảng thứ 6 — Android Native)*: Sau khi vượt qua `quality-gate`, trên `windows-latest` kiểm tra `cargo clippy --target aarch64-linux-android --lib -p thk-android-ffi -p thk-app -- -D warnings` và đóng gói APK căn chỉnh trang nhớ `16 KB` qua `scripts/build-android-apk.ps1 -Profile release`:
   - `trihienkun-laucher-android-arm64.apk` (`aarch64-linux-android` / `arm64-v8a`)
4. **`publish-release`**: Khi đẩy thẻ phiên bản dạng `v*` (ví dụ `git tag v1.0.1 && git push origin v1.0.1`), hệ thống hợp nhất các tệp băm theo thứ tự `LC_ALL=C sort` vào `SHA256SUMS.txt` duy nhất (loại bỏ các tệp `.sha256` lẻ) và phát hành trọn bộ **6 gói cài đặt + `SHA256SUMS.txt`** lên GitHub Releases để trình cập nhật trong ứng dụng (`updater.rs`) tải về và đối chiếu `SHA-256`.
