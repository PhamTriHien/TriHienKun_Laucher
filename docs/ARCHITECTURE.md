# Kiến Trúc Hệ Thống — TriHienKun Laucher

Tài liệu này mô tả kiến trúc tổng thể, sơ đồ phụ thuộc giữa 9 crates trong Cargo Workspace, luồng giao tiếp bất đồng bộ giữa `tokio` và `egui`, quy trình khởi chạy đa tiến trình Minecraft (`InstanceSupervisor`), máy chủ Yggdrasil cục bộ, trình quản trị máy chủ chuyên dụng (`LocalServerSupervisor`) và đường ống đồ họa 3D `wgpu`.

---

## 1. Tổng Quan Công Nghệ & Ràng Buộc Kỹ Thuật

- **Ngôn ngữ & Biên dịch**: Rust Edition 2021 (`1.85+`), cấu hình tối ưu hóa Release `opt-level = 3`, `lto = "thin"`, `codegen-units = 1`, `strip = "symbols"`.
- **An toàn bộ nhớ tuyệt đối**: Toàn bộ 9 crates trong workspace đều khai báo `#![deny(unsafe_code)]` ở cấp gốc (`lib.rs` / `main.rs`) và `unused_must_use = "deny"` trong `Cargo.toml`.
- **Khung giao diện & Đồ họa GPU**: `eframe 0.37.0` + `egui 0.37.0` + `egui-wgpu 0.37.0` chạy trực tiếp trên `wgpu 30.0.1` (DX12 / Vulkan / Metal) — hoàn toàn không sử dụng Electron, Node.js hay WebView2.
- **Runtime bất đồng bộ**: `tokio 1.54.1` (`rt-multi-thread`) xử lý toàn bộ mạng HTTP/2 (`reqwest 0.13.5`), tiến trình con JVM, quét UDP Multicast, đường hầm TCP (`bore-cli 0.6.0`) và máy chủ HTTP cục bộ (`axum 0.8.9`).
- **Lưu trữ bền vững**:
  - Cấu hình, Profile, Tài khoản, Máy chủ, Thống kê giờ chơi: SQLite nhúng (`rusqlite 0.41.1`, tính năng `bundled`) vận hành ở chế độ `WAL` (`Write-Ahead Logging`).
  - Khóa truy cập và làm mới OAuth2 Microsoft: Lưu trữ mã hóa cấp hệ điều hành thông qua `keyring 3.6.3` (`Windows Credential Manager`, `macOS Keychain`, `Linux Secret Service`).

---

## 2. Sơ Đồ Phụ Thuộc Cargo Workspace (9 Crates — 56 Tệp `.rs`)

```mermaid
flowchart TD
    APP["thk-app (trihienkun-laucher)\nUI egui + rodio + tray-icon + updater"]
    LAUNCHER["thk-launcher\nVanilla + 5 Loaders + InstanceSupervisor + CrashAnalyzer"]
    SERVER["thk-server\n10 Server Engines + LocalServerSupervisor + Safe File Manager"]
    MODS["thk-mods\nModrinth + CurseForge + FTB + Technic + Spiget + Local Sync"]
    NETWORK["thk-network\nSLP Ping + DNS SRV + Safe LAN Multicast + Bore + Discord RPC"]
    AUTH["thk-auth\nMicrosoft OAuth2 PKCE + Keyring + Offline UUID v3 + Local Yggdrasil"]
    JAVA["thk-java\nSystem Scanner + Mojang JRE + Adoptium / Zulu / GraalVM"]
    RENDERER["thk-renderer3d\nwgpu 30 WGSL Character + Outer Layer + Cape + Elytra"]
    CORE["thk-core\nAppPaths + SafePath + SQLite WAL + DownloadManager + Safe Archive + I18n"]

    APP --> LAUNCHER
    APP --> SERVER
    APP --> MODS
    APP --> NETWORK
    APP --> AUTH
    APP --> JAVA
    APP --> RENDERER
    APP --> CORE

    LAUNCHER --> AUTH
    LAUNCHER --> JAVA
    LAUNCHER --> MODS
    LAUNCHER --> CORE

    SERVER --> JAVA
    SERVER --> NETWORK
    SERVER --> CORE

    MODS --> CORE
    NETWORK --> CORE
    AUTH --> CORE
    JAVA --> CORE
    RENDERER --> CORE
```

### Chi Tiết Vai Trò Từng Crate

| Crate | Các Module Chính | Trách Nhiệm Cốt Lõi |
| :--- | :--- | :--- |
| `crates/thk-core` | `paths`, `db`, `downloader`, `archive`, `hash`, `i18n`, `models`, `error` | Định nghĩa mô hình miền, đường dẫn chuẩn `.TriHienKunLaucher`, bộ kiểm tra chống thoát thư mục `SafePath`, cơ sở dữ liệu SQLite WAL, bộ tải luồng song song `DownloadManager` (điều chỉnh động `2..=32` luồng, kiểm tra `SHA-1`/`SHA-256`/`SHA-512` trên luồng tải `.part`), giải nén ZIP/TAR.GZ chống Zip-Slip và đa ngôn ngữ `vi`/`en`. |
| `crates/thk-auth` | `microsoft`, `offline`, `token_store`, `yggdrasil` | Luồng đăng nhập Microsoft (Device Code, Desktop Redirect PKCE S256, Loopback PKCE S256) → Xbox Live → XSTS → Minecraft Services; quản lý khóa trong OS Keyring; sinh Offline UUID v3 (`OfflinePlayer:<name>`); máy chủ HTTP `axum` Yggdrasil cục bộ phục vụ Skin/Cape/Elytra qua `authlib-injector`. |
| `crates/thk-java` | `scanner`, `mojang_jre`, `distros` | Tự động ánh xạ phiên bản Minecraft sang chuẩn Java (`8`, `17`, `21`, `25`), dò tìm Java trên máy, tải Mojang JRE theo manifest chính thức và cài đặt trực tiếp các gói `Adoptium`, `Azul Zulu`, `GraalVM`. |
| `crates/thk-mods` | `modrinth`, `curseforge`, `ftb`, `technic`, `spiget`, `matcher`, `provider` | Tích hợp API tìm kiếm, phân giải phụ thuộc đệ quy và cài đặt gói nội dung từ 5 nhà cung cấp; nhập trực tiếp `.mrpack` và CurseForge `.zip`; đồng bộ hóa thư mục cục bộ và đối chiếu mã băm `SHA-1` hàng loạt trên Modrinth. |
| `crates/thk-launcher` | `vanilla`, `loaders`, `supervisor`, `crash` | Đánh giá quy tắc điều kiện hệ điều hành/kiến trúc Mojang, hợp nhất thư viện Maven theo khóa chống trùng lặp `(group, artifact, classifier)`, cài đặt 5 Mod Loaders (`Vanilla`, `Fabric`, `Quilt`, `Forge`, `NeoForge`), quản lý đa phiên bản game chạy song song và phân tích nhật ký sự cố `CrashAnalyzer`. |
| `crates/thk-server` | `jars`, `manager`, `config_files` | Tải và cài đặt tự động 10 lõi máy chủ chuyên dụng, giám sát tiến trình máy chủ (`sysinfo` đo CPU/RAM thực tế, đếm người chơi trực tuyến), lọc ký tự điều khiển đầu vào Console, trình quản lý tệp an toàn và chỉnh sửa `server.properties` / `whitelist.json` / `banned-players.json`. |
| `crates/thk-network` | `ping`, `lan_tunnel`, `discord_rpc` | Thực thi giao thức Minecraft Server List Ping (kèm phân giải bản ghi DNS `_minecraft._tcp` SRV và định dạng màu MOTD), lắng nghe gói quảng bá `Open to LAN` có xác thực IP card mạng nội bộ, nhúng trực tiếp thư viện `bore-cli` mở đường hầm TCP và đồng bộ trạng thái Discord Rich Presence. |
| `crates/thk-renderer3d` | `geometry`, `pipeline`, `skin_image` | Xây dựng lưới nhân vật 3D 2 lớp (Inner Body + Outer Overlay + Cape + Elytra), bệ sân khấu bát giác 3D kèm vòng năng lượng Hologram (`build_stage_pedestal_meshes`), chuyển đổi skin `64x32` sang `64x64`, kết xuất MSAA 4x bằng cặp WGSL shader (`SKIN_SHADER_WGSL` + `STAGE_SHADER_WGSL`) có chiếu sáng viền Fresnel và làm mượt camera lò xo mũ (`snap_to_preset`). |
| `crates/thk-app` | `main`, `fx3d`, `state`, `theme`, `audio`, `tray`, `updater`, `views/*` | Điểm vào ứng dụng Desktop, bộ máy đồ họa giao diện 3D 60 FPS (`fx3d`: chiếu phối cảnh `Mat4` cho 9 biểu tượng 3D đa bộ phận, nền Parallax 3 lớp, thẻ/nút cơ học 3D, biểu đồ cột Isometric 3D), điều phối trạng thái bất đồng bộ, âm thanh `rodio`, khay hệ thống `tray-icon` và bộ cập nhật `SHA-256`. |

---

## 3. Mô Hình Sự Kiện Bất Đồng Bộ Không Chặn Luồng Giao Diện

Luồng giao diện `egui` không bao giờ chờ mạng hay đĩa cứng đồng bộ. Mọi tác vụ I/O đều chạy trên `tokio::runtime::Handle` và đẩy sự kiện về kênh không giới hạn `mpsc::UnboundedSender<AppAsyncEvent>` kèm lời gọi `egui_ctx.request_repaint()`:

```mermaid
sequenceDiagram
    participant UI as Luồng Giao diện (eframe / egui)
    participant State as LauncherAppState
    participant Tokio as Tokio Multi-Thread Pool
    participant Sub as Supervisors (Game / Server / LAN)

    UI->>State: Tương tác người dùng (Nhấn Play / Tìm Modpack / Mở Tunnel)
    State->>Tokio: rt.spawn(async move { ... })
    UI-->>UI: Tiếp tục vẽ mượt 60+ FPS (không khóa cứng khung hình)
    Tokio->>Sub: Tải tệp song song / Khởi chạy JVM / Mở Bore Tunnel
    Sub-->>State: Gửi AppAsyncEvent qua mpsc::unbounded_channel
    Sub-->>UI: egui_ctx.request_repaint()
    UI->>State: drain_async_events() đầu mỗi chu kỳ vẽ
    State-->>UI: Cập nhật thanh tiến trình, nhật ký ảo hóa & thông báo Toast
```

---

## 4. Quy Trình Khởi Chạy Đa Tiến Trình (`InstanceSupervisor`)

`InstanceSupervisor` định danh mỗi phiên chơi bằng khóa kép `InstanceKey { profile_id, account_id }`, cho phép chạy song song nhiều Profile hoặc nhiều tài khoản cùng lúc mà không ghi đè trạng thái của nhau:

1. **Làm mới Xác thực (`ensure_valid_minecraft_token`)**:
   - Tài khoản Microsoft: Kiểm tra hạn token; nếu sắp hết hạn trong vòng 5 phút sẽ tự động dùng `ms_refresh_token` từ OS Keyring để lấy token mới trước khi khởi chạy.
   - Tài khoản Offline có gắn Skin / Cape / Elytra tùy chỉnh: Tự động kiểm tra `authlib-injector.jar` và khởi tạo máy chủ HTTP Yggdrasil cục bộ trên `127.0.0.1:0` (cổng ngẫu nhiên do hệ điều hành cấp phát), truyền cờ `-javaagent:<authlib>=http://127.0.0.1:<port>` vào JVM.
2. **Chuẩn bị Java Runtime (`RequiredJavaSpec` + `ensure_mojang_java`)**:
   - Đọc trường `javaVersion` từ manifest phiên bản hoặc suy luận chính xác theo thế hệ Minecraft (`<= 1.16.5` → Java 8; `1.17.x` → Java 17; `1.18–1.20.4` → Java 17; `1.20.5–1.21.x` → Java 21; `26.x+` → Java 25).
   - Nếu người dùng không chỉ định đường dẫn Java riêng trên Profile, tự động tải và xác thực từng tệp của Mojang JRE chính thức vào `runtimes/`.
3. **Đồng bộ Tệp Gốc & Mod Loader (`prepare_vanilla_files` + `prepare_loader_patch`)**:
   - Tải song song `client.jar`, thư viện Maven, thư viện gốc (`natives`) và đối tượng tài nguyên (`assets/objects`) có kiểm tra mã băm `SHA-1`.
   - Cài đặt và hợp nhất cấu hình của `Fabric`, `Quilt`, `Forge` (hỗ trợ cả trình cài đặt tự động kèm `ForgeWrapper` cho `1.13+` và `universal.jar` cho bản cũ) hoặc `NeoForge`.
   - Khử trùng lặp thư viện trên Classpath theo bộ ba `(group_id, artifact_id, classifier)` để giữ nguyên các gói native classifier (như `natives-windows`) đồng thời ưu tiên phiên bản thư viện mới hơn từ Mod Loader.
   - Nếu Profile dùng `Fabric` và bật `auto_perf_mods`, tự động kiểm tra và tải các bản tương thích mới nhất của `Sodium`, `Lithium`, `FerriteCore`, `EntityCulling` và `ModMenu` từ Modrinth.
4. **Tối ưu GPU & Khởi tạo Tiến trình JVM**:
   - Trên Windows, đăng ký khóa `HKCU\Software\Microsoft\DirectX\UserGpuPreferences` (`GpuPreference=2;`) cho tệp thực thi `java.exe` để ưu tiên card đồ họa rời hiệu năng cao.
   - Bảo toàn đồng thời cả tham số JVM của người dùng (`profile.jvm_args`), tham số của Mojang và tham số của Mod Loader; tự động chọn `--quickPlayMultiplayer <host:port>` (cho `1.20+` / `25+`) hoặc `--server <host> --port <port>` (cho các bản cũ hơn).
5. **Thu thập Nhật ký, Thống kê & Phân tích Crash**:
   - Thu thập bất đồng bộ `stdout` và `stderr` vào bộ đệm vòng `4,000` dòng mỗi tiến trình.
   - Khi tiến trình kết thúc: ghi nhận nguyên tử (`atomic SQL UPDATE/UPSERT`) tổng giây chơi, số phiên và biểu đồ ngày vào bảng `profiles` + `daily_play_stats` mà không ghi đè các chỉnh sửa cấu hình Profile trong lúc đang chơi.
   - Nếu thoát bất thường (`exit_code != 0` và không phải do người dùng bấm Dừng), kích hoạt `analyze_crash_logs` để bóc tách nguyên nhân gốc (thiếu Mod phụ thuộc, xung đột phiên bản Java, tràn bộ nhớ `OutOfMemoryError`, xung đột Mixin) và đề xuất nút sửa tự động 1 chạm.

---

## 5. Đường Ống Kết Xuất Nhân Vật 3D (`thk-renderer3d`) & Giao Diện Gaming 3D (`fx3d`)

`SkinPreviewViewport` kết xuất vào đích trung gian khử răng cưa đa mẫu **MSAA 4x** (`Depth24Plus`) trên `egui_wgpu::RenderState` và hiển thị đồng bộ tại cả **Banner Arena Trang chủ** lẫn **Phòng Studio Tài khoản**:

- **Chuẩn hóa Texture (`skin_image.rs`)**: Tự động chuyển đổi ảnh skin cổ điển `64x32` sang bố cục chuẩn `64x64` có lật đối xứng tay/chân trái; chuẩn hóa texture Cape/Elytra về tỉ lệ chuẩn `2:1` (tối thiểu `64x32`).
- **Xây dựng Hình học Nhân vật & Sân khấu Hologram (`geometry.rs`)**:
  - Hỗ trợ cả hai khung xương `Wide` (tay rộng `4px`) và `Slim` (tay rộng `3px`), 6 khối cơ thể trong và 6 khối lớp phủ ngoài (`0.25–0.5` đơn vị), áo choàng `Cape` và cánh `Elytra`.
  - Sinh động lưới sân khấu 3D (`build_stage_pedestal_meshes`): đài bát giác 2 tầng, tâm phát sáng nhịp thở, 2 vòng năng lượng Hologram xoay ngược chiều và 6 tinh thể Voxel bay quỹ đạo 3D quanh nhân vật.
- **Cặp Shader WGSL & Camera Lò xo (`pipeline.rs`)**:
  - `STAGE_SHADER_WGSL` kết xuất sân khấu 3D ở lượt vẽ đầu; `SKIN_SHADER_WGSL` áp dụng ma trận chiếu phối cảnh thuận tay phải miền sâu `[0.0, 1.0]`, lấy mẫu `Nearest` giữ nét Pixel Art, kết hợp chiếu sáng khuếch tán hai nguồn và viền sáng kép **Fresnel Rim** (Emerald + Violet) phụ thuộc góc nhìn `eye_pos`.
  - Góc xoay `yaw`, `pitch` và khoảng cách `distance` nội suy mượt theo hàm mũ (`1 - exp(-14 * dt)`), hỗ trợ chuyển nhanh giữa các góc máy định sẵn (`snap_to_preset`).
- **Bộ máy Hoạt ảnh Giao diện 3D (`crates/thk-app/src/fx3d.rs`)**:
  - `Poly3dBuffer`: Phép chiếu phối cảnh `glam::Mat4`, loại bỏ mặt khuất (`backface culling`), tô bóng Lambertian + Rim và sắp xếp chiều sâu Painter's Algorithm ở tốc độ 60 FPS cho 9 biểu tượng 3D đa bộ phận (`BrandVoxelCore`, `HomePortal`, `ModpackCrate`, `ServerBladeTower`, `AccountCharacter`, `JavaReactor`, `LanRadar`, `SettingsGyro`, `LaunchPrism`).
  - Các thành phần tương tác 3D: nền lưới chân trời 3D Parallax 3 lớp (`paint_gaming_3d_backdrop`), thẻ vát cạnh phản chiếu vệt sáng theo con trỏ (`gaming_card_3d`), nút cơ học có đế nổi 3D và độ lún `0.97` khi nhấn (`tactile_button_3d`), thanh chuyển tab trượt mượt (`sliding_pill_tabs`), biểu đồ cột khối Voxel Isometric 3D (`paint_3d_isometric_bars`) và ống dẫn dòng gói tin Bore (`paint_3d_tunnel_conduit`).
