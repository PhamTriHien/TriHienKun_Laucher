# TriHienKun Laucher

**TriHienKun Laucher** là trình khởi chạy và quản trị hệ sinh thái Minecraft Desktop độc lập 100% viết thuần bằng **Rust** (`eframe` / `egui 0.37` + `wgpu 30` + `tokio` + `rusqlite`), không phụ thuộc Electron, WebView hay máy chủ trung gian đóng kín.

---

## Tính năng cốt lõi

- **100% Native Rust & GPU `wgpu` 30**: Giao diện phản hồi tức thì, tiêu thụ RAM tối thiểu, tích hợp bộ kết xuất mô hình nhân vật 3D (Wide / Slim, Outer Overlay Layer, Áo choàng Cape, Cánh Elytra uốn động, đi bộ và xoay quỹ đạo 360°) bằng WGSL shader trực tiếp trên phần cứng GPU.
- **Đa Profile & Chạy Song Song Nhiều Phiên Bản (`InstanceSupervisor`)**:
  - Hỗ trợ đầy đủ **5 Mod Loaders**: `Vanilla`, `Fabric` (tự động cài đặt gói tối ưu FPS: Sodium, Lithium, FerriteCore, EntityCulling, ModMenu), `Quilt`, `Forge` (kể cả `ForgeWrapper` cho các bản 1.13+) và `NeoForge`.
  - Hỗ trợ khởi chạy đồng thời nhiều tài khoản / Profile, QuickPlay vào thẳng máy chủ, lưu thống kê thời lượng chơi 14 ngày gần nhất vào SQLite (chế độ WAL).
- **Xác Thực Tài Khoản & Máy Chủ Skin Yggdrasil Cục Bộ**:
  - **Microsoft OAuth2**: Hỗ trợ đăng nhập bằng Mã Thiết Bị (`microsoft.com/link`), Desktop Redirect PKCE và Loopback PKCE tuỳ chỉnh; chuỗi xác thực trực tiếp `Xbox Live -> XSTS -> Minecraft Services`; lưu khóa làm mới an toàn trong **OS Keychain** (`Windows Credential Manager` / `macOS Keychain` / `Secret Service`).
  - **Offline + Custom Skin/Cape/Elytra**: Tạo UUID v3 chuẩn `OfflinePlayer:` và tự động khởi chạy máy chủ HTTP Yggdrasil cục bộ (`axum` trên `127.0.0.1:0` kèm `authlib-injector`) để hiển thị đầy đủ Skin, Cape và Elytra trong game.
- **Quản Lý Java Đa Phân Phối Tự Động (`thk-java`)**:
  - Tự động phát hiện và tải Mojang JRE chính thức theo từng phiên bản Minecraft (từ 1.7.10 đến 1.21.x / 26.x).
  - Trình cài đặt thủ công các bản `Java 8 / 11 / 17 / 21 / 25` từ **Eclipse Temurin (Adoptium)**, **Azul Zulu** và **GraalVM Community**.
- **Kho Nội Dung 4 Nền Tảng + Cài Đặt Phụ Thuộc Đệ Quy (`thk-mods`)**:
  - Duyệt, tìm kiếm và cài đặt trực tiếp Modpack, Mod, Shader, ResourcePack và Plugin từ **Modrinth**, **CurseForge**, **FTB Modpacks**, **Technic Platform (+ Solder)** và **Spiget**.
  - Nhập/xuất trực tiếp gói `.mrpack` và CurseForge `.zip` từ ổ cứng; tự động đối chiếu băm `SHA-1` với Modrinth cho các tệp `.jar` chép tay.
- **Trình Quản Trị Máy Chủ Minecraft Chuyên Dụng (`thk-server`)**:
  - Cài đặt tự động **10 lõi máy chủ**: `Paper`, `Purpur`, `Folia`, `Vanilla`, `Fabric`, `Forge`, `NeoForge`, `Arclight`, `Mohist`, `SpongeVanilla`.
  - Bảng điều khiển trực tiếp có gợi ý lệnh bằng phím `Tab`, biểu đồ tài nguyên thực `CPU % / RAM MB / Số người chơi / Uptime`, trình quản lý tệp chống thoát thư mục (`SafePath`), trình sửa mã tích hợp, chỉnh sửa `server.properties`, `Whitelist` và `Banned Players`.
- **Mạng An Toàn, Ping Máy Chủ & Đường Hầm TCP Bore (`thk-network`)**:
  - Giao thức Minecraft Server List Ping (hỗ trợ bản ghi DNS `SRV` qua `hickory-resolver` và phân tích màu MOTD).
  - Quét thế giới `Open to LAN` qua UDP Multicast `224.0.2.60:4445` có xác minh địa chỉ IP card mạng nội bộ (chặn gói quảng bá giả mạo từ máy khác) kết hợp client `bore-cli` nhúng thuần Rust mở địa chỉ công khai qua `bore.pub`.
- **Chẩn Đoán Crash & Tự Khắc Phục 1 Chạm**: Phân tích nhật ký lỗi JVM/Fabric/Forge/NeoForge và tự động tải Mod phụ thuộc còn thiếu từ Modrinth.
- **Tiện Ích Hệ Thống**: Discord Rich Presence, Khay hệ thống (System Tray), Hiệu ứng âm thanh tổng hợp thời gian thực (`rodio`), Đa ngôn ngữ Việt / Anh, và Trình cập nhật tự động xác thực băm `SHA-256`.

---

## Kiến trúc Cargo Workspace

| Crate | Vai trò |
| :--- | :--- |
| `crates/thk-core` | Đường dẫn chuẩn `.TriHienKunLaucher`, bảo vệ đường dẫn `SafePath`, kho lưu trữ SQLite WAL, trình tải song song xác thực `SHA-1`/`SHA-256`/`SHA-512`, nén/giải nén an toàn chống Zip-Slip và đa ngôn ngữ `vi`/`en`. |
| `crates/thk-auth` | Luồng đăng nhập Microsoft OAuth2/Xbox/MC, lưu trữ OS Keyring, Offline UUID v3 và máy chủ Yggdrasil cục bộ (`axum` + `authlib-injector`). |
| `crates/thk-java` | Dò tìm Java hệ thống, tải Mojang JRE theo manifest và trình cài đặt Adoptium / Azul Zulu / GraalVM. |
| `crates/thk-mods` | Client API thống nhất cho Modrinth, CurseForge, FTB, Technic/Solder, Spiget và quản lý Mod/Shader/ResourcePack/World cục bộ. |
| `crates/thk-launcher` | Trình hợp nhất đối số khởi chạy Vanilla + 5 Mod Loaders, giám sát đa tiến trình `InstanceSupervisor` và bộ phân tích `CrashAnalyzer`. |
| `crates/thk-server` | Trình cài đặt 10 lõi máy chủ, giám sát tài nguyên `LocalServerSupervisor`, cấu hình `server.properties`/Whitelist/Banlist và quản lý tệp máy chủ. |
| `crates/thk-network` | Minecraft TCP Ping + DNS SRV, bộ quét LAN Multicast an toàn, đường hầm TCP `bore` nhúng và dịch vụ Discord RPC. |
| `crates/thk-renderer3d` | Đường ống đồ họa WGSL `wgpu` 30 hiển thị mô hình nhân vật Minecraft 3D hai lớp kèm Cape & Elytra. |
| `crates/thk-app` | Ứng dụng Desktop `trihienkun-laucher` kết nối toàn bộ dịch vụ bất đồng bộ `tokio` với giao diện `eframe` / `egui`, âm thanh `rodio`, khay hệ thống và bộ cập nhật `SHA-256`. |

---

## Yêu cầu & Hướng dẫn Biên dịch

### Yêu cầu môi trường
- **Rust Toolchain**: `1.85+` (`stable`, `msvc` trên Windows).
- **Linux** (nếu biên dịch trên Ubuntu/Debian):
  ```bash
  sudo apt-get install -y pkg-config libasound2-dev libgtk-3-dev libayatana-appindicator3-dev libxdo-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libssl-dev
  ```

### Kiểm thử, Kiểm tra tĩnh & Đóng gói
```bash
# Chạy toàn bộ bộ kiểm thử đơn vị (Unit Tests) trên cả 9 crates
cargo test --workspace

# Kiểm tra chất lượng mã nguồn nghiêm ngặt với Clippy (0 cảnh báo)
cargo clippy --workspace --all-targets -- -D warnings

# Biên dịch bản thực thi tối ưu hóa Release
cargo build --release -p thk-app
```

Tệp thực thi sau khi biên dịch nằm tại:
- **Windows**: `target/release/trihienkun-laucher.exe`
- **Linux / macOS**: `target/release/trihienkun-laucher`

Dữ liệu người dùng, cơ sở dữ liệu SQLite, các Profile, Máy chủ cục bộ và Java Runtime được lưu trữ độc lập tại:
- **Windows**: `%APPDATA%\.TriHienKunLaucher`
- **macOS**: `~/Library/Application Support/TriHienKunLaucher`
- **Linux**: `~/.local/share/TriHienKunLaucher`

---

## Giấy phép

Phát hành theo giấy phép **MIT License**.
