# TriHienKun Laucher

**TriHienKun Laucher** là trình khởi chạy và quản trị hệ sinh thái Minecraft Desktop độc lập 100% viết thuần bằng **Rust** (`eframe` / `egui 0.37` + `wgpu 30` + `tokio` + `rusqlite`), biên dịch trực tiếp ra một tệp thực thi gốc duy nhất và không phụ thuộc Electron, Node.js, WebView hay máy chủ trung gian đóng kín.

---

## Điểm Nổi Bật Cốt Lõi

- **100% Native Rust & Đồ Họa GPU `wgpu 30.0.1`**:
  - Khởi động tức thì, toàn bộ 9 crates tuân thủ nghiêm ngặt `#![deny(unsafe_code)]`.
  - Tích hợp khung xem mô hình nhân vật 3D thời gian thực bằng WGSL shader: hỗ trợ khung xương `Wide (4px)` / `Slim (3px)`, lớp giáp ngoài (`Outer Overlay`), áo choàng `Cape`, cánh `Elytra`, hoạt ảnh bước đi và xoay quỹ đạo 360°.
- **Đa Profile & Chạy Song Song Nhiều Phiên Bản (`InstanceSupervisor`)**:
  - Hỗ trợ trọn vẹn **5 Mod Loaders**: `Vanilla`, `Fabric` (tự động cài gói tối ưu FPS: *Sodium, Lithium, FerriteCore, EntityCulling, ModMenu*), `Quilt`, `Forge` (tích hợp `ForgeWrapper` cho `1.13+`) và `NeoForge`.
  - Cho phép chọn thư mục lưu trữ tùy chỉnh cho từng Profile, chạy đồng thời nhiều tài khoản/phiên bản, vào thẳng máy chủ qua `QuickPlay` và thống kê biểu đồ giờ chơi 14 ngày gần nhất trên **SQLite WAL**.
- **Xác Thực Tài Khoản An Toàn & Máy Chủ Skin Yggdrasil Cục Bộ**:
  - **Microsoft OAuth2**: Hỗ trợ đăng nhập bằng Mã Thiết Bị (`microsoft.com/link`), Desktop Redirect PKCE `S256` và Loopback PKCE tùy chỉnh; lưu trữ khóa làm mới mã hóa trong **OS Keyring** (`Windows Credential Manager` / `macOS Keychain` / `Linux Secret Service`).
  - **Offline + Custom Skin / Cape / Elytra**: Tạo Offline UUID v3 chuẩn `OfflinePlayer:` và tự động khởi chạy máy chủ HTTP Yggdrasil cục bộ (`axum` trên `127.0.0.1:0` kèm `authlib-injector`) để hiển thị đầy đủ ngoại hình trong game.
- **Quản Lý & Tải Tự Động Đa Phân Phối Java (`thk-java`)**:
  - Tự động phát hiện và tải Mojang JRE chính thức theo từng thế hệ Minecraft (`Java 8 / 17 / 21 / 25`, hỗ trợ từ `1.7.10` đến `1.21.x` và `26.x`).
  - Trình quản lý tải thủ công các bản JRE/JDK từ **Eclipse Temurin (Adoptium)**, **Azul Zulu** và **GraalVM Community**.
- **Kho Nội Dung 5 Nguồn + Phân Giải Phụ Thuộc Đệ Quy (`thk-mods`)**:
  - Tìm kiếm và cài đặt trực tiếp Modpack, Mod, Shader, ResourcePack và Plugin từ **Modrinth**, **CurseForge**, **FTB Modpacks**, **Technic Platform (+ Solder)** và **Spiget**.
  - Nhập trực tiếp gói `.mrpack` và CurseForge `.zip` từ ổ cứng; tự động nhận diện các tệp `.jar` chép tay thông qua đối chiếu băm `SHA-1` trên Modrinth.
- **Trình Quản Trị Máy Chủ Minecraft Chuyên Dụng (`thk-server`)**:
  - Cài đặt tự động **10 lõi máy chủ**: `Paper`, `Purpur`, `Folia`, `Vanilla`, `Fabric`, `Forge`, `NeoForge`, `Arclight`, `Mohist`, `SpongeVanilla`.
  - Bảng điều khiển trực tiếp hỗ trợ gợi ý lệnh bằng phím `Tab`, đo `CPU % / RAM MB / Số người chơi / Uptime` thực tế, cấu hình phần cứng & JVM sau khi tạo, trình quản lý tệp chống thoát thư mục (`SafePath`), trình sửa mã tích hợp, chỉnh sửa `server.properties`, `Whitelist` và `Banned Players`.
- **Mạng An Toàn, Ping Máy Chủ & Đường Hầm TCP Bore Nhúng (`thk-network`)**:
  - Giao thức Minecraft Server List Ping có phân giải bản ghi DNS `SRV` (`hickory-resolver`) và hiển thị màu MOTD.
  - Quét thế giới `Open to LAN` qua UDP Multicast `224.0.2.60:4445` có xác minh địa chỉ IP card mạng nội bộ (chặn gói quảng bá giả mạo từ máy khác) kết hợp thư viện `bore-cli` nhúng thuần Rust mở cổng công khai qua `bore.pub`.
- **Cá Nhân Hóa & Tiện Ích Hệ Thống**:
  - Chẩn đoán sự cố `CrashAnalyzer` kèm nút tự tải Mod thiếu 1 chạm từ Modrinth.
  - Tùy chỉnh ảnh nền giao diện (`PNG / JPG / WEBP`), âm thanh giao diện tổng hợp + nhạc nền riêng (`MP3 / OGG / WAV` qua `rodio`), điều chỉnh động số luồng tải song song (`2–32` luồng), Discord Rich Presence, Khay hệ thống (`System Tray`), Đa ngôn ngữ Việt / Anh và Trình cập nhật xác thực băm `SHA-256`.

---

## Cấu Trúc Kho Mã Nguồn

```text
trihienkun-laucher/
├── Cargo.toml                  # Cấu hình Cargo Workspace (9 crates, LTO Release)
├── assets/
│   ├── icons/                  # Biểu tượng thương hiệu (icon.png, icon.ico)
│   └── locales/                # Từ điển đa ngôn ngữ đồng bộ (vi.json, en.json)
├── crates/
│   ├── thk-core/               # AppPaths, SafePath, SQLite WAL, DownloadManager, Archive, I18n
│   ├── thk-auth/               # Microsoft OAuth2 PKCE, OS Keyring, Offline UUID v3, Local Yggdrasil
│   ├── thk-java/               # Trình quét Java hệ thống, Mojang JRE, Adoptium / Zulu / GraalVM
│   ├── thk-mods/               # Modrinth, CurseForge, FTB, Technic/Solder, Spiget & đồng bộ cục bộ
│   ├── thk-launcher/           # Vanilla + 5 Mod Loaders, InstanceSupervisor, CrashAnalyzer
│   ├── thk-server/             # 10 lõi máy chủ, LocalServerSupervisor, Quản lý tệp & cấu hình
│   ├── thk-network/            # Minecraft Ping + DNS SRV, LAN Multicast an toàn, Bore, Discord RPC
│   ├── thk-renderer3d/         # Đường ống đồ họa WGSL wgpu 30 hiển thị nhân vật 3D + Cape/Elytra
│   └── thk-app/                # Ứng dụng Desktop eframe/egui, âm thanh rodio, Tray & SHA-256 Updater
├── docs/
│   ├── ARCHITECTURE.md         # Kiến trúc chi tiết, sơ đồ phụ thuộc & luồng khởi chạy
│   ├── DATABASE_SCHEMA.md      # Lược đồ 7 bảng SQLite WAL, AppPaths & OS Keyring
│   ├── SECURITY.md             # Mô hình bảo mật & bảng đối chiếu các lớp phòng thủ
│   └── USER_GUIDE.md           # Hướng dẫn sử dụng từng tính năng từ cơ bản đến nâng cao
├── CONTRIBUTING.md             # Quy chuẩn phát triển, kiểm thử và phát hành CI/CD
└── .github/workflows/
    └── release.yml             # Pipeline kiểm thử & đóng gói đa nền tảng kèm SHA256SUMS.txt
```

---

## Tài Liệu Dự Án Chi Tiết

- [Kiến trúc Hệ thống & Luồng vận hành (`docs/ARCHITECTURE.md`)](docs/ARCHITECTURE.md)
- [Đặc tả Cơ sở dữ liệu SQLite WAL, Thư mục & Keyring (`docs/DATABASE_SCHEMA.md`)](docs/DATABASE_SCHEMA.md)
- [Mô hình Bảo mật & Kiểm soát An toàn (`docs/SECURITY.md`)](docs/SECURITY.md)
- [Hướng dẫn Sử dụng Toàn diện (`docs/USER_GUIDE.md`)](docs/USER_GUIDE.md)
- [Hướng dẫn Đóng góp & Quy trình CI/CD (`CONTRIBUTING.md`)](CONTRIBUTING.md)

---

## Hướng Dẫn Biên Dịch & Kiểm Thử Nhanh

### Yêu cầu môi trường
- **Rust**: `1.85+` (`stable`, `msvc` trên Windows).
- **Linux (Ubuntu/Debian)**:
  ```bash
  sudo apt-get install -y pkg-config libasound2-dev libgtk-3-dev libayatana-appindicator3-dev libxdo-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libssl-dev
  ```

### Kiểm thử, Phân tích Tĩnh & Đóng gói Release
```bash
# Chạy trọn bộ 26 bài kiểm thử đơn vị trên toàn bộ 9 crates
cargo test --workspace

# Kiểm tra chất lượng mã nguồn nghiêm ngặt với Clippy (0 cảnh báo)
cargo clippy --workspace --all-targets -- -D warnings

# Biên dịch tệp thực thi tối ưu hóa Release
cargo build --release -p thk-app
```

- **Tệp thực thi sau biên dịch**:
  - Windows: `target/release/trihienkun-laucher.exe`
  - Linux / macOS: `target/release/trihienkun-laucher`
- **Thư mục lưu trữ dữ liệu người dùng (`AppPaths::resolve_default`)**:
  - Windows: `%APPDATA%\.TriHienKunLaucher`
  - macOS: `~/Library/Application Support/.TriHienKunLaucher`
  - Linux: `~/.config/.TriHienKunLaucher` (hoặc `$XDG_CONFIG_HOME/.TriHienKunLaucher`)

---

## Giấy Phép

Phát hành theo giấy phép **MIT License**.
