# Kiểm Thử & Trạng Thái Xác Minh — TriHienKun Laucher

Mốc ghi nhận: **09/11/2026** (phiên bản **`v1.0.1`**), trên Windows & thiết bị Android vật lý, toolchain **rustc 1.99.1** (quy mô mã nguồn: **`80` tệp `.rs`**, **`70.137` dòng mã Rust** trên **9 crates** khai báo `#![deny(unsafe_code)]`). Đây là kết quả xác minh cục bộ tại thời điểm ghi nhận, không thay thế kiểm chứng runtime trên mọi hệ điều hành. Tách rõ kiểm thử cô lập, dịch vụ thật, desktop native và thiết bị vật lý; không suy từ một lớp sang lớp khác.

## 1. Lệnh Kiểm Tra

Chạy từ thư mục chứa `Cargo.toml` của workspace:

```bash
cargo fmt --all -- --check
git -c core.autocrlf=false diff --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --target aarch64-linux-android --lib -p thk-app -- -D warnings
cargo test --workspace -- --test-threads=1
cargo build --release -p thk-app
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-android-apk.ps1 -Profile release
```

- Mốc hoàn thiện toàn diện bản **`v1.0.1`** ngày **09/11/2026**: **`175 passed, 0 failed, 1 ignored`**; `cargo fmt --all -- --check` đạt `0 diff` toàn workspace; `git -c core.autocrlf=false diff --check` sạch; doc-tests đạt (`0 failed`), `cargo clippy --workspace --all-targets -- -D warnings` và `cargo clippy --target aarch64-linux-android --lib -p thk-app -- -D warnings` đều đạt `0 warnings`, `22` ảnh golden `egui_kittest` khớp `0 failed pixels`. Build Release Windows (`target/release/trihienkun-laucher.exe` & `Desktop\TriHienKun Laucher.exe`, **`50.118.144 byte`** / **`47.80 MB`**, `SHA-256 a9ee6a064239506db78e780239c89d9c505d9307a21930ab8e3a7b204bf6f684`, đang chạy tương tác trên Windows `Session 1` `PID 19616`) và Android `arm64-v8a` (`target/android/trihienkun-laucher-arm64-v8a-release.apk` & `Desktop\TriHienKun Laucher.apk`, **`52.478.673 byte`** / **`50.05 MB`**, `SHA-256 c59cfb2b8ef74e3b26849591f3c41d8d3c053c6e7bf9cf7e217f248ccc1682ae`, căn chỉnh trang nhớ `PT_LOAD Alignment: 16384` + `zipalign -c -v -P 16 4` đặt `lib/arm64-v8a/libthk_app.so` không nén tại offset `16875520` và `lib/arm64-v8a/libthk_jvm_shim.so` không nén tại offset `52461568`, ký `apksigner` v3 hợp lệ, cài đặt qua `adb -s PZ8T6HQWCIFUUKXK install -r` `Success` và khởi chạy trên Xiaomi `2602BRT18C` Android 17 SDK 37 `PID 20208` kèm phản chiếu `scrcpy` trên `Session 1` `PID 10204`) đều hoàn tất với `0 lỗi, 0 cảnh báo`; giới hạn kiểm chứng desktop và thiết bị vật lý được ghi ở mục 4–5.
- Test bị ignore yêu cầu mạng và phải chạy riêng theo mục 3. Tổng trên là mốc ghi nhận, không phải số lượng bắt buộc khi thêm test mới.

| Crate | Passed | Failed | Ignored | Nhóm kiểm thử tiêu biểu |
| :--- | ---: | ---: | ---: | :--- |
| `thk-app` | 70 | 0 | 1 | Golden `egui_kittest` (**22 ảnh `0 failed pixels`**, `threshold 0.0`, chủ đề mặc định **Engine Game Tươi Sáng `ThemeMode::Light`**), **Bố cục Thẻ `Mods` / `Addons` 3 tầng đáp ứng dưới `< 500pt` không chồng nút `Xóa`, Ảo hóa dòng theo khung nhìn `ui.clip_rect()` có nhân tỷ lệ `font_scale` cho cả Profile `Mods/Shaders/ResourcePacks` và Máy chủ `Plugins / Mods`, lọc từ khóa Unicode Tiếng Việt không phân biệt hoa thường, cuộn đồng nhất nội dung Profile và nút điều khiển nổi tròn `48pt`**, **Chống Trôi / Rung Khung Hình 60 FPS Giao diện `Máy chủ` & Thanh bên (`LeftNavRail` + `ServersSidebar`) ở trạng thái Rỗng và Có dữ liệu trên Ngang hai thanh bên (`1093×505`), Ngang gọn (`852×393`) và Dọc (`393×852`) dưới `pixels_per_point` lẻ `2.535` + `SafeAreaInsets`** (`servers_landscape_and_portrait_never_drift_or_jitter_across_frames`), **Quét Đệ quy Hộp Bao AccessKit Chống Tràn Ngang trên Toàn bộ 6 Thẻ `Máy chủ` (`Console`, `Files` + Editor, `PluginsMods`, `Properties`, `AccessControl`, `Network`) với đường dẫn `@libraries/.../unix_args.txt` không khoảng trắng & Hộp thoại kẹp vùng an toàn trên Dọc (`393×852`, `360×800`), Ngang (`852×393`), Desktop gọn (`720×640`) và phóng chữ `100% / 135% / 200%`** (`servers_long_modpack_paths_and_all_tabs_never_overflow_portrait_or_landscape`), **Quét Cỡ chữ Cực trị `50% – 200%` (`[0.50, 0.75, 1.00, 1.50, 2.00]`) trên Desktop (`1280×800`), Mobile Ngang (`924×426`), Mobile Dọc (`412×892`), mọi phân mục/tab & 5 hộp thoại không bể bố cục** (`font_scale_extremes_from_min_to_max_never_break_layout`), **Vùng An toàn Viền Màn hình Android `SafeAreaInsets` + Mặc định Tự xoay Tự do `FullSensor` + Di trú `mobile_free_orientation_v1` + Kẹp biên Phím Bedrock `(0,0)`/`(1,1)` & 4 góc nút điều khiển nổi tròn `48pt`** (`android_safe_area_insets_and_free_autorotate_keep_edge_buttons_clear`), **Bố cục Đáp ứng Mobile Dọc (`412×892`) & Ngang (`960×440`) + Ngăn kéo Bộ lọc Modpack + Xoay màn hình từ nút điều khiển nổi tròn `48pt`** (`visual_bright_engine_portrait_and_landscape_responsive`), **Đa cửa sổ Desktop (`Cửa sổ Đa nhiệm`, `Tách cửa sổ Nhật ký`, `Tách cửa sổ Console`)** (`desktop_multi_window_detached_panels_render_cleanly`), **Màn hình Run Game Mobile + Phím Bedrock 3×3 + Chuột Ảo + Edit Phím + nút điều khiển nổi tròn `48pt` bật/ẩn nhanh giao diện Launcher** (`mobile_game_screen_bedrock_controls_editor_and_floating_hub_toggle`), mở Port trực tiếp theo mạng đang kết nối (LAN / VPN / IPv6 Online / UPnP) & sao chép địa chỉ `[2001:db8::42]:25565` trên cả thẻ `Máy chủ → Mạng` và hộp thoại `Chia sẻ LAN` (`connected_network_direct_host_ui_and_copy_regression`), hiển thị CPU đa nền tảng & số lõi sử dụng (`server_cpu_telemetry_and_cores_fit_across_widths_and_locales`), Ping thực / TPS / MSPT + Quản trị Người chơi (`ops.json`, `banned-ips.json`, luật người chơi) + xoay màn hình di động `MobileOrientationMode` (`server_ping_tps_player_config_and_mobile_orientation_regression`), xoay vòng & đọc đuôi UTF-8 tệp nhật ký nội bộ `launcher.log` (`rolling_launcher_log_rotates_and_tails_clean_lines`), kéo giãn chuột các thanh bên & góc cửa sổ hộp thoại (`workspace_panels_and_modals_support_pointer_drag_resize`), hiển thị đơn cổng `bore.pub:<port>` + sao chép Clipboard `eframe/clipboard` + nút `Khởi động lại` máy chủ + gửi lệnh console bằng nút & phím `Enter` (`server_network_copy_restart_and_console_command_regression`), cuộn/AccessKit, lưu & áp dụng tỷ lệ cỡ chữ tự do `50%–200%` (`font_scale`), `pack_install` liên kết `fs::hard_link` có báo tiến trình (`commit_with_progress`) & ghi nguyên tử `installed_content`, nhận diện manifest `.mrpack`/`.zip`, khởi tạo máy chủ Modpack trực tiếp từ Cloud (`provision_cloud_modpack_server`), Chế độ Siêu tiết kiệm RAM (`trim_launcher_memory`) & cấu hình FTP/Modpack Server, đa tiến trình `active_tasks` + hủy, cổng EULA & hộp thoại xóa an toàn, phân loại `NoPublishedReleases` & Windows updater helper, live Modrinth (`ignored`) |
| `thk-mods` | 24 | 0 | 0 | Giao dịch `install_content_batch` + rollback/sharing violation, phân giải manifest CurseForge song song 16 luồng giữ đúng thứ tự & phát tiến trình (`concurrent_pack_resolution_preserves_order_and_emits_progress`), trích xuất phiên bản chuẩn Modpack/Mod (`extract_canonical_version_number`), khởi tạo Server từ 4 kho Cloud (`populate_server_from_cloud_modpack`), từ Modpack/Profile & lọc mod chỉ dành cho Client (`server_pack`), vân tay Murmur2 CurseForge, tự nhận diện loader Technic, lập kế hoạch/nhập CurseForge & hủy pending resolve, bật/tắt & xóa nội dung/server JAR |
| `thk-core` | 18 | 0 | 0 | Mặc định `MobileOrientationMode::FullSensor` & `safe_inset_{left,top,right,bottom}_px` trong `AndroidPermissionStatus`, bố cục phím cảm ứng Bedrock & làm sạch giới hạn tọa độ/kích thước `mobile_controls.json` (`mobile_control_layout_roundtrips_and_sanitizes`), `SafePath`, giải nén `.tar.xz` an toàn chống traversal & hủy sớm (`tarxz_roundtrip_and_traversal_and_precancel`), SQLite WAL + triggers dọn dẹp + di trú schema (`oauth_client_id`, `github_repo`, `low_ram_mode`, `java_runtime_mode`, `font_scale`, `mobile_orientation`, `mobile_keep_alive`, `mobile_renderer`), quét dữ liệu phục hồi, `flatten_single_subdir` tên có dấu chấm, đồng bộ `vi`/`en`, `DownloadManager` retry/backoff `429`/`503` + `force_overwrite` |
| `thk-server` | 17 | 0 | 0 | Đo CPU `/proc/<pid>/stat` & số lõi sử dụng không cần `/proc/stat` (`cpu_telemetry_computes_exact_percentage_and_used_cores_without_proc_stat`), rút gọn tên CPU Desktop & ARM SoC (`cpu_brand_summarizer_formats_desktop_and_heterogeneous_arm_socs`), phân tích TPS/MSPT từ Paper/Forge/Vanilla `tick query` (`tps_and_mspt_telemetry_parses_paper_forge_and_vanilla_tick_query`), quản trị `ops.json`, `banned-ips.json`, `usercache.json` & đặt lại dữ liệu `.dat` (`player_ops_ip_bans_and_playerdata_management_roundtrip`), Paper/Folia Fill v3 (`STABLE`, kích thước & SHA-256), SpongeVanilla sắp xếp ngữ nghĩa `RC10 > RC9`, nhận diện loader Arclight, truyền luồng triển khai FTP (`ftp_deploy`) & lọc tệp máy chủ, chặn Start trước khi chấp thuận `eula.txt` và khi cổng TCP đã bị chiếm (`start_rejects_port_already_bound_before_spawning_duplicate_jvm`), cờ tắt JLine cho ống dẫn `stdin` ẩn & lọc ANSI/NUL (`server_jvm_args_disable_headless_jline_and_strip_ansi_noise`), lọc lệnh console & chuẩn hóa `/`, quản lý tệp & `server.properties` |
| `thk-renderer3d` | 10 | 0 | 0 | Chọn mẫu khử răng cưa `resolve_offscreen_sample_count` (`sample_count = 1` trên `wgpu::Backend::Gl` OpenGL ES 3 để tránh lỗi `GL_FRAMEBUFFER_INCOMPLETE_MULTISAMPLE_EXT` trên Android, giữ nguyên `MSAA 4x` trên `Vulkan`/`Dx12`/`Metal`), ma trận chiếu phối cảnh `wgpu` `[0, 1]`, nhận diện UV `detect_skin_variant` (`Wide`/`Slim`/HD `Slim`), trích xuất Skin Steve `64×64` từ `client.jar`, Cape/Elytra bay phía sau lưng (`-Z`) & cánh đối xứng, cắt khung hình Cape động & tổng hợp UV Elytra `22x17`, chuyển đổi skin `64x32 → 64x64`, bệ đứng 3D hữu hạn |
| `thk-java` | 9 | 0 | 0 | Suy luận Java 8–25 (kể cả `26.x`), danh mục OpenJDK Bionic `aarch64` (`Java 8/17/21/25`) & phân giải `LD_LIBRARY_PATH` (`android_bionic_openjdk_catalog_and_ld_library_path_resolution`), ánh xạ chế độ tối ưu `optimal_graalvm_major_for_spec` (`GraalVM 17/21/25` & bảo toàn `Java 8` cho `<= 1.16.5`), truy vấn chọn lọc GraalVM theo major + `.sha256` sidecar & hủy cài đặt, ánh xạ 5 thành phần Mojang JRE + chặn thoát symlink, giải nén JRE qua staging (giữ bản cũ khi gói hỏng) |
| `thk-auth` | 8 | 0 | 0 | Offline UUID v3, PKCE `S256` & lọc `state`/`code`, Loopback PKCE chống callback lạ & giải phóng cổng qua `drop_guard`, chuẩn hóa Skin HD `128×128` / `64×32` sang `64×64` premultiplied-alpha không lem viền (`normalize_skin_png_bytes_for_minecraft`), Yggdrasil cục bộ ký `SHA1withRSA` + URL texture băm `SHA-256` không query (`SKIN`/`CAPE`/`ELYTRA`) + `prefetched` + tra cứu tên `/api/users/profiles/minecraft/{username}` & `/api/profiles/minecraft` + dự phòng `Slim` + `204 No Content` cho UUID lạ |
| `thk-network` | 10 | 0 | 0 | Cầu nối TCP `DirectPortHostHandle` hai chiều `TCP_NODELAY` & dọn dẹp sạch (`direct_port_host_bridges_tcp_traffic_and_stops_cleanly`), phân loại địa chỉ IPv4 LAN / VPN / CGNAT `100.64.0.0/10` / Global Unicast IPv6 `2000::/3` & khung gói LAN Multicast (`ip_classification_and_lan_frame_formatting_are_accurate`), phân tích XML thiết bị & phản hồi SOAP `GetExternalIPAddress` của UPnP IGD (`upnp_igd_xml_and_soap_parsers_handle_wan_services_and_relative_urls`), VarInt, phân tích IPv6 `[::1]:25565`, màu MOTD hex `§x`, cắt chuỗi UTF-8 Discord RPC `<= 128B`, định dạng đơn cổng `TunnelStatus::public_endpoint`, xác minh IP card mạng nội bộ & `start_lan_scan` |
| `thk-launcher` | 9 | 0 | 0 | Phân tách gói LWJGL3 `lwjgl-glfw`/`lwjgl-opengl`/`lwjgl-jemalloc` tương thích JPMS của Forge `1.19.2+` và xây dựng đặc tả khởi chạy Android `:game` LWJGL3 (`3.2.3`/`3.3.3`), bộ kết xuất `Holy-GL4ES` / `MobileGlues` & thay thế thư viện gốc ARM64 (`mobile_jvm_spec_builder_handles_renderers_lwjgl_and_library_upgrades`), quy tắc Mojang OS/arch (kể cả Linux `aarch64`), tiền tố NeoForge `1.21` / `26.x`, khử trùng lặp Maven classifier, thứ tự đối số `authlibinjector.yggdrasil.prefetched` + `-javaagent` + độ phân giải/fullscreen, `launch_async` qua `runtime::Handle`, `CrashAnalyzer` (`UpgradeLoader` & regex hậu tố `-rc1`) |
| **Tổng cộng** | **175** | **0** | **1** | Kèm doc-tests (`0 failed`), `cargo fmt --all -- --check` (`0 diff`), `cargo clippy --workspace --all-targets -- -D warnings` (`0 warnings`) và `cargo clippy --target aarch64-linux-android --lib -p thk-app -- -D warnings` (`0 warnings`) |
- Workflow `.github/workflows/release.yml` chạy `quality-gate` trên cả `ubuntu-24.04` và `windows-latest` (bao gồm `cargo fmt --check`, `cargo clippy -D warnings` và `cargo test --workspace -- --test-threads=1`). Runtime thực tế trên máy Linux/macOS vật lý chưa được xác minh trong phiên này.
- Khi tự động hóa build/test dài, khởi chạy nền và nhận thông báo hoàn tất; không dùng sleep hoặc vòng lặp chờ đồng bộ trong terminal.

## 2. Regression Cô Lập & Giao Diện

### Golden và tương tác egui

```bash
# Module hiện nằm dưới views::servers, dù kiểm tra nhiều màn hình
cargo test -p thk-app views::servers::visual_tests
cargo test -p thk-app servers_landscape_and_portrait_never_drift_or_jitter_across_frames
cargo test -p thk-app accounts_pkce_disclosure_handles_accesskit_actions
cargo test -p thk-app general_form_scrolls_to_visible_save_and_persists_isolated_profile
```

- Dùng `egui_kittest =0.37.0` với `wgpu`, `snapshot`, không dùng Playwright cho launcher native.
- Golden cố định Windows, Light/Emerald (chủ đề Engine Game Tươi Sáng mặc định, kèm kiểm thử song song Dark Cobalt), 1.0 pixel/point, font hệ thống được cài bởi ứng dụng và thời gian `1.0`. So khớp **threshold 0.0 / 0 failed pixels**; toàn bộ 22 ảnh golden hiện hành khớp tuyệt đối qua 3 lần chạy liên tiếp ở môi trường ghi nhận. Font/driver khác có thể làm ảnh khác; phải điều tra thay vì tự nới tolerance.
- Giữ `VISUAL_RENDER_LOCK`: renderer snapshot chạy đồng thời từng gây access violation trong `d3d10warp.dll` trên Windows.
- Bài cuộn/animation cho thời gian tiến triển bằng `harness.input_mut().time = None`. Navigation truy vấn named `Role::Button`, kiểm tra trạng thái `AriaCurrent::Page/False` và nút cần thao tác đã thực sự nằm trong viewport.
- Kiểm tra VI/EN, nhiều chiều rộng/scale cho General, Accounts, navigation và danh sách/lưới modpack. Scale trong harness **không thay thế** kiểm tra DPI Windows thật.
- PKCE kiểm Expand/Collapse lặp lại, Click, semantics/actions và không tràn ở 300/450/710/1000 points; không khởi tạo login. Tác vụ Accounts được tách bằng `SavedAccountActions`; lưu lựa chọn tài khoản/Profile dùng SQLite cô lập, không dùng tài khoản cá nhân làm fixture.
- Không thay golden để chấp nhận UI bị cắt, không ghi đè dữ liệu người dùng để tạo trạng thái test.

### Cài modpack và rollback

```bash
cargo test -p thk-app pack_install::tests
cargo test -p thk-app pack_rollback_sharing_violation_retains_payload_and_recovery_after_drop
```

Regression kiểm tra đích có dữ liệu, xung đột phát sinh muộn, thư mục rỗng thuộc Profile khác, hủy, ZIP traversal, lỗi SQLite và cleanup staging.

```bash
cargo test -p thk-core archive::tests
cargo test -p thk-mods curseforge::pack_tests
```

Regression bổ sung cho giải nén có token và manifest CurseForge: hủy trước khi tạo đích/mở archive; ZIP/filesystem thật kiểm traversal, ưu tiên `client-overrides`, hủy sau lần ghi đầu tiên của tệp lớn; giữ hash, từ chối tên không an toàn, không bỏ qua lỗi required file; hủy future resolve đang pending không dùng timer; nhập ZIP cô lập với overrides tùy chỉnh và bỏ qua optional. Kiểm lỗi resolve dùng kết quả lỗi tổng hợp trong hàm lập kế hoạch, không phải bằng chứng lỗi từ API CurseForge thật. Kiểm hủy giữa tệp chủ động hủy ở writer của bài test, không phải race trên một lượt tải provider thật.

Cả Trạm Cài Đặt trực tuyến lẫn hộp thoại **Nhập Modpack từ đĩa** đều nối `CancellationToken` vào worker giải nén (`extract_zip_prefix_cancellable`), kiểm giữa entry và trước/sau khối đọc tối đa 64 KiB, đồng thời lưu nguyên tử danh sách `installed_content` cùng Profile qua `upsert_profile_with_installed_content`; caller vẫn chờ worker kết thúc trước cleanup. Không suy ra việc abort/drop future sẽ dọn xong staging.

Regression sharing violation trên Windows khóa một tệp thật bằng `.share_mode(1)`, dựng trạng thái sau copy trong thư mục cô lập rồi gọi trực tiếp nhánh rollback. Bài test kiểm lỗi trả đường dẫn và giữ payload/`recovery.json` sau `Drop`; không dùng race/timer và **không phải** lỗi được gây giữa một lượt cài provider thật. Không suy ra có crash-atomic, khóa liên tiến trình hay tự phục hồi sau mất điện.

### Helper cập nhật Windows

```bash
cargo test -p thk-app updater::helper_tests
```

7 regression dùng PowerShell/filesystem thật trong môi trường cô lập: cache thay đổi, hash staging sai, staging/backup/lock đã tồn tại, thay executable lỗi, không khởi tạo được bản mới, rollback lỗi giữ backup và thay bằng executable thật giữ bản cũ. Dữ liệu/metadata tổng hợp được dùng để kiểm logic; đây **không phải** một lượt nâng cấp GitHub Release thật hoặc kiểm app mới chạy khỏe. Không thay executable launcher của người dùng trong test.

### Giao dịch cài đặt nội dung & các crate nền

```bash
cargo test -p thk-mods install
cargo test -p thk-core downloader::tests
cargo test -p thk-auth microsoft::loopback_tests
cargo test -p thk-server
```

- `thk-mods` (`install::tests` & `install_tests`): kiểm tra từ chối tên tệp trùng hoặc thuộc dự án khác, không ghi đè tệp không được quản lý, giữ trạng thái `.disabled` khi cập nhật, hoàn tác toàn bộ lô khi lỗi giữa chừng hoặc lỗi SQLite, giữ staging + `recovery.json` khi vướng sharing violation lúc rollback, và không xóa nhầm bản ghi của nhà cung cấp khác trùng `project_id`.
- `thk-core` (`downloader::tests` & `tests`): kiểm tra hủy luồng tải bị treo kèm dọn tệp `.part`, giữ nguyên tệp cũ khi lệch kích thước hoặc khi batch lỗi, di trú `github_repo` mặc định cũ mà không mất tùy chọn người dùng; trong `thk-app`, kiểm tra `app_settings` JSON hỏng dừng khởi tạo mà không ghi đè dữ liệu lưu trữ.
- `thk-auth`, `thk-java`, `thk-launcher`, `thk-network`, `thk-server`: kiểm tra callback Loopback PKCE lạ không tiêu thụ phiên đăng nhập và đóng listener qua `drop_guard`, phân tích phiên bản OpenJDK không ngoặc kép & ánh xạ `jre-legacy`/`alpha`/`gamma`/`delta`/`epsilon`, chạy `launch_async` và `start_lan_scan` bằng `tokio::runtime::Handle` tường minh, chọn build `STABLE` Paper/Folia Fill v3 và chặn `start_server` khi chưa chấp thuận `eula.txt`.

## 3. Live Modrinth — Chạy Riêng Khi Có Mạng

```bash
cargo test -p thk-app live_modrinth_pack_download_hashes_and_profile_commit -- --ignored --nocapture
```

Test liên hệ Modrinth thật, tải pack công khai cố định vào thư mục thử nghiệm và ghi SQLite riêng; tự dọn dữ liệu khi hoàn tất. Không dùng DB cá nhân và không khởi chạy Minecraft.

| Dữ liệu đã ghi nhận | Giá trị |
| :--- | :--- |
| Project | `Aa5L6RtV` |
| Version | `K3sPpfVz` |
| Archive SHA-1 | `1d011a5eee7e3adfd725fc954e8d17756e798160` |
| Tệp client được xác minh | 16 |
| Profile đã lưu | Minecraft `1.21.4`, Quilt `0.29.1` |

Archive và các tệp client được đối chiếu kích thước, SHA-1, SHA-512 theo metadata/index của nguồn. Test chạy lại sau sửa required-file/hủy giải nén đã đạt, vẫn xác minh 16 tệp và commit Profile vào SQLite cô lập. Hash ở đây kiểm khớp metadata, không chứng minh chữ ký nhà phát hành. Kết quả tải/cài không chứng minh game/modpack đã chạy hoặc mọi provider/phụ thuộc đều tương thích; mạng và nội dung nguồn có thể thay đổi.

## 4. Kiểm Chứng Desktop, Thiết Bị Android Vật Lý & Backend Thật

| Luồng | Kết quả đã quan sát | Giới hạn |
| :--- | :--- | :--- |
| Windows Desktop Release (`v1.0.1`) | `target/release/trihienkun-laucher.exe` & `Desktop\TriHienKun Laucher.exe` (`50.118.144 byte` / `47.80 MB`, `SHA-256 a9ee6a064239506db78e780239c89d9c505d9307a21930ab8e3a7b204bf6f684`) chạy tương tác trên Windows `Session 1` (`PID 19616`) | Không thay thế kiểm chứng runtime trên máy Linux/macOS vật lý |
| Android `arm64-v8a` Release (`v1.0.1`, Xiaomi `2602BRT18C`) | `target/android/trihienkun-laucher-arm64-v8a-release.apk` & `Desktop\TriHienKun Laucher.apk` (`52.478.673 byte` / `50.05 MB`, `SHA-256 c59cfb2b8ef74e3b26849591f3c41d8d3c053c6e7bf9cf7e217f248ccc1682ae`, `PT_LOAD 16384`, `lib/arm64-v8a/libthk_app.so` không nén tại offset `16875520`, `lib/arm64-v8a/libthk_jvm_shim.so` không nén tại offset `52461568`, `apksigner` v3) cài đặt qua `adb -s PZ8T6HQWCIFUUKXK install -r` (`Success`), chạy thực tế trên Android 17 SDK 37 (`PID 20208`, `SCREEN_ORIENTATION_FULL_SENSOR`, `0` lỗi `FATAL EXCEPTION`/`RustPanic`) kèm phản chiếu `scrcpy` trên `Session 1` (`PID 10204`) | Chưa kiểm hết mọi dòng thiết bị ARM64/ROM Android khác |
| Tiến trình `:game` `GameJvmService` (Xiaomi `2602BRT18C`) | Khởi chạy thực tế gói Modpack `Cisco's Fantasy Medieval RPG [Ultimate]` (`Forge 1.19.2 - 43.5.1`, 200+ mods, OpenJDK `17.0.19-internal`, gói `thk-mobile-runtime-1.1.8-arm64-v1` kèm Caciocavallo17 AWT headless + `MobileGlues 1.3.4` `OpenGL ES 3.2 -> OpenGL 4.0.0` + OpenAL Soft, khung `SurfaceView` `WindowManager.LayoutParams.TYPE_APPLICATION` + nút chuyển đổi tròn `42dp` `InAppSwitchIconView`) lên tới màn hình chính Minecraft `2772×1280` với `0` lỗi crash/tombstone | Chưa tự động hóa bước tạo/vào thế giới 3D bên trong modpack hoặc kiểm hết mọi phiên bản/mod loader trên Android |
| Minecraft 1.21.4 | Vào main menu, Quit exit 0 sau sửa runtime Play | Không phải kiểm mọi loader/modpack hoặc session Microsoft |
| LAN scan | Bật → dừng → bật lại không crash sau sửa đăng ký UDP vào runtime | Không chứng minh bạn bè kết nối được qua bore.pub |
| Paper 1.21.4 | Tải JAR qua Fill v3, kiểm kích thước/SHA-256; Start bị chặn trước EULA | Không chấp thuận EULA, chưa chạy server/kết nối client |
| PKCE disclosure | Windows UI Automation Expand/Collapse đều `delivered_verified`; Client ID/nút Start hiện/ẩn đúng; không tràn ngang ở 600×900, scale 1.0 | Không tạo phiên login |
| Updater | Kho GitHub chưa có Release trả `NoPublishedReleases` (không quăng lỗi `404`); lỗi mạng/kho không tồn tại vẫn được giữ nguyên | Chưa có lượt nâng cấp release thật |

Paper JAR được ghi nhận có **51.437.498 byte**, SHA-256 `5ee4f542f628a14c644410b08c94ea42e772ef4d29fe92973636b6813d4eaffc`. Thông báo Start: `Invalid input: Accept the Minecraft EULA before starting this server.`. Folia 1.21.4 không có build `STABLE` trong phản hồi nguồn đã kiểm; không tự coi đây là lỗi launcher.

Ảnh PKCE mở đã đọc/xác minh ở môi trường audit: `C:\Users\PhamTriHien\AppData\Local\Temp\opencode\thk-pkce-fixed-release-600-expanded.png`. Đây là bằng chứng cục bộ, không phải artifact được đóng gói trong repository. Không dùng ảnh collapsed cũ làm bằng chứng trạng thái mở.

Audit PKCE qua Windows UI Automation chạy trên bản Release trước đợt hoàn thiện cuối cùng; không gán kết quả kiểm UIA cụ thể đó cho bản mới nếu chưa chạy lại kịch bản UIA tương ứng. Bản Release **`v1.0.1`** ngày **09/11/2026** (`target/release/trihienkun-laucher.exe` & `Desktop\TriHienKun Laucher.exe`, `50.118.144 byte` / `47.80 MB`, `SHA-256 a9ee6a064239506db78e780239c89d9c505d9307a21930ab8e3a7b204bf6f684`) vượt qua toàn bộ **175 passed, 0 failed, 1 ignored** và đang chạy tương tác trên Windows `Session 1` (`PID 19616`).

## 5. Các Phần Chưa Xác Minh / Đang Bị Chặn

- Microsoft login tới Minecraft session và upload/reset skin thật: cần người dùng xác thực; không dùng tài khoản thật làm fixture tự động.
- Server sau chấp thuận EULA và kết nối client: cần quyết định chủ động của chủ máy chủ; không tự đặt `eula=true`.
- Khởi chạy modpack từ mọi provider và ma trận dependency/version conflicts đầy đủ chưa hoàn tất.
- Chưa kiểm thực tế một lượt tự nâng cấp từ GitHub Release đã phát hành; khi kho lưu trữ hợp lệ chưa có Release, launcher trả `NoPublishedReleases` thay vì lỗi `404`.
- Đã kiểm thực tế màn hình Windows DPI `150%` (`scale_factor = 1.5`, `1920×1008` trên `Session 1` `PID 14332`); các mức DPI `125%` / `200%` chưa kiểm riêng. Semantic scroll trả `ACTION_NOT_SUPPORTED` / `Scroll is not available`; không tự chuyển sang physical/headed input.
- Chưa kiểm crash/mất điện giữa filesystem và SQLite; modpack chưa khóa liên tiến trình/chống toàn bộ tác nhân ngoài thay path. Updater chưa health handshake hay tự phục hồi sau crash.
- Trên điện thoại vật lý Xiaomi `2602BRT18C` (`PZ8T6HQWCIFUUKXK`, `arm64-v8a`, Android 17 SDK 37, `1280×2772` / `2772×1280` với `DisplayCutout top=152px`, `RoundedCorners radius=156px`, `NAVIGATION_BAR=53px`), bản APK Release `v1.0.1` (`target/android/trihienkun-laucher-arm64-v8a-release.apk` & `Desktop\TriHienKun Laucher.apk`, `52.478.673 byte` / `50.05 MB`, `SHA-256 c59cfb2b8ef74e3b26849591f3c41d8d3c053c6e7bf9cf7e217f248ccc1682ae`) đã xác thực căn chỉnh `PT_LOAD Alignment: 16384` + `zipalign -c -v -P 16 4` (`lib/arm64-v8a/libthk_app.so` không nén tại offset `16875520`, `lib/arm64-v8a/libthk_jvm_shim.so` không nén tại offset `52461568`) + chữ ký `apksigner` v3, cài đặt trực tiếp qua `adb -s PZ8T6HQWCIFUUKXK install -r` (`Success`), khởi chạy ổn định (`PID 20208`, `mCurrentAppOrientation=SCREEN_ORIENTATION_FULL_SENSOR`, `SafeAreaInsets`, `ThkKeepAliveService`, kèm phản chiếu `scrcpy` trên Windows `Session 1` `PID 10204`) và khởi chạy thực tế gói Modpack `Cisco's Fantasy Medieval RPG [Ultimate]` (`Forge 1.19.2 - 43.5.1`, 200+ mods, OpenJDK `17.0.19-internal`, gói `thk-mobile-runtime-1.1.8-arm64-v1` kèm Caciocavallo17 AWT headless + `MobileGlues 1.3.4` `OpenGL ES 3.2 -> OpenGL 4.0.0` + OpenAL Soft, khung `SurfaceView` `WindowManager.LayoutParams.TYPE_APPLICATION` + nút chuyển đổi tròn `42dp` `InAppSwitchIconView`) bên trong tiến trình `:game` `GameJvmService` lên tới màn hình chính Minecraft `2772×1280` với `0` lỗi crash/tombstone. Chưa tự động hóa bước tạo/vào thế giới 3D bên trong modpack hoặc kiểm hết mọi phiên bản/mod loader trên Android.

## 6. Nguyên Tắc Ghi Nhận Kết Quả

Ghi rõ lệnh, hệ điều hành/toolchain, bản executable đã chạy, dữ liệu cô lập hay dịch vụ thật và điều kiện còn thiếu. Không công khai token, Device Code, URL redirect chứa `code`, dữ liệu DB cá nhân hoặc log chưa rà thông tin nhạy cảm. Không commit/push/tag/phát hành chỉ để kiểm updater nếu chưa được yêu cầu. Hành vi staging, recovery và quyền chấp thuận được mô tả thêm trong [ARCHITECTURE.md](ARCHITECTURE.md), [SECURITY.md](SECURITY.md) và [USER_GUIDE.md](USER_GUIDE.md).
