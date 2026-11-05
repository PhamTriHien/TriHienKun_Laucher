# Hướng Dẫn Đóng Góp & Phát Triển — TriHienKun Laucher

Tài liệu này quy định quy trình thiết lập môi trường phát triển, tiêu chuẩn chất lượng mã nguồn, quy tắc đồng bộ đa ngôn ngữ và quy trình phát hành tự động cho **TriHienKun Laucher**.

---

## 1. Yêu Cầu Môi Trường Phát Triển

- **Rust Toolchain**: Phiên bản `1.85` trở lên (`stable`), cài đặt kèm `clippy` và `rustfmt`:
  ```bash
  rustup update stable
  rustup component add clippy rustfmt
  ```
- **Hệ điều hành & Thư viện hệ thống**:
  - **Windows 10 / 11 (`x86_64-pc-windows-msvc`)**: Cài đặt *Visual Studio Build Tools (Desktop development with C++)* và Windows 10/11 SDK.
  - **Ubuntu / Debian Linux (`x86_64-unknown-linux-gnu`)**:
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

---

## 2. Các Lệnh Kiểm Thử & Biên Dịch Chuẩn

Mọi thay đổi mã nguồn trước khi tạo Commit hoặc Pull Request **bắt buộc** vượt qua trọn vẹn 3 bước kiểm tra sau với **0 lỗi, 0 cảnh báo**:

```bash
# 1. Chạy toàn bộ Unit Tests trên cả 9 crates
cargo test --workspace

# 2. Phân tích tĩnh nghiêm ngặt với Clippy (coi mọi cảnh báo là lỗi biên dịch)
cargo clippy --workspace --all-targets -- -D warnings

# 3. Biên dịch bản tối ưu hóa Release
cargo build --release -p thk-app
```

---

## 3. Quy Chuẩn Kiến Trúc & An Toàn Mã Nguồn

1. **Cấm mã `unsafe`**: Giữ nguyên `#![deny(unsafe_code)]` ở đầu mọi crate. Không thêm khối `unsafe` dưới bất kỳ hình thức nào.
2. **Tách biệt tầng nghiệp vụ và giao diện**:
   - Logic nghiệp vụ, giao thức mạng, giải nén và cơ sở dữ liệu phải đặt trong các crate nền (`thk-core`, `thk-auth`, `thk-java`, `thk-mods`, `thk-launcher`, `thk-server`, `thk-network`, `thk-renderer3d`) kèm bài kiểm thử đơn vị độc lập.
   - `thk-app` chỉ đảm nhiệm điều phối trạng thái giao diện `egui`, âm thanh, khay hệ thống và kết nối sự kiện `tokio`.
3. **Không chặn luồng giao diện (`eframe::App::ui`)**:
   - Không gọi mạng đồng bộ, không giải nén tệp lớn hoặc chờ tiến trình con trực tiếp trên luồng vẽ UI. Luôn thực thi trong `self.rt.spawn(...)` và gửi kết quả qua `AppAsyncEvent`.
4. **An toàn đường dẫn tệp tin**:
   - Mọi thao tác đọc, ghi, xóa hoặc giải nén dựa trên tên tệp bên ngoài (từ gói `.zip`/`.mrpack`, phản hồi API hoặc ô nhập liệu) bắt buộc đi qua `thk_core::SafePath::resolve_child` hoặc `SafePath::validate_single_name`.
5. **Đồng bộ Đa ngôn ngữ (`vi` / `en`)**:
   - Khi thêm bất kỳ khóa chuỗi nào vào `assets/locales/vi.json`, bắt buộc thêm khóa tương ứng vào `assets/locales/en.json`.
   - Bài kiểm thử `thk_core::tests::i18n_keys_synchronized` sẽ tự động kiểm chứng sự đồng nhất 100% giữa hai tệp ngôn ngữ.

---

## 4. Quy Trình Phát Hành Tự Động (CI/CD GitHub Actions)

Quy trình `.github/workflows/release.yml` tự động vận hành theo 3 giai đoạn:

1. **`quality-gate`**: Chạy `cargo test --workspace` và `cargo clippy --workspace --all-targets -- -D warnings` trên mọi lần đẩy mã (`push`) và `pull_request`.
2. **`build-matrix`**: Biên dịch chéo song song 4 tệp thực thi tối ưu hóa Release:
   - `trihienkun-laucher-windows-x64.exe` (`x86_64-pc-windows-msvc`)
   - `trihienkun-laucher-linux-x64` (`x86_64-unknown-linux-gnu`)
   - `trihienkun-laucher-macos-x64` (`x86_64-apple-darwin`)
   - `trihienkun-laucher-macos-arm64` (`aarch64-apple-darwin`)
3. **`publish-release`**: Khi đẩy thẻ phiên bản dạng `v*` (ví dụ `git tag v1.0.1 && git push origin v1.0.1`), hệ thống tự động tổng hợp tệp băm `SHA256SUMS.txt` và phát hành trọn bộ gói cài đặt lên GitHub Releases để trình cập nhật trong ứng dụng (`updater.rs`) tải về và đối chiếu `SHA-256`.
