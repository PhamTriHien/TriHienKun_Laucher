use super::*;
use crate::state::HomeTab;
use egui_kittest::{
    kittest::{NodeT, Queryable},
    Harness, SnapshotOptions,
};
use thk_core::models::{AccentPreset, ServerEngine, ThemeMode};

// Concurrent snapshot renderers caused a native d3d10warp.dll access violation on Windows.
static VISUAL_RENDER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn harness_themed<'a>(
    width: f32,
    height: f32,
    theme: ThemeMode,
    render: impl FnMut(&mut egui::Ui) + 'a,
) -> Harness<'a> {
    let egui_theme = match theme {
        ThemeMode::Dark => egui::Theme::Dark,
        ThemeMode::Light => egui::Theme::Light,
    };
    let mut harness = Harness::builder()
        .with_size(vec2(width, height))
        .with_pixels_per_point(1.0)
        .with_theme(egui_theme)
        .with_os(egui::os::OperatingSystem::Windows)
        .with_options(SnapshotOptions::new().threshold(0.0).max_failed_pixels(0))
        .with_context_setup(move |ctx| {
            crate::theme::install_unicode_fonts(ctx);
            crate::theme::apply_theme(ctx, theme, AccentPreset::Emerald);
        })
        .build_ui(render);
    harness.input_mut().time = Some(1.0);
    harness.run_steps(3);
    harness
}

fn harness<'a>(width: f32, height: f32, render: impl FnMut(&mut egui::Ui) + 'a) -> Harness<'a> {
    harness_themed(width, height, ThemeMode::Light, render)
}

fn with_isolated_app(language: Language, test: impl FnOnce(&mut crate::state::LauncherAppState)) {
    let root = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap())
        .join("Temp/opencode")
        .join(format!("thk-scroll-test-{}", uuid::Uuid::new_v4()));
    let paths = thk_core::AppPaths::from_root(root.clone()).unwrap();
    let db = std::sync::Arc::new(thk_core::Database::open_in_memory().unwrap());
    let mut settings = db.load_settings().unwrap();
    settings.language = language;
    settings.auto_check_updates = false;
    settings.music_enabled = false;
    settings.discord_rpc_enabled = false;
    db.save_settings(&settings).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut app = crate::state::LauncherAppState::initialize(
        runtime.handle().clone(),
        egui::Context::default(),
        paths,
        db,
        None,
    )
    .unwrap();
    test(&mut app);
    app.audio.shutdown();
    drop(app);
    drop(runtime);
    std::fs::remove_dir_all(root).unwrap();
}

fn scroll_until_visible(harness: &mut Harness<'_>, label: &str, viewport: Rect) {
    // Snapshot time is fixed; wheel smoothing needs advancing frame time.
    harness.input_mut().time = None;
    assert!(harness
        .query_by_role_and_label(egui::Role::Button, label)
        .is_none_or(|node| !viewport.contains_rect(node.rect())));
    for _ in 0..30 {
        harness.input_mut().events.extend([
            egui::Event::PointerMoved(viewport.center()),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, -150.0),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        harness.run_steps(8);
        if harness
            .query_by_role_and_label(egui::Role::Button, label)
            .is_some_and(|node| viewport.contains_rect(node.rect()))
        {
            return;
        }
    }
    panic!("Action never became fully visible after scrolling: {label}; viewport={viewport:?}; action={:?}", harness.query_by_role_and_label(egui::Role::Button, label).map(|node| node.rect()));
}

#[test]
fn accounts_pkce_disclosure_handles_accesskit_actions() {
    for language in [Language::Vi, Language::En] {
        with_isolated_app(language, |app| {
            let title = app
                .i18n
                .tr(
                    "Nâng cao: Đăng nhập Loopback PKCE với Azure Client ID riêng",
                    "Advanced: Loopback PKCE Sign-In with Custom Azure Client ID",
                )
                .to_owned();
            let start = app
                .i18n
                .tr("Mở cổng Loopback PKCE", "Start Loopback PKCE")
                .to_owned();
            for width in [300.0, 450.0, 710.0, 1000.0] {
                let mut harness = harness(width, 1800.0, |ui| {
                    crate::views::accounts::render_accounts_view(app, ui, None);
                    assert!(ui.min_rect().right() <= width, "Accounts overflow: {width}");
                });
                harness.input_mut().time = None;
                for (action, expanded) in [
                    (egui::accesskit::Action::Expand, true),
                    (egui::accesskit::Action::Expand, true),
                    (egui::accesskit::Action::Collapse, false),
                    (egui::accesskit::Action::Collapse, false),
                    (egui::accesskit::Action::Click, true),
                    (egui::accesskit::Action::Click, false),
                ] {
                    let (target_node, target_tree) = harness
                        .get_by_role_and_label(egui::Role::DisclosureTriangle, &title)
                        .accesskit_node()
                        .locate();
                    harness
                        .input_mut()
                        .events
                        .push(egui::Event::AccessKitActionRequest(
                            egui::accesskit::ActionRequest {
                                action,
                                target_node,
                                target_tree,
                                data: None,
                            },
                        ));
                    harness.run_steps(12);
                    let header =
                        harness.get_by_role_and_label(egui::Role::DisclosureTriangle, &title);
                    for supported in [
                        egui::accesskit::Action::Expand,
                        egui::accesskit::Action::Collapse,
                        egui::accesskit::Action::Click,
                    ] {
                        assert!(header.accesskit_node().data().supports_action(supported));
                    }
                    assert_eq!(
                        header.accesskit_node().data().is_expanded(),
                        Some(expanded),
                        "{language:?}, {width}, {action:?}"
                    );
                    assert!(
                        header.rect().right() <= width,
                        "PKCE title overflow: {width}"
                    );
                    assert_eq!(
                        harness
                            .query_by_role_and_label(egui::Role::Button, &start)
                            .is_some(),
                        expanded
                    );
                }
            }
        });
    }
}

#[test]
fn navigation_scroll_reaches_visible_settings_and_opens_modal() {
    for language in [Language::Vi, Language::En] {
        with_isolated_app(language, |app| {
            let label = app.i18n.t("nav.settings").to_owned();
            let mut harness = harness(600.0, 250.0, |ui| {
                crate::views::render_left_nav_rail(
                    app,
                    ui,
                    crate::views::SafeAreaInsets::default(),
                );
            });
            scroll_until_visible(
                &mut harness,
                &label,
                Rect::from_min_size(pos2(0.0, 0.0), vec2(236.0, 250.0)),
            );
            harness
                .get_by_role_and_label(egui::Role::Button, &label)
                .click();
            harness.run_steps(3);
            drop(harness);
            assert!(app.show_settings_modal);
        });
    }
}

#[test]
fn accounts_scroll_reaches_visible_selection_and_persists_isolated_account() {
    use thk_core::models::{Account, AccountType, SkinVariant};

    for language in [Language::Vi, Language::En] {
        for width in [300.0, 450.0, 710.0, 1000.0] {
            with_isolated_app(language, |app| {
                // A synthetic offline account in an isolated database; no stored credentials.
                let account = Account {
                    id: "scroll-account".into(),
                    username: "ScrollTestPlayer".into(),
                    uuid: "00000000-0000-0000-0000-000000000000".into(),
                    account_type: AccountType::Offline,
                    skin_variant: SkinVariant::Wide,
                    skin_url: None,
                    cape_url: None,
                    custom_skin_path: None,
                    custom_cape_path: None,
                    custom_elytra_path: None,
                    token_expires_at: None,
                    oauth_client_id: None,
                    created_at: 0,
                };
                app.db.upsert_account(&account).unwrap();
                app.accounts.accounts = vec![account];
                app.accounts.selected_account_id = None;
                let label = app.i18n.tr("Chọn", "Select").to_owned();
                let mut harness = harness(width, 250.0, |ui| {
                    crate::views::accounts::render_accounts_view(app, ui, None);
                    assert!(
                        ui.min_rect().right() <= width,
                        "accounts overflow at {width}"
                    );
                });
                scroll_until_visible(
                    &mut harness,
                    &label,
                    Rect::from_min_size(pos2(0.0, 0.0), vec2(width, 250.0)),
                );
                harness
                    .get_by_role_and_label(egui::Role::Button, &label)
                    .click();
                harness.run_steps(3);
                drop(harness);
                assert_eq!(
                    app.accounts.selected_account_id.as_deref(),
                    Some("scroll-account")
                );
                assert_eq!(
                    app.db.get_kv("selected_account_id").unwrap().as_deref(),
                    Some("scroll-account")
                );
                assert_eq!(app.db.list_accounts().unwrap().len(), 1);
            });
        }
    }
}

#[test]
fn navigation_cards_expose_named_buttons_and_activate() {
    for language in [Language::Vi, Language::En] {
        let i18n = I18n::new(language);
        for section in [
            crate::state::ActiveSection::Home,
            crate::state::ActiveSection::Modpacks,
            crate::state::ActiveSection::Servers,
            crate::state::ActiveSection::Accounts,
        ] {
            let key = match section {
                crate::state::ActiveSection::Home => "nav.home",
                crate::state::ActiveSection::Modpacks => "nav.modpacks",
                crate::state::ActiveSection::Servers => "nav.servers",
                crate::state::ActiveSection::Accounts => "nav.accounts",
            };
            let label = i18n.t(key);
            for selected in [false, true] {
                let activations = std::cell::Cell::new(0);
                let mut harness = harness(236.0, 120.0, |ui| {
                    if crate::views::section_navigation_card(
                        ui,
                        section,
                        label,
                        crate::fx3d::Icon3dKind::HomePortal,
                        crate::theme::ACCENT_EMERALD,
                        selected,
                    )
                    .clicked()
                    {
                        activations.set(activations.get() + 1);
                    }
                });
                let button = harness.get_by_role(egui::Role::Button);
                let node = button.accesskit_node();
                assert_eq!(node.label().as_deref(), Some(label));
                assert_eq!(node.toggled(), None);
                assert_eq!(
                    node.aria_current(),
                    Some(if selected {
                        egui::accesskit::AriaCurrent::Page
                    } else {
                        egui::accesskit::AriaCurrent::False
                    })
                );
                assert_eq!(node.is_selected(), None);
                assert!(node.data().supports_action(egui::accesskit::Action::Click));
                button.click();
                harness.run_steps(3);
                assert_eq!(activations.get(), 1, "pointer navigation: {label}");
                harness.get_by_role(egui::Role::Button).click_accesskit();
                harness.run_steps(3);
                assert_eq!(activations.get(), 2, "accessible navigation: {label}");
            }
        }
    }
}

#[test]
fn saved_accounts_fit_and_actions_activate_without_side_effects() {
    use crate::views::accounts::saved_account_card_ui;
    use thk_core::models::{Account, AccountType, SkinVariant};

    // Synthetic account used only to exercise rendering; no credentials or database access.
    let mut account = Account {
        id: "layout-account".into(),
        username: "LayoutTestPlayer".into(),
        uuid: "00000000-0000-0000-0000-000000000000".into(),
        account_type: AccountType::Offline,
        skin_variant: SkinVariant::Wide,
        skin_url: None,
        cape_url: None,
        custom_skin_path: None,
        custom_cape_path: Some("layout-only-cape.png".into()),
        custom_elytra_path: None,
        token_expires_at: None,
        oauth_client_id: None,
        created_at: 0,
    };
    for language in [Language::Vi, Language::En] {
        let i18n = I18n::new(language);
        for account_type in [AccountType::Offline, AccountType::Microsoft] {
            account.account_type = account_type;
            for selected in [false, true] {
                for width in [300.0, 450.0, 710.0] {
                    let select = std::cell::Cell::new(0);
                    let delete = std::cell::Cell::new(0);
                    let refresh = std::cell::Cell::new(0);
                    let mut harness = harness(width, 500.0, |ui| {
                        let actions = saved_account_card_ui(ui, &account, selected, &i18n);
                        select.set(select.get() + usize::from(actions.select));
                        delete.set(delete.get() + usize::from(actions.delete));
                        refresh.set(refresh.get() + usize::from(actions.refresh));
                        assert!(
                            ui.min_rect().right() <= width,
                            "account exceeds {width}: {:?}",
                            ui.min_rect()
                        );
                    });
                    let provider_label = match account_type {
                        AccountType::Microsoft => {
                            i18n.tr("Microsoft Bản quyền", "Microsoft Official")
                        }
                        AccountType::Offline => "Offline + Yggdrasil Skin",
                    };
                    for text in ["wide", "Cape", provider_label] {
                        let rect = harness.get_by_label(text).rect();
                        assert!(rect.right() <= width);
                        assert!(rect.height() < 30.0, "account badge wraps: {text}");
                    }
                    let label = if selected {
                        i18n.t("common.delete")
                    } else {
                        i18n.tr("Chọn", "Select")
                    };
                    let action = harness.get_by_label(label);
                    assert!(action.rect().right() <= width);
                    action.click();
                    harness.run_steps(3);
                    if selected {
                        assert_eq!(delete.get(), 1);
                    } else {
                        assert_eq!(select.get(), 1);
                    }
                    if selected && account_type == AccountType::Microsoft {
                        let sync = harness
                            .get_by_label(i18n.tr("Đồng bộ Token & Skin", "Sync Token & Skin"));
                        assert!(sync.rect().right() <= width);
                        assert!(!sync.rect().intersects(harness.get_by_label(label).rect()));
                        sync.click_accesskit();
                        harness.run_steps(3);
                        assert_eq!(refresh.get(), 1);
                    }
                    harness
                        .get_by_role(egui::Role::RadioButton)
                        .click_accesskit();
                    harness.run_steps(3);
                    assert_eq!(select.get(), if selected { 1 } else { 2 });
                }
            }
        }
    }
}

// Deliberately synthetic UI fixture; never read the user's database or start a server.
fn server_fixture() -> ManagedServer {
    ManagedServer {
        id: "visual-test-server".into(),
        name: "Visual Test SMP".into(),
        engine: ServerEngine::Paper,
        game_version: "1.21.4".into(),
        server_dir: std::path::PathBuf::from("visual-test-not-a-real-server"),
        is_custom_dir: false,
        port: 25565,
        ram_min_mb: 1024,
        ram_max_mb: 4096,
        java_path: None,
        jvm_flags: String::new(),
        jar_file: "paper-1.21.4.jar".into(),
        created_at: 0,
    }
}

#[test]
fn visual_servers_header_and_command() {
    let _render_guard = VISUAL_RENDER_LOCK.lock().expect("visual renderer lock");
    let mut snapshots = egui_kittest::SnapshotResults::new();
    for (language, locale) in [(Language::Vi, "vi"), (Language::En, "en")] {
        for (width, size) in [(450.0, "minimum"), (710.0, "default")] {
            for (runtime, status) in [
                (ServerRuntimeState::Offline, "offline"),
                (ServerRuntimeState::Online, "online"),
            ] {
                let i18n = I18n::new(language);
                let server = server_fixture();
                let mut input = String::new();
                let mut harness = harness(width, 330.0, |ui| {
                    server_header_ui(ui, &server, runtime, &ServerResourceStats::default(), &i18n);
                    ui.add_space(12.0);
                    server_command_ui(ui, &mut input, &i18n);
                    assert!(ui.min_rect().right() <= ui.max_rect().right() + 0.5);
                    assert!(ui.min_rect().bottom() <= ui.max_rect().bottom() + 0.5);
                });
                let clear = harness
                    .get_by_label(i18n.tr("Xoá màn hình", "Clear"))
                    .rect();
                let send = harness.get_by_label(i18n.tr("Gửi lệnh", "Send")).rect();
                assert!(clear.right() <= width);
                assert!(clear.bottom() <= 330.0);
                assert!(!clear.intersects(send));
                let port = harness.get_by_label("Port 25565").rect();
                assert!(port.right() <= width);
                assert!(port.height() < 30.0);
                let name = harness.get_by_label("Visual Test SMP").rect();
                let folder = harness.get_by_label(i18n.tr("Thư mục", "Folder")).rect();
                assert!(folder.top() >= name.bottom());
                harness.snapshot(format!("servers_{locale}_{size}_{status}"));
                snapshots.extend(harness.take_snapshot_results());
            }
        }
    }
}

#[test]
fn visual_profile_tabs() {
    let _render_guard = VISUAL_RENDER_LOCK.lock().expect("visual renderer lock");
    let mut snapshots = egui_kittest::SnapshotResults::new();
    for (language, locale) in [(Language::Vi, "vi"), (Language::En, "en")] {
        for (width, size) in [(450.0, "minimum"), (710.0, "default")] {
            let i18n = I18n::new(language);
            let mut current = HomeTab::General;
            let mut harness = harness(width, 150.0, |ui| {
                if let Some(next) = crate::views::home::profile_tabs_ui(ui, &i18n, current) {
                    current = next;
                }
                assert!(ui.min_rect().right() <= ui.max_rect().right() + 0.5);
            });
            harness.snapshot(format!("profile_tabs_{locale}_{size}"));
            snapshots.extend(harness.take_snapshot_results());
            harness.get_by_label(i18n.t("home.tab.logs")).click();
            harness.run_steps(3);
            drop(harness);
            assert_eq!(current, HomeTab::Logs);
        }
    }
}

#[test]
fn visual_profile_hero() {
    let _render_guard = VISUAL_RENDER_LOCK.lock().expect("visual renderer lock");
    use crate::views::home::{profile_hero_ui, ProfileHeroUi};
    use thk_core::models::{GameProfile, LoaderType};

    // Synthetic profile and preview geometry; no personal database or 3D renderer.
    let profile = GameProfile {
        id: "visual-profile".into(),
        name: "Visual Test Profile".into(),
        game_version: "1.21.4".into(),
        loader: LoaderType::Vanilla,
        loader_version: None,
        instance_path: "visual-test-not-a-real-profile".into(),
        is_custom_path: false,
        ram_mb: 4096,
        java_path: None,
        jvm_args: String::new(),
        auto_perf_mods: false,
        icon_url: None,
        window_width: Some(1280),
        window_height: Some(720),
        fullscreen: false,
        total_play_seconds: 456,
        session_count: 2,
        crash_count: 0,
        last_played_at: None,
        created_at: 0,
    };
    let mut snapshots = egui_kittest::SnapshotResults::new();
    for (language, locale) in [(Language::Vi, "vi"), (Language::En, "en")] {
        for (width, size) in [(450.0, "minimum"), (710.0, "default")] {
            for is_running in [false, true] {
                let i18n = I18n::new(language);
                let mut activated = false;
                let mut harness = harness(width, 340.0, |ui| {
                    let actions = profile_hero_ui(
                        ui,
                        &ProfileHeroUi {
                            profile: &profile,
                            i18n: &i18n,
                            icon: None,
                            accounts: &[],
                            selected_account_id: None,
                            is_running,
                            live_instances: &[],
                        },
                        |ui| {
                            ui.allocate_exact_size(vec2(142.0, 154.0), egui::Sense::hover());
                            ui.add_space(8.0);
                        },
                    );
                    activated |= actions.launch_or_stop;
                    assert!(
                        ui.min_rect().right() <= width,
                        "hero bounds {:?}, width {width}",
                        ui.min_rect()
                    );
                    assert!(ui.min_rect().bottom() <= 340.0);
                });
                let action_label = i18n.t(if is_running { "home.stop" } else { "home.play" });
                let button = harness.get_by_label(action_label).rect();
                assert!(button.right() <= width);
                assert!(button.bottom() <= 340.0);
                for text in ["MC 1.21.4", "Vanilla", "4096 MB RAM", "1280×720", "7m 36s"] {
                    let badge = harness.get_by_label(text).rect();
                    assert!(badge.right() <= width, "{text} exceeds {width}");
                    assert!(badge.height() < 30.0, "{text} wraps vertically");
                    assert!(
                        button.top() >= badge.bottom(),
                        "{text} overlaps launch/stop"
                    );
                }
                let folder = harness.get_by_label(i18n.t("home.open_folder")).rect();
                assert!(button.top() >= folder.bottom());
                let status = if is_running { "running" } else { "idle" };
                harness.snapshot(format!("profile_hero_{locale}_{size}_{status}"));
                snapshots.extend(harness.take_snapshot_results());
                harness.get_by_label(action_label).click();
                harness.run_steps(3);
                drop(harness);
                assert!(activated);
            }
        }
    }
}

fn project_fixture() -> thk_mods::ProjectSummary {
    use thk_core::models::{ContentKind, ProviderSource};
    // Synthetic layout fixture, not a claim about a real catalog project.
    thk_mods::ProjectSummary {
        provider: ProviderSource::Modrinth,
        project_id: "layout-test".into(),
        slug: "layout-test".into(),
        title: "Layout Test Modpack With A Long Title".into(),
        summary: "A deliberately long test description to exercise wrapping within the result card, without moving any actions beyond its bounds.".into(),
        author: "Layout Test Author".into(),
        downloads: 1234567,
        icon_url: None,
        categories: vec!["adventure".into(), "fabric".into(), "optimization".into(), "multiplayer".into()],
        updated_at: Some("2026-11-06T00:00:00Z".into()),
        content_kind: ContentKind::Modpack,
    }
}

#[test]
fn project_list_badges_and_actions_fit_at_multiple_scales() {
    let hit = project_fixture();
    for language in [Language::Vi, Language::En] {
        let i18n = I18n::new(language);
        for scale in [1.0, 1.25, 1.5] {
            for physical_width in [450.0, 710.0] {
                let width = physical_width / scale;
                let mut activated = false;
                let mut harness = harness(width, 700.0, |ui| {
                    let (install, _) = crate::views::modpacks::project_list_card_ui(
                        ui,
                        &hit,
                        None,
                        crate::theme::ACCENT_EMERALD,
                        &i18n,
                    );
                    activated |= install;
                    assert!(
                        ui.min_rect().right() <= width,
                        "list exceeds {width}: {:?}",
                        ui.min_rect()
                    );
                });
                for label in ["adventure", "fabric", "optimization", "multiplayer"] {
                    let rect = harness.get_by_label(label).rect();
                    assert!(rect.height() < 25.0, "badge {label} wraps: {rect:?}");
                    assert!(rect.right() <= width);
                }
                let install = harness.get_by_label(i18n.tr("Cài đặt", "Install")).rect();
                let detail = harness.get_by_label(i18n.tr("Chi tiết", "Details")).rect();
                assert!(!install.intersects(detail));
                assert!(install.right() <= width && detail.right() <= width);
                harness.get_by_label(i18n.tr("Cài đặt", "Install")).click();
                harness.run_steps(3);
                drop(harness);
                assert!(activated);
            }
        }
    }
}

#[test]
fn project_grid_badges_and_install_fit_narrow_columns() {
    let hit = project_fixture();
    for language in [Language::Vi, Language::En] {
        let i18n = I18n::new(language);
        for width in [240.0, 265.0, 310.0, 450.0] {
            let mut activated = false;
            let mut harness = harness(width, 700.0, |ui| {
                let card_width = ui.available_width();
                let (install, _) = crate::views::modpacks::project_grid_card_ui(
                    ui,
                    &hit,
                    None,
                    card_width,
                    crate::theme::ACCENT_EMERALD,
                    &i18n,
                );
                activated |= install;
                assert!(
                    ui.min_rect().right() <= width,
                    "grid exceeds {width}: {:?}",
                    ui.min_rect()
                );
            });
            for label in ["adventure", "fabric"] {
                let rect = harness.get_by_label(label).rect();
                assert!(rect.height() < 25.0);
                assert!(rect.right() <= width);
            }
            harness.get_by_label(i18n.tr("Cài", "Install")).click();
            harness.run_steps(3);
            drop(harness);
            assert!(activated);
        }
    }
}

#[test]
fn workspace_panels_and_modals_support_pointer_drag_resize() {
    with_isolated_app(Language::Vi, |app| {
        app.settings.setup_completed = true;
        let mut srv = server_fixture();
        srv.name = "Cisco's Fantasy Medieval RPG [Ultimate] Server".into();
        app.servers.selected_server_id = Some(srv.id.clone());
        app.servers.servers = vec![srv];

        let create_btn_label = app.i18n.t("server.create").to_owned();
        let mut panel_harness = harness(1240.0, 780.0, |ui| {
            crate::views::servers::render_servers_view(app, ui);
            crate::views::modals::render_all_modals(app, ui.ctx());
        });
        panel_harness.input_mut().time = None;

        let init_btn_rect = panel_harness
            .get_by_role_and_label(egui::Role::Button, &create_btn_label)
            .rect();
        assert!(init_btn_rect.right() <= 262.0);

        let start_divider = pos2(262.0, 320.0);
        let target_divider = pos2(520.0, 320.0);
        panel_harness
            .input_mut()
            .events
            .push(egui::Event::PointerMoved(start_divider));
        panel_harness.run_steps(2);
        panel_harness
            .input_mut()
            .events
            .push(egui::Event::PointerButton {
                pos: start_divider,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            });
        panel_harness.run_steps(2);
        panel_harness
            .input_mut()
            .events
            .push(egui::Event::PointerMoved(target_divider));
        panel_harness.run_steps(4);
        panel_harness
            .input_mut()
            .events
            .push(egui::Event::PointerButton {
                pos: target_divider,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            });
        panel_harness.run_steps(3);

        let stretched_btn_rect = panel_harness
            .get_by_role_and_label(egui::Role::Button, &create_btn_label)
            .rect();
        assert!(
            stretched_btn_rect.right() > 480.0,
            "sidebar failed to stretch past 350pt ceiling: {stretched_btn_rect:?}"
        );
        drop(panel_harness);

        app.show_new_server_modal = true;
        let win_title = app
            .i18n
            .tr(
                "Tạo Máy Chủ Minecraft Cục Bộ",
                "Create Local Minecraft Server",
            )
            .to_owned();
        let cancel_lbl = app.i18n.t("common.cancel").to_owned();
        let mut modal_harness = Harness::builder()
            .with_size(vec2(1240.0, 860.0))
            .with_pixels_per_point(1.0)
            .with_theme(egui::Theme::Dark)
            .with_os(egui::os::OperatingSystem::Windows)
            .with_accessibility_check(false)
            .with_context_setup(|ctx| {
                crate::theme::install_unicode_fonts(ctx);
                crate::theme::apply_theme(ctx, ThemeMode::Dark, AccentPreset::Emerald);
            })
            .build_ui(|ui| {
                crate::views::modals::render_all_modals(app, ui.ctx());
            });
        modal_harness.run_steps(3);
        modal_harness.input_mut().time = None;

        let modal_rect_init = modal_harness
            .get_by_role_and_label(egui::Role::Window, &win_title)
            .rect();
        let corner_start = modal_harness
            .get_by_role_and_label(egui::Role::Splitter, "Resize window bottom-right corner")
            .rect()
            .center();
        let corner_end = corner_start + vec2(150.0, 100.0);
        modal_harness
            .input_mut()
            .events
            .push(egui::Event::PointerMoved(corner_start));
        modal_harness.run_steps(2);
        modal_harness
            .input_mut()
            .events
            .push(egui::Event::PointerButton {
                pos: corner_start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            });
        modal_harness.run_steps(2);
        modal_harness
            .input_mut()
            .events
            .push(egui::Event::PointerMoved(corner_end));
        modal_harness.run_steps(4);
        modal_harness
            .input_mut()
            .events
            .push(egui::Event::PointerButton {
                pos: corner_end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            });
        modal_harness.run_steps(3);

        let modal_rect_after = modal_harness
            .get_by_role_and_label(egui::Role::Window, &win_title)
            .rect();
        assert!(
            modal_rect_after.width() >= modal_rect_init.width() + 140.0
                && modal_rect_after.height() >= modal_rect_init.height() + 90.0,
            "modal did not resize on both axes: before={modal_rect_init:?}, after={modal_rect_after:?}"
        );
        let cancel_rect_after = modal_harness
            .get_by_role_and_label(egui::Role::Button, &cancel_lbl)
            .rect();
        assert!(
            modal_rect_after.contains_rect(cancel_rect_after),
            "cancel button escaped resized modal: modal={modal_rect_after:?}, cancel={cancel_rect_after:?}"
        );
    });
}

#[test]
fn server_network_copy_restart_and_console_command_regression() {
    with_isolated_app(Language::Vi, |app| {
        app.settings.setup_completed = true;
        let srv = server_fixture();
        let srv_id = srv.id.clone();
        app.servers.selected_server_id = Some(srv_id.clone());
        app.servers.servers = vec![srv.clone()];
        app.servers.active_tab = ServerTab::Network;
        app.servers
            .server_states
            .insert(srv_id.clone(), ServerRuntimeState::Starting);
        app.servers.server_tunnels.insert(
            srv_id,
            TunnelStatus::Running {
                local_port: 25565,
                remote_addr: "bore.pub:25435".into(),
                remote_port: 25435,
            },
        );

        let restart_lbl = app.i18n.t("server.restart").to_owned();
        let copy_lbl = app.i18n.tr("Sao chép địa chỉ", "Copy Address").to_owned();

        let mut net_harness = harness(1240.0, 820.0, |ui| {
            crate::views::servers::render_servers_view(app, ui);
        });
        net_harness.input_mut().time = None;

        let endpoint_rect = net_harness.get_by_label("bore.pub:25435").rect();
        assert!(endpoint_rect.width() > 20.0);
        assert!(
            net_harness.query_by_label("bore.pub:25435:25435").is_none(),
            "tunnel endpoint must never duplicate port"
        );

        let restart_rect = net_harness
            .get_by_role_and_label(egui::Role::Button, &restart_lbl)
            .rect();
        assert!(restart_rect.width() > 20.0);

        let copy_pos = net_harness
            .get_by_role_and_label(egui::Role::Button, &copy_lbl)
            .rect()
            .center();
        net_harness
            .input_mut()
            .events
            .push(egui::Event::PointerMoved(copy_pos));
        net_harness.run_steps(1);
        net_harness
            .input_mut()
            .events
            .push(egui::Event::PointerButton {
                pos: copy_pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            });
        net_harness.step();
        net_harness
            .input_mut()
            .events
            .push(egui::Event::PointerButton {
                pos: copy_pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            });
        net_harness.step();
        let copied =
            net_harness.output().platform_output.commands.iter().any(
                |cmd| matches!(cmd, egui::OutputCommand::CopyText(t) if t == "bore.pub:25435"),
            );
        drop(net_harness);

        assert!(copied, "Copy Address button must emit CopyText command");
        assert!(
            app.toasts.iter().any(|t| t.text.contains("bore.pub:25435")),
            "Copy Address button must push confirmation toast"
        );
    });

    let i18n = I18n::new(Language::Vi);
    let mut cmd_text = "/list".to_string();
    let submitted_count = std::cell::Cell::new(0usize);
    let send_lbl = i18n.tr("Gửi lệnh", "Send").to_owned();
    let mut cmd_harness = harness(700.0, 140.0, |ui| {
        let (send, _) = server_command_ui(ui, &mut cmd_text, &i18n);
        if send {
            submitted_count.set(submitted_count.get() + 1);
        }
    });
    cmd_harness.get_by_label(&send_lbl).click();
    cmd_harness.run_steps(2);
    assert_eq!(
        submitted_count.get(),
        1,
        "Send button must trigger command submission and keep focus"
    );
    cmd_harness.key_press(egui::Key::Enter);
    cmd_harness.run_steps(2);
    drop(cmd_harness);
    assert_eq!(
        submitted_count.get(),
        2,
        "Enter key must also trigger command submission"
    );
}

#[test]
fn server_cpu_telemetry_and_cores_fit_across_widths_and_locales() {
    let _render_guard = VISUAL_RENDER_LOCK.lock().expect("visual renderer lock");
    let server = server_fixture();
    let stats = ServerResourceStats {
        cpu_percent: 37.5,
        used_cores: 3.0,
        logical_cores: 8,
        allowed_cores: 8,
        cpu_model: "MediaTek MT6991".into(),
        ram_mb: 1536,
        uptime_secs: 725,
        online_players: 3,
        max_players: 20,
        ..ServerResourceStats::default()
    };

    for (language, cores_word) in [(Language::Vi, "lõi"), (Language::En, "cores")] {
        let i18n = I18n::new(language);
        let low_load = ServerResourceStats {
            cpu_percent: 0.6,
            used_cores: 0.05,
            logical_cores: 8,
            allowed_cores: 6,
            cpu_model: "AMD Ryzen 7 7840HS".into(),
            ..ServerResourceStats::default()
        };
        assert_eq!(
            format_cpu_usage_label(&low_load, &i18n),
            format!("CPU 0.6% (0.05/8 {cores_word})")
        );

        for width in [450.0, 710.0] {
            let mut input_off = String::new();
            let offline_harness = harness(width, 330.0, |ui| {
                server_header_ui(ui, &server, ServerRuntimeState::Offline, &stats, &i18n);
                ui.add_space(12.0);
                server_command_ui(ui, &mut input_off, &i18n);
                assert!(ui.min_rect().right() <= ui.max_rect().right() + 0.5);
                assert!(ui.min_rect().bottom() <= ui.max_rect().bottom() + 0.5);
            });
            let cpu_off_rect = offline_harness.get_by_label("CPU: MediaTek MT6991").rect();
            let cores_off_lbl = format!("8 {cores_word}");
            let cores_off_rect = offline_harness.get_by_label(&cores_off_lbl).rect();
            assert!(cpu_off_rect.right() <= width + 0.5);
            assert!(cores_off_rect.right() <= width + 0.5);
            drop(offline_harness);

            let mut input_on = String::new();
            let online_harness = harness(width, 330.0, |ui| {
                server_header_ui(ui, &server, ServerRuntimeState::Online, &stats, &i18n);
                ui.add_space(12.0);
                server_command_ui(ui, &mut input_on, &i18n);
                assert!(ui.min_rect().right() <= ui.max_rect().right() + 0.5);
                assert!(ui.min_rect().bottom() <= ui.max_rect().bottom() + 0.5);
            });
            let soc_badge_rect = online_harness.get_by_label("MediaTek MT6991").rect();
            let cpu_on_lbl = format!("CPU 37.5% (3.0/8 {cores_word})");
            let cpu_on_rect = online_harness.get_by_label(&cpu_on_lbl).rect();
            assert!(soc_badge_rect.right() <= width + 0.5);
            assert!(cpu_on_rect.right() <= width + 0.5);
            drop(online_harness);
        }
    }

    let desktop_cpu = CpuHardwareInfo {
        model_name: "AMD Ryzen 7 7840HS w/ Radeon 780M Graphics".into(),
        short_model: "AMD Ryzen 7 7840HS".into(),
        logical_cores: 16,
        physical_cores: Some(8),
    };
    let affinity_stats = ServerResourceStats {
        cpu_percent: 25.0,
        used_cores: 4.0,
        logical_cores: 16,
        allowed_cores: 12,
        cpu_model: "AMD Ryzen 7 7840HS".into(),
        ..ServerResourceStats::default()
    };
    let i18n_vi = I18n::new(Language::Vi);
    let row_harness = harness(760.0, 120.0, |ui| {
        egui::Grid::new("test_cpu_row_grid")
            .num_columns(2)
            .show(ui, |ui| {
                server_system_cpu_row_ui(
                    ui,
                    &desktop_cpu,
                    ServerRuntimeState::Online,
                    Some(&affinity_stats),
                    &i18n_vi,
                );
                ui.end_row();
            });
    });
    assert!(row_harness.get_by_label("CPU hệ thống:").rect().width() > 20.0);
    assert!(
        row_harness
            .get_by_label("AMD Ryzen 7 7840HS w/ Radeon 780M Graphics")
            .rect()
            .width()
            > 40.0
    );
    assert!(row_harness.get_by_label("8 nhân / 16 luồng").rect().width() > 20.0);
    assert!(
        row_harness
            .get_by_label("CPU 25.0% (4.0/16 lõi)")
            .rect()
            .width()
            > 20.0
    );
    assert!(
        row_harness
            .get_by_label("Lõi cấp phép: 12/16")
            .rect()
            .width()
            > 20.0
    );
    drop(row_harness);
}

#[test]
fn server_ping_tps_player_config_and_mobile_orientation_regression() {
    use thk_core::models::MobileOrientationMode;
    use thk_server::{KnownPlayerEntry, OpEntry};

    let _render_guard = VISUAL_RENDER_LOCK.lock().expect("visual renderer lock");
    let server = server_fixture();
    let live_stats = ServerResourceStats {
        cpu_percent: 22.0,
        used_cores: 1.76,
        logical_cores: 8,
        allowed_cores: 8,
        cpu_model: "MediaTek MT6991".into(),
        ram_mb: 1840,
        uptime_secs: 1820,
        online_players: 2,
        max_players: 32,
        local_ping_ms: Some(1),
        public_ping_ms: Some(48),
        tps: Some(19.95),
        mspt: Some(3.4),
        online_player_names: vec!["TriHienKun".into(), "AlexBuilder".into()],
    };

    for language in [Language::Vi, Language::En] {
        let i18n = I18n::new(language);
        for width in [450.0, 710.0] {
            let hdr_harness = harness(width, 280.0, |ui| {
                server_header_ui(ui, &server, ServerRuntimeState::Online, &live_stats, &i18n);
                assert!(ui.min_rect().right() <= ui.max_rect().right() + 0.5);
            });
            for badge_txt in ["TPS 20.0 (3.4ms)", "Ping 1ms", "Internet 48ms"] {
                let rect = hdr_harness.get_by_label(badge_txt).rect();
                assert!(
                    rect.right() <= width + 0.5,
                    "{badge_txt} overflow at {width}"
                );
            }
            drop(hdr_harness);
        }
    }

    with_isolated_app(Language::Vi, |app| {
        app.settings.setup_completed = true;
        assert_eq!(
            app.settings.mobile_orientation,
            MobileOrientationMode::FullSensor
        );
        app.cycle_mobile_orientation();
        assert_eq!(app.settings.mobile_orientation, MobileOrientationMode::Auto);
        assert_eq!(
            app.db.load_settings().unwrap().mobile_orientation,
            MobileOrientationMode::Auto
        );

        let srv_dir = app.paths.servers_dir.join("test-player-mgmt-srv");
        std::fs::create_dir_all(&srv_dir).unwrap();
        let mut srv = server_fixture();
        srv.server_dir = srv_dir.clone();
        let srv_id = srv.id.clone();

        app.servers.selected_server_id = Some(srv_id.clone());
        app.servers.servers = vec![srv];
        app.servers.active_tab = ServerTab::AccessControl;
        app.servers
            .server_states
            .insert(srv_id.clone(), ServerRuntimeState::Online);
        app.servers.server_stats.insert(srv_id, live_stats.clone());
        app.servers.ops_list = vec![OpEntry {
            uuid: "00000000-0000-0000-0000-000000000001".into(),
            name: "TriHienKun".into(),
            level: 4,
            bypasses_player_limit: true,
        }];
        app.servers.known_players = vec![KnownPlayerEntry {
            uuid: "00000000-0000-0000-0000-000000000002".into(),
            name: "OfflineExplorer".into(),
            expires_on: None,
            has_playerdata: true,
            playerdata_bytes: 4096,
        }];
        app.servers.new_ban_ip = "203.0.113.45".into();
        app.servers.new_ban_ip_reason = "Packet flood".into();

        let save_lbl = app
            .i18n
            .tr("Lưu cấu hình người chơi", "Save Player Configuration")
            .to_owned();
        let ban_ip_lbl = app.i18n.tr("Cấm IP", "Ban IP").to_owned();

        let mut players_harness = harness(1240.0, 1400.0, |ui| {
            crate::views::servers::render_servers_view(app, ui);
        });
        players_harness.input_mut().time = None;
        assert_eq!(
            players_harness.query_all_by_label("TriHienKun").count(),
            2,
            "TriHienKun must appear in both Online Players card and Server Operators card"
        );
        assert!(players_harness.get_by_label("AlexBuilder").rect().width() > 20.0);
        assert!(
            players_harness
                .get_by_label("OfflineExplorer")
                .rect()
                .width()
                > 20.0
        );
        players_harness
            .get_by_role_and_label(egui::Role::Button, &save_lbl)
            .click();
        players_harness.run_steps(2);
        players_harness
            .get_by_role_and_label(egui::Role::Button, &ban_ip_lbl)
            .click();
        players_harness.run_steps(2);
        drop(players_harness);

        assert!(srv_dir.join("server.properties").is_file());
        assert_eq!(app.servers.banned_ips.len(), 1);
        assert_eq!(app.servers.banned_ips[0].ip, "203.0.113.45");
    });
}

#[test]
fn connected_network_direct_host_ui_and_copy_regression() {
    use std::net::{Ipv4Addr, Ipv6Addr};
    use thk_network::{
        ConnectedNetworkSnapshot, DirectHostActiveInfo, DirectHostStatus, NetworkInterfaceEntry,
        UpnpPortStatus,
    };

    let _render_guard = VISUAL_RENDER_LOCK.lock().expect("visual renderer lock");
    let synthetic_snapshot = ConnectedNetworkSnapshot {
        primary_ipv4: Some(Ipv4Addr::new(192, 168, 1, 5)),
        interfaces: vec![
            NetworkInterfaceEntry {
                name: "Wi-Fi".into(),
                ipv4: Ipv4Addr::new(192, 168, 1, 5),
                is_primary: true,
                is_virtual_hypervisor: false,
            },
            NetworkInterfaceEntry {
                name: "VPN (tun0)".into(),
                ipv4: Ipv4Addr::new(100, 96, 12, 8),
                is_primary: false,
                is_virtual_hypervisor: false,
            },
        ],
        global_ipv6: Some("2001:db8::42".parse::<Ipv6Addr>().unwrap()),
        public_ipv4: Some(Ipv4Addr::new(198, 51, 100, 77)),
    };
    let active_info = DirectHostActiveInfo {
        local_port: 25565,
        host_port: 25565,
        lan_endpoints: vec![
            ("Wi-Fi".into(), "192.168.1.5:25565".into()),
            ("VPN (tun0)".into(), "100.96.12.8:25565".into()),
        ],
        ipv6_endpoint: Some("[2001:db8::42]:25565".into()),
        public_ipv4_endpoint: Some("198.51.100.77:25565".into()),
        upnp_status: UpnpPortStatus::Mapped {
            external_ip: Ipv4Addr::new(198, 51, 100, 77),
            external_port: 25565,
            gateway_label: "IGD Gateway".into(),
        },
        announcing_lan: true,
        bridging_tcp: true,
        active_clients: 2,
    };

    for language in [Language::Vi, Language::En] {
        with_isolated_app(language, |app| {
            app.settings.setup_completed = true;
            let srv = server_fixture();
            let srv_id = srv.id.clone();
            app.servers.selected_server_id = Some(srv_id.clone());
            app.servers.servers = vec![srv];
            app.servers.active_tab = ServerTab::Network;
            app.servers.network_snapshot = synthetic_snapshot.clone();
            app.lan_modal.network_snapshot = synthetic_snapshot.clone();
            app.lan_modal.primary_lan_ip = Some("192.168.1.5".into());
            app.servers
                .server_direct_hosts
                .insert(srv_id, DirectHostStatus::Running(active_info.clone()));

            let copy_v6_lbl = app
                .i18n
                .tr("Sao chép Online IPv6", "Copy Online IPv6")
                .to_owned();
            let copy_v4_lbl = app
                .i18n
                .tr("Sao chép Online IPv4", "Copy Online IPv4")
                .to_owned();
            let close_lbl = app
                .i18n
                .tr("Đóng Port Mạng", "Close Hosted Port")
                .to_owned();

            let mut net_harness = harness(1240.0, 920.0, |ui| {
                crate::views::servers::render_servers_view(app, ui);
                assert!(ui.min_rect().right() <= 1240.5);
            });
            net_harness.input_mut().time = None;

            assert!(
                net_harness
                    .get_by_label("[2001:db8::42]:25565")
                    .rect()
                    .width()
                    > 30.0
            );
            assert!(
                net_harness
                    .get_by_label("198.51.100.77:25565")
                    .rect()
                    .width()
                    > 30.0
            );
            assert!(
                net_harness
                    .get_by_role_and_label(egui::Role::Button, &copy_v4_lbl)
                    .rect()
                    .width()
                    > 20.0
            );
            assert!(
                net_harness
                    .get_by_role_and_label(egui::Role::Button, &close_lbl)
                    .rect()
                    .width()
                    > 20.0
            );

            let copy_v6_pos = net_harness
                .get_by_role_and_label(egui::Role::Button, &copy_v6_lbl)
                .rect()
                .center();
            net_harness
                .input_mut()
                .events
                .push(egui::Event::PointerMoved(copy_v6_pos));
            net_harness.run_steps(1);
            net_harness
                .input_mut()
                .events
                .push(egui::Event::PointerButton {
                    pos: copy_v6_pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                });
            net_harness.step();
            net_harness
                .input_mut()
                .events
                .push(egui::Event::PointerButton {
                    pos: copy_v6_pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                });
            net_harness.step();
            let copied_v6 = net_harness.output().platform_output.commands.iter().any(
                |cmd| matches!(cmd, egui::OutputCommand::CopyText(t) if t == "[2001:db8::42]:25565"),
            );
            drop(net_harness);
            assert!(copied_v6, "Copy Online IPv6 must emit CopyText command");

            app.show_lan_modal = true;
            app.lan_modal.direct_host_status = DirectHostStatus::Running(active_info.clone());
            let mut lan_harness = harness(960.0, 760.0, |ui| {
                crate::views::modals::render_all_modals(app, ui.ctx());
            });
            lan_harness.input_mut().time = None;
            assert!(
                lan_harness
                    .get_by_label("[2001:db8::42]:25565")
                    .rect()
                    .width()
                    > 30.0
            );
            assert!(lan_harness.get_by_label("192.168.1.5:25565").rect().width() > 30.0);
            drop(lan_harness);
        });
    }
}

#[test]
fn mobile_game_screen_bedrock_controls_editor_and_floating_hub_toggle() {
    let _render_guard = VISUAL_RENDER_LOCK.lock().expect("visual renderer lock");
    for language in [Language::Vi, Language::En] {
        with_isolated_app(language, |app| {
            app.settings.setup_completed = true;
            app.open_mobile_game_screen(false);
            app.mobile_game.floating_menu_expanded = true;

            let open_hub_lbl = app
                .i18n
                .tr("Hiện HUB UI App", "Open Launcher HUB UI")
                .to_owned();
            let edit_lbl = app
                .i18n
                .tr("Edit Phím Tùy Chỉnh", "Edit Custom Controls")
                .to_owned();

            let mut game_harness = harness(960.0, 540.0, |ui| {
                crate::views::render_root_ui(app, ui, None);
            });
            game_harness.input_mut().time = None;
            assert!(game_harness.get_by_label(&open_hub_lbl).rect().width() > 40.0);

            game_harness.get_by_label(&edit_lbl).click();
            game_harness.run_steps(2);
            drop(game_harness);
            assert!(app.mobile_game.edit_mode);

            let add_key_lbl = app.i18n.tr("+ Thêm Phím", "+ Add Key").to_owned();
            let save_done_lbl = app.i18n.tr("Lưu & Xong", "Save & Done").to_owned();
            let initial_btn_count = app.mobile_controls.buttons.len();

            let mut edit_harness = harness(960.0, 540.0, |ui| {
                crate::views::render_root_ui(app, ui, None);
            });
            edit_harness.input_mut().time = None;
            edit_harness.get_by_label(&add_key_lbl).click();
            edit_harness.run_steps(2);
            edit_harness.get_by_label(&save_done_lbl).click();
            edit_harness.run_steps(2);
            drop(edit_harness);

            assert!(!app.mobile_game.edit_mode);
            assert_eq!(app.mobile_controls.buttons.len(), initial_btn_count + 1);
            assert!(app.paths.root.join("mobile_controls.json").is_file());

            app.mobile_game.floating_menu_expanded = true;
            let mut toggle_hub_harness = harness(960.0, 540.0, |ui| {
                crate::views::render_root_ui(app, ui, None);
            });
            toggle_hub_harness.input_mut().time = None;
            toggle_hub_harness.get_by_label(&open_hub_lbl).click();
            toggle_hub_harness.run_steps(2);
            drop(toggle_hub_harness);

            assert!(app.mobile_game.hub_visible_over_game);

            app.mobile_game.floating_menu_expanded = true;
            let hide_hub_lbl = app
                .i18n
                .tr("Ẩn HUB UI (Về Game)", "Hide HUB (Back to Game)")
                .to_owned();
            let mut back_harness = harness(960.0, 540.0, |ui| {
                crate::views::render_root_ui(app, ui, None);
            });
            back_harness.input_mut().time = None;
            back_harness.get_by_label(&hide_hub_lbl).click();
            back_harness.run_steps(2);
            drop(back_harness);

            assert!(!app.mobile_game.hub_visible_over_game);
        });
    }
}

#[test]
fn visual_bright_engine_portrait_and_landscape_responsive() {
    use thk_core::models::{GameProfile, LoaderType};

    let _render_guard = VISUAL_RENDER_LOCK.lock().expect("visual renderer lock");
    let mut snapshots = egui_kittest::SnapshotResults::new();

    with_isolated_app(Language::Vi, |app| {
        app.settings.setup_completed = true;
        let prof = GameProfile {
            id: "portrait-landscape-prof".into(),
            name: "Bright Engine Survival".into(),
            game_version: "1.21.4".into(),
            loader: LoaderType::Fabric,
            loader_version: Some("0.16.10".into()),
            instance_path: "visual-test-not-a-real-profile".into(),
            is_custom_path: false,
            ram_mb: 4096,
            java_path: None,
            jvm_args: String::new(),
            auto_perf_mods: true,
            icon_url: None,
            window_width: Some(1280),
            window_height: Some(720),
            fullscreen: false,
            total_play_seconds: 3720,
            session_count: 5,
            crash_count: 0,
            last_played_at: None,
            created_at: 0,
        };
        let prof_id = prof.id.clone();
        app.home.profiles = vec![prof];
        app.select_profile(prof_id);

        let srv = server_fixture();
        app.servers.selected_server_id = Some(srv.id.clone());
        app.servers.servers = vec![srv];

        assert!(crate::views::is_compact_portrait(Rect::from_min_size(
            pos2(0.0, 0.0),
            vec2(412.0, 892.0)
        )));
        assert!(!crate::views::is_compact_portrait(Rect::from_min_size(
            pos2(0.0, 0.0),
            vec2(960.0, 440.0)
        )));

        // 1. Mobile Portrait (412x892) in Bright Game-Engine Theme + Dark Cobalt Theme
        let nav_servers_lbl = app.i18n.t("nav.servers").to_owned();
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            app.settings.theme = theme;
            for section in [
                crate::state::ActiveSection::Home,
                crate::state::ActiveSection::Modpacks,
                crate::state::ActiveSection::Servers,
                crate::state::ActiveSection::Accounts,
            ] {
                app.section = section;
                let mut h = harness_themed(412.0, 892.0, theme, |ui| {
                    crate::views::render_root_ui(app, ui, None);
                });
                if theme == ThemeMode::Light && section == crate::state::ActiveSection::Home {
                    h.snapshot("workspace_portrait_home_light");
                    snapshots.extend(h.take_snapshot_results());
                }
                let play_or_nav = h
                    .get_by_role_and_label(egui::Role::Button, &nav_servers_lbl)
                    .rect();
                assert!(play_or_nav.right() <= 412.5 && play_or_nav.left() >= -0.5);
                drop(h);
            }
        }

        // Verify compact Modpacks filter drawer opens cleanly on Portrait
        app.settings.theme = ThemeMode::Light;
        app.section = crate::state::ActiveSection::Modpacks;
        assert!(!app.detached.compact_modpack_filters_open);
        let filter_lbl = app.i18n.tr("Bộ lọc", "Filters").to_owned();
        let mut mp_h = harness_themed(412.0, 892.0, ThemeMode::Light, |ui| {
            crate::views::render_root_ui(app, ui, None);
        });
        mp_h.input_mut().time = None;
        mp_h.get_by_role_and_label(egui::Role::Button, &filter_lbl)
            .click();
        mp_h.run_steps(2);
        drop(mp_h);
        assert!(app.detached.compact_modpack_filters_open);

        // 2. Mobile Landscape (960x440) in Bright Game-Engine Theme
        app.section = crate::state::ActiveSection::Home;
        let mut land_h = harness_themed(960.0, 440.0, ThemeMode::Light, |ui| {
            crate::views::render_root_ui(app, ui, None);
        });
        land_h.snapshot("workspace_landscape_home_light");
        snapshots.extend(land_h.take_snapshot_results());
        drop(land_h);

        // 3. Mobile Game Screen in Portrait (412x892) + Orientation cycle in floating HUB menu
        app.open_mobile_game_screen(false);
        app.mobile_game.floating_menu_expanded = true;
        let init_orient = app.settings.mobile_orientation;
        let orient_btn_lbl = format!(
            "{}: {}",
            app.i18n.tr("Xoay màn hình", "Orientation"),
            app.mobile_orientation_label(init_orient)
        );
        let mut mg_h = harness_themed(412.0, 892.0, ThemeMode::Light, |ui| {
            crate::views::render_root_ui(app, ui, None);
        });
        mg_h.input_mut().time = None;
        let orient_rect = mg_h
            .get_by_role_and_label(egui::Role::Button, &orient_btn_lbl)
            .rect();
        assert!(orient_rect.right() <= 412.0);
        mg_h.get_by_role_and_label(egui::Role::Button, &orient_btn_lbl)
            .click();
        mg_h.run_steps(2);
        drop(mg_h);
        assert_eq!(app.settings.mobile_orientation, init_orient.next_cycle());
    });
}

#[test]
fn desktop_multi_window_detached_panels_render_cleanly() {
    use thk_core::models::{GameProfile, LoaderType};

    let _render_guard = VISUAL_RENDER_LOCK.lock().expect("visual renderer lock");
    with_isolated_app(Language::Vi, |app| {
        app.settings.setup_completed = true;
        assert_eq!(app.settings.theme, ThemeMode::Light);

        let prof_id = "multiwin-prof".to_string();
        let prof = GameProfile {
            id: prof_id.clone(),
            name: "Multi-Window Fabric".into(),
            game_version: "1.21.4".into(),
            loader: LoaderType::Fabric,
            loader_version: Some("0.16.10".into()),
            instance_path: "visual-test-not-a-real-profile".into(),
            is_custom_path: false,
            ram_mb: 4096,
            java_path: None,
            jvm_args: String::new(),
            auto_perf_mods: false,
            icon_url: None,
            window_width: Some(1280),
            window_height: Some(720),
            fullscreen: false,
            total_play_seconds: 120,
            session_count: 1,
            crash_count: 0,
            last_played_at: None,
            created_at: 0,
        };
        app.home.profiles = vec![prof];
        app.select_profile(prof_id.clone());
        app.home.instance_logs.insert(
            thk_launcher::InstanceKey::new(prof_id, "acct-test"),
            vec!["[Render thread/INFO]: OpenGL initialized".into()],
        );

        let srv = server_fixture();
        let srv_id = srv.id.clone();
        app.servers.selected_server_id = Some(srv_id.clone());
        app.servers.servers = vec![srv];
        app.servers
            .server_logs
            .insert(srv_id, vec!["[12:00:00 INFO]: Done (2.1s)!".into()]);

        // Toggle multi-window workspace button from desktop left navigation rail
        let multi_lbl = app.i18n.tr("Cửa sổ Đa nhiệm", "Multi-Window").to_owned();
        let mut root_h = harness_themed(1280.0, 800.0, ThemeMode::Light, |ui| {
            crate::views::render_root_ui(app, ui, None);
        });
        root_h.input_mut().time = None;
        root_h
            .get_by_role_and_label(egui::Role::Button, &multi_lbl)
            .click();
        root_h.run_steps(2);
        drop(root_h);
        assert!(app.detached.secondary_workspace_open);

        // Render detached Game Session Logs panel cleanly
        app.detached.game_logs_window_open = true;
        let logs_h = harness_themed(720.0, 480.0, ThemeMode::Light, |ui| {
            crate::views::home::render_detached_game_logs_panel(app, ui);
        });
        assert!(
            logs_h
                .get_by_label("[Render thread/INFO]: OpenGL initialized")
                .rect()
                .width()
                > 40.0
        );
        drop(logs_h);

        // Render detached Server Console panel cleanly
        app.detached.server_console_window_open = true;
        let srv_h = harness_themed(780.0, 520.0, ThemeMode::Light, |ui| {
            crate::views::servers::render_detached_server_console_panel(app, ui);
        });
        assert!(
            srv_h
                .get_by_label("[12:00:00 INFO]: Done (2.1s)!")
                .rect()
                .width()
                > 40.0
        );
        drop(srv_h);
    });
}

fn harness_scaled<'a>(
    width: f32,
    height: f32,
    font_scale: f32,
    theme: ThemeMode,
    render: impl FnMut(&mut egui::Ui) + 'a,
) -> Harness<'a> {
    let egui_theme = match theme {
        ThemeMode::Dark => egui::Theme::Dark,
        ThemeMode::Light => egui::Theme::Light,
    };
    let mut harness = Harness::builder()
        .with_size(vec2(width, height))
        .with_pixels_per_point(1.0)
        .with_theme(egui_theme)
        .with_os(egui::os::OperatingSystem::Windows)
        .with_accessibility_check(false)
        .with_context_setup(move |ctx| {
            crate::theme::install_unicode_fonts(ctx);
            crate::theme::apply_font_scale(ctx, font_scale);
            crate::theme::apply_theme(ctx, theme, AccentPreset::Emerald);
        })
        .build_ui(render);
    harness.input_mut().time = Some(1.0);
    harness.run_steps(3);
    harness
}

#[test]
fn font_scale_extremes_from_min_to_max_never_break_layout() {
    use crate::state::{ActiveSection, ServerTab};
    use thk_core::models::{
        Account, AccountType, GameProfile, LoaderType, SkinVariant, MAX_FONT_SCALE, MIN_FONT_SCALE,
    };

    let _render_guard = VISUAL_RENDER_LOCK.lock().expect("visual renderer lock");
    with_isolated_app(Language::Vi, |app| {
        app.settings.setup_completed = true;
        let acct = Account {
            id: "scale-acct".into(),
            username: "ScaleTester".into(),
            uuid: "00000000-0000-0000-0000-000000000001".into(),
            account_type: AccountType::Offline,
            skin_variant: SkinVariant::Wide,
            skin_url: None,
            cape_url: None,
            custom_skin_path: None,
            custom_cape_path: None,
            custom_elytra_path: None,
            token_expires_at: None,
            oauth_client_id: None,
            created_at: 0,
        };
        app.accounts.selected_account_id = Some(acct.id.clone());
        app.accounts.accounts = vec![acct];

        let prof = GameProfile {
            id: "scale-prof".into(),
            name: "Extreme Font Scale Fabric 1.21.4".into(),
            game_version: "1.21.4".into(),
            loader: LoaderType::Fabric,
            loader_version: Some("0.16.10".into()),
            instance_path: "visual-test-scale-profile".into(),
            is_custom_path: false,
            ram_mb: 4096,
            java_path: None,
            jvm_args: "-XX:+UseG1GC".into(),
            auto_perf_mods: true,
            icon_url: None,
            window_width: Some(1280),
            window_height: Some(720),
            fullscreen: false,
            total_play_seconds: 5400,
            session_count: 3,
            crash_count: 0,
            last_played_at: None,
            created_at: 0,
        };
        app.home.profiles = vec![prof.clone()];
        app.select_profile(prof.id);

        let srv = server_fixture();
        app.servers.selected_server_id = Some(srv.id.clone());
        app.servers.servers = vec![srv];

        let scales = [MIN_FONT_SCALE, 0.75, 1.00, 1.50, MAX_FONT_SCALE];
        let viewports = [
            ("desktop", 1280.0_f32, 800.0_f32),
            ("mobile_landscape", 924.0_f32, 426.0_f32),
            ("mobile_portrait", 412.0_f32, 892.0_f32),
        ];

        for &scale in &scales {
            app.settings.font_scale = scale;
            for &(vp_name, phys_w, phys_h) in &viewports {
                for (section, home_tab, srv_tab) in [
                    (ActiveSection::Home, HomeTab::General, ServerTab::Console),
                    (ActiveSection::Home, HomeTab::Mods, ServerTab::Console),
                    (ActiveSection::Home, HomeTab::Bookmarks, ServerTab::Console),
                    (
                        ActiveSection::Modpacks,
                        HomeTab::General,
                        ServerTab::Console,
                    ),
                    (ActiveSection::Servers, HomeTab::General, ServerTab::Console),
                    (
                        ActiveSection::Servers,
                        HomeTab::General,
                        ServerTab::Properties,
                    ),
                    (
                        ActiveSection::Servers,
                        HomeTab::General,
                        ServerTab::AccessControl,
                    ),
                    (ActiveSection::Servers, HomeTab::General, ServerTab::Network),
                    (
                        ActiveSection::Accounts,
                        HomeTab::General,
                        ServerTab::Console,
                    ),
                ] {
                    app.section = section;
                    app.home.active_tab = home_tab;
                    app.servers.active_tab = srv_tab;
                    app.mobile_game.screen_open = false;

                    let nav_srv_lbl = app.i18n.t("nav.servers").to_owned();
                    let mut observed_vp = Rect::NOTHING;
                    let h = harness_scaled(phys_w, phys_h, scale, ThemeMode::Light, |ui| {
                        assert!(
                            (ui.ctx().zoom_factor() - scale).abs() < 1e-4,
                            "zoom_factor mismatch at scale={scale}"
                        );
                        observed_vp = ui.ctx().content_rect();
                        crate::views::render_root_ui(app, ui, None);
                    });
                    let nav_btn_rect = h
                        .query_by_role_and_label(egui::Role::Button, &nav_srv_lbl)
                        .map(|n| n.rect());
                    drop(h);

                    assert!(
                        observed_vp.width() > 60.0 && observed_vp.height() > 60.0,
                        "invalid content_rect {observed_vp:?} on {vp_name}@{scale}"
                    );
                    if let Some(r) = nav_btn_rect {
                        assert!(
                            r.width() > 8.0
                                && r.height() > 8.0
                                && r.left() >= -2.0
                                && r.right() <= observed_vp.width() + 2.0,
                            "nav button overflow on {vp_name}@{scale} ({section:?}): {r:?} vs {observed_vp:?}"
                        );
                    }
                }

                // Stress modals at min/max font scale on every form factor
                app.section = ActiveSection::Home;
                for modal_case in 0..4 {
                    app.show_settings_modal = modal_case == 0;
                    app.show_new_profile_modal = modal_case == 1;
                    app.show_new_server_modal = modal_case == 2;
                    app.show_close_confirm_modal = modal_case == 3;

                    let mut vp_rect = Rect::NOTHING;
                    let mut win_rect = Rect::NOTHING;
                    let h = harness_scaled(phys_w, phys_h, scale, ThemeMode::Light, |ui| {
                        vp_rect = ui.ctx().content_rect();
                        win_rect = crate::views::centered_window_rect(ui.ctx(), vec2(650.0, 580.0));
                        crate::views::render_root_ui(app, ui, None);
                    });
                    drop(h);
                    assert!(
                        win_rect.width() <= vp_rect.width() + 1.0
                            && win_rect.height() <= vp_rect.height() + 1.0
                            && win_rect.left() >= vp_rect.left() - 1.0
                            && win_rect.right() <= vp_rect.right() + 1.0,
                        "modal window rect overflow on {vp_name}@{scale} case={modal_case}: {win_rect:?} vs {vp_rect:?}"
                    );
                }
                app.show_settings_modal = false;
                app.show_new_profile_modal = false;
                app.show_new_server_modal = false;
                app.show_close_confirm_modal = false;

                // Initial setup modal at extreme font scales
                app.settings.setup_completed = false;
                let start_lbl = app
                    .i18n
                    .tr(
                        "Bắt đầu sử dụng TriHienKun Laucher",
                        "Get Started with TriHienKun Laucher",
                    )
                    .to_owned();
                let mut setup_vp = Rect::NOTHING;
                let setup_h = harness_scaled(phys_w, phys_h, scale, ThemeMode::Light, |ui| {
                    setup_vp = ui.ctx().content_rect();
                    crate::views::render_root_ui(app, ui, None);
                });
                let start_rect = setup_h
                    .get_by_role_and_label(egui::Role::Button, &start_lbl)
                    .rect();
                drop(setup_h);
                assert!(
                    start_rect.left() >= -2.0 && start_rect.right() <= setup_vp.width() + 2.0,
                    "initial setup CTA overflow on {vp_name}@{scale}: {start_rect:?} vs {setup_vp:?}"
                );
                app.settings.setup_completed = true;
            }
        }
    });
}

#[test]
fn android_safe_area_insets_and_free_autorotate_keep_edge_buttons_clear() {
    use thk_core::mobile_controls::{
        ControlBinding, ControlShape, MobileControlButton, GLFW_KEY_E, GLFW_KEY_F3,
    };
    use thk_core::models::{AppSettings, MobileOrientationMode};

    let _render_guard = VISUAL_RENDER_LOCK.lock().expect("visual renderer lock");

    // 1. Default orientation must be unconstrained 4-way FullSensor ("full_sensor")
    assert_eq!(
        AppSettings::default().mobile_orientation,
        MobileOrientationMode::FullSensor
    );
    assert_eq!(
        MobileOrientationMode::FullSensor.as_bridge_str(),
        "full_sensor"
    );
    assert_eq!(MobileOrientationMode::Auto.as_bridge_str(), "unspecified");

    // 2. Verify one-time migration upgrades legacy constrained SensorLandscape to FullSensor,
    //    then respects subsequent explicit user choices.
    {
        let root = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap())
            .join("Temp/opencode")
            .join(format!("thk-orient-mig-{}", uuid::Uuid::new_v4()));
        let paths = thk_core::AppPaths::from_root(root.clone()).unwrap();
        let db = std::sync::Arc::new(thk_core::Database::open_in_memory().unwrap());
        let mut legacy = db.load_settings().unwrap();
        legacy.mobile_orientation = MobileOrientationMode::SensorLandscape;
        legacy.auto_check_updates = false;
        legacy.music_enabled = false;
        legacy.discord_rpc_enabled = false;
        db.save_settings(&legacy).unwrap();

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut app = crate::state::LauncherAppState::initialize(
            runtime.handle().clone(),
            egui::Context::default(),
            paths.clone(),
            db.clone(),
            None,
        )
        .unwrap();
        assert_eq!(
            app.settings.mobile_orientation,
            MobileOrientationMode::FullSensor,
            "legacy constrained orientation should migrate once to FullSensor"
        );
        assert_eq!(
            db.get_kv("mobile_free_orientation_v1").unwrap().as_deref(),
            Some("1")
        );
        app.settings.mobile_orientation = MobileOrientationMode::Portrait;
        app.persist_settings();
        app.audio.shutdown();
        drop(app);

        let app2 = crate::state::LauncherAppState::initialize(
            runtime.handle().clone(),
            egui::Context::default(),
            paths,
            db,
            None,
        )
        .unwrap();
        assert_eq!(
            app2.settings.mobile_orientation,
            MobileOrientationMode::Portrait,
            "explicit user orientation choice after migration must be preserved"
        );
        app2.audio.shutdown();
        drop(app2);
        drop(runtime);
        let _ = std::fs::remove_dir_all(root);
    }

    // 3. Inject realistic Android cutout + gesture bar + rounded corner insets and verify
    //    every edge bar, dock, Bedrock control, and floating HUB menu stays strictly inside safe area.
    with_isolated_app(Language::Vi, |app| {
        app.settings.setup_completed = true;
        app.android_permissions.safe_inset_left_px = 42;
        app.android_permissions.safe_inset_top_px = 36;
        app.android_permissions.safe_inset_right_px = 24;
        app.android_permissions.safe_inset_bottom_px = 28;

        let nav_labels: Vec<String> = ["nav.home", "nav.modpacks", "nav.servers", "nav.accounts"]
            .into_iter()
            .map(|k| app.i18n.t(k).to_owned())
            .collect();
        let settings_lbl = app.i18n.t("nav.settings").to_owned();
        let nav_home_lbl = app.i18n.t("nav.home").to_owned();

        // 3a. Mobile Portrait (412x892): Top bar + Bottom dock buttons must clear safe area
        app.section = crate::state::ActiveSection::Home;
        let mut safe_portrait = Rect::NOTHING;
        let port_h = harness_themed(412.0, 892.0, ThemeMode::Light, |ui| {
            crate::views::render_root_ui(app, ui, None);
            safe_portrait = crate::views::safe_content_rect(ui.ctx());
        });
        let dock_rects: Vec<(String, Rect)> = nav_labels
            .iter()
            .map(|lbl| {
                (
                    lbl.clone(),
                    port_h.get_by_role_and_label(egui::Role::Button, lbl).rect(),
                )
            })
            .collect();
        let top_btn_r = port_h
            .get_by_role_and_label(egui::Role::Button, &settings_lbl)
            .rect();
        drop(port_h);

        assert!((safe_portrait.left() - 42.0).abs() < 0.5);
        assert!((safe_portrait.top() - 36.0).abs() < 0.5);
        assert!((safe_portrait.right() - (412.0 - 24.0)).abs() < 0.5);
        assert!((safe_portrait.bottom() - (892.0 - 28.0)).abs() < 0.5);

        for (lbl, r) in dock_rects {
            assert!(
                r.left() >= safe_portrait.left() - 1.0
                    && r.right() <= safe_portrait.right() + 1.0
                    && r.bottom() <= safe_portrait.bottom() + 1.0,
                "portrait dock button '{lbl}' intruded into bezel/gesture inset: {r:?} vs {safe_portrait:?}"
            );
        }
        assert!(
            top_btn_r.top() >= safe_portrait.top() - 1.0
                && top_btn_r.right() <= safe_portrait.right() + 1.0,
            "portrait top bar settings button intruded into cutout inset: {top_btn_r:?} vs {safe_portrait:?}"
        );

        // 3b. Mobile Landscape (924x426): Left nav rail must clear left camera cutout
        let mut safe_landscape = Rect::NOTHING;
        let land_h = harness_themed(924.0, 426.0, ThemeMode::Light, |ui| {
            crate::views::render_root_ui(app, ui, None);
            safe_landscape = crate::views::safe_content_rect(ui.ctx());
        });
        let nav_home_r = land_h
            .get_by_role_and_label(egui::Role::Button, &nav_home_lbl)
            .rect();
        drop(land_h);
        assert!(
            nav_home_r.left() >= safe_landscape.left() - 1.0
                && nav_home_r.top() >= safe_landscape.top() - 1.0,
            "landscape left nav button intruded into camera cutout: {nav_home_r:?} vs {safe_landscape:?}"
        );

        // 3c. Mobile Game Screen: extreme (0,0) and (1,1) touch buttons + corner floating HUB menu
        app.mobile_controls.buttons.push(MobileControlButton {
            id: "edge_tl".into(),
            label: "TL".into(),
            x_norm: 0.0,
            y_norm: 0.0,
            width_dp: 56.0,
            height_dp: 56.0,
            scale: 1.0,
            opacity: 0.85,
            toggle_mode: false,
            shape: ControlShape::Round,
            binding: ControlBinding::Key {
                primary: GLFW_KEY_F3,
                secondary: 0,
            },
        });
        app.mobile_controls.buttons.push(MobileControlButton {
            id: "edge_br".into(),
            label: "BR".into(),
            x_norm: 1.0,
            y_norm: 1.0,
            width_dp: 56.0,
            height_dp: 56.0,
            scale: 1.0,
            opacity: 0.85,
            toggle_mode: false,
            shape: ControlShape::Round,
            binding: ControlBinding::Key {
                primary: GLFW_KEY_E,
                secondary: 0,
            },
        });
        app.mobile_game.screen_open = true;
        app.mobile_game.hub_visible_over_game = false;
        app.mobile_game.floating_menu_expanded = true;

        for (fx, fy) in [
            (-50.0_f32, -50.0_f32),
            (9999.0, -50.0),
            (-50.0, 9999.0),
            (9999.0, 9999.0),
        ] {
            app.mobile_game.floating_menu_pos = Some(pos2(fx, fy));

            let mut safe_game = Rect::NOTHING;
            let hub_lbl = app
                .i18n
                .tr("Hiện HUB UI App", "Open Launcher HUB UI")
                .to_owned();
            let game_h = harness_themed(924.0, 426.0, ThemeMode::Light, |ui| {
                crate::views::render_root_ui(app, ui, None);
                safe_game = crate::views::safe_content_rect(ui.ctx());
            });
            let hub_btn_r = game_h
                .get_by_role_and_label(egui::Role::Button, &hub_lbl)
                .rect();
            drop(game_h);

            for btn in &app.mobile_controls.buttons {
                let br = crate::views::mobile_game::button_rect_on_canvas(btn, safe_game);
                assert!(
                    br.left() >= safe_game.left() - 0.5
                        && br.right() <= safe_game.right() + 0.5
                        && br.top() >= safe_game.top() - 0.5
                        && br.bottom() <= safe_game.bottom() + 0.5,
                    "Bedrock button '{}' overflowed safe canvas: {br:?} vs {safe_game:?}",
                    btn.id
                );
            }

            let fab_r =
                crate::views::mobile_game::floating_menu_hit_rect(&app.mobile_game, safe_game);
            assert!(
                fab_r.left() >= safe_game.left() - 0.5
                    && fab_r.right() <= safe_game.right() + 0.5
                    && fab_r.top() >= safe_game.top() - 0.5
                    && fab_r.bottom() <= safe_game.bottom() + 0.5,
                "Floating HUB pill overflowed safe rect at ({fx}, {fy}): {fab_r:?} vs {safe_game:?}"
            );

            assert!(
                hub_btn_r.left() >= safe_game.left() - 1.0
                    && hub_btn_r.right() <= safe_game.right() + 1.0
                    && hub_btn_r.top() >= safe_game.top() - 1.0
                    && hub_btn_r.bottom() <= safe_game.bottom() + 1.0,
                "Expanded floating HUB menu button overflowed safe rect at ({fx}, {fy}): {hub_btn_r:?} vs {safe_game:?}"
            );
        }
    });
}

#[test]
fn servers_long_modpack_paths_and_all_tabs_never_overflow_portrait_or_landscape() {
    use crate::state::{ActiveSection, ServerTab};
    use thk_core::models::{ContentKind, ProviderSource};
    use thk_mods::{ProjectVersion, ServerAddonFile, VersionFile};
    use thk_server::{
        BannedIpEntry, BannedPlayerEntry, KnownPlayerEntry, OpEntry, ServerFileEntry,
        WhitelistEntry,
    };

    fn assert_subtree_within_horizontal_bounds(
        node: egui_kittest::Node<'_>,
        min_phys_x: f32,
        max_phys_x: f32,
        context: &str,
    ) {
        let role = node.accesskit_node().role();
        if let Some(bb) = node.accesskit_node().bounding_box() {
            let x0 = bb.x0 as f32;
            let x1 = bb.x1 as f32;
            let w = x1 - x0;
            let h = (bb.y1 - bb.y0) as f32;
            if x0.is_finite() && x1.is_finite() && w > 0.5 && h > 0.5 {
                let label = node.accesskit_node().label().unwrap_or_default();
                assert!(
                    x0 >= min_phys_x - 3.0 && x1 <= max_phys_x + 3.0,
                    "horizontal overflow in {context}: role={role:?} label={label:?} x=[{x0:.1}, {x1:.1}] bounds=[{min_phys_x:.1}, {max_phys_x:.1}]"
                );
            }
        }
        if matches!(
            role,
            egui::accesskit::Role::TextInput | egui::accesskit::Role::MultilineTextInput
        ) {
            return;
        }
        for child in node.children() {
            assert_subtree_within_horizontal_bounds(child, min_phys_x, max_phys_x, context);
        }
    }

    let _render_guard = VISUAL_RENDER_LOCK.lock().expect("visual renderer lock");
    with_isolated_app(Language::Vi, |app| {
        app.settings.setup_completed = true;
        app.section = ActiveSection::Servers;

        let mut srv = server_fixture();
        srv.name = "Cisco's Fantasy Medieval RPG [Ultimate] Server".into();
        srv.engine = ServerEngine::Forge;
        srv.game_version = "1.19.2".into();
        srv.jar_file = "@libraries/net/minecraftforge/forge/1.19.2-43.5.2/unix_args.txt".into();
        srv.jvm_flags = "-XX:+UseG1GC -XX:+ParallelRefProcEnabled -XX:MaxGCPauseMillis=200 -XX:+UnlockExperimentalVMOptions".into();
        let srv_id = srv.id.clone();

        app.servers.selected_server_id = Some(srv_id.clone());
        app.servers.servers = vec![srv];
        app.servers.runnable_targets = vec![
            "@libraries/net/minecraftforge/forge/1.19.2-43.5.2/unix_args.txt".into(),
            "forge-1.19.2-43.5.2-shim.jar".into(),
        ];
        app.servers.server_logs.insert(
            srv_id.clone(),
            vec![
                "[21:10:01] [main/INFO] [cpw.mods.modlauncher.Launcher/MODLAUNCHER]: ModLauncher running: args [--launchTarget, forgeserver, --fml.forgeVersion, 43.5.2]".into(),
                "[21:10:15] [Server thread/INFO] [minecraft/DedicatedServer]: Done (14.218s)! For help, type \"help\"".into(),
            ],
        );
        app.servers.dir_entries = vec![
            ServerFileEntry {
                name: "libraries".into(),
                rel_path: "libraries".into(),
                is_dir: true,
                size_bytes: 0,
            },
            ServerFileEntry {
                name: "ciscos_fantasy_medieval_rpg_ultimate_server_overrides_config.toml".into(),
                rel_path: "ciscos_fantasy_medieval_rpg_ultimate_server_overrides_config.toml"
                    .into(),
                is_dir: false,
                size_bytes: 48_192,
            },
        ];
        app.servers.installed_addons = vec![ServerAddonFile {
            file_name: "ciscos_medieval_rpg_questline_core_integration-1.19.2-4.8.12-forge.jar"
                .into(),
            base_file_name:
                "ciscos_medieval_rpg_questline_core_integration-1.19.2-4.8.12-forge.jar".into(),
            folder: "mods".into(),
            enabled: true,
            size_bytes: 2_621_440,
            title: "Cisco's Medieval RPG Questline Core Integration".into(),
            version_number: "4.8.12-forge".into(),
        }];
        let addon_hit = project_fixture();
        app.servers.addon_hits = vec![addon_hit.clone()];
        app.servers.addon_total_hits = 1;
        app.servers.addon_selected = Some((
            addon_hit,
            vec![ProjectVersion {
                id: "ver-long-forge".into(),
                project_id: "layout-test".into(),
                provider: ProviderSource::Modrinth,
                name: "Release 4.8.12+mc1.19.2-forge-universal-dedicated-server".into(),
                version_number: "4.8.12+mc1.19.2-forge-universal-dedicated-server".into(),
                release_channel: "release".into(),
                published_at: None,
                game_versions: vec!["1.19.2".into()],
                loaders: vec!["forge".into()],
                files: vec![VersionFile {
                    url: "https://cdn.example.invalid/mod.jar".into(),
                    filename: "layout-test-4.8.12-forge-universal.jar".into(),
                    size: 1024,
                    sha1: None,
                    sha512: None,
                    primary: true,
                }],
                dependencies: Vec::new(),
                server_pack_file_id: None,
            }],
        ));
        app.servers.properties_map.extend([
            ("gamemode".into(), "survival".into()),
            ("difficulty".into(), "hard".into()),
            (
                "motd".into(),
                "Cisco's Fantasy Medieval RPG [Ultimate] Dedicated Server".into(),
            ),
            ("level-type".into(), "minecraft:normal".into()),
            (
                "resource-pack".into(),
                "https://cdn.example.invalid/packs/ciscos-medieval-ultimate-hd-v8e.zip".into(),
            ),
            ("online-mode".into(), "false".into()),
            ("pvp".into(), "true".into()),
        ]);
        app.servers.known_players = vec![KnownPlayerEntry {
            uuid: "00000000-0000-0000-0000-000000000099".into(),
            name: "MedievalKnightCommander".into(),
            expires_on: Some("2027-01-01 00:00:00 +0000".into()),
            has_playerdata: true,
            playerdata_bytes: 16_384,
        }];
        app.servers.ops_list = vec![OpEntry {
            uuid: "00000000-0000-0000-0000-000000000099".into(),
            name: "MedievalKnightCommander".into(),
            level: 4,
            bypasses_player_limit: true,
        }];
        app.servers.whitelist = vec![WhitelistEntry {
            uuid: "00000000-0000-0000-0000-000000000099".into(),
            name: "MedievalKnightCommander".into(),
        }];
        app.servers.banlist = vec![BannedPlayerEntry {
            uuid: "00000000-0000-0000-0000-000000000088".into(),
            name: "GrieferWithVeryLongUsernameAccount".into(),
            created: "2026-11-08".into(),
            source: "Server".into(),
            expires: "forever".into(),
            reason: "Repeated griefing in spawn citadel".into(),
        }];
        app.servers.banned_ips = vec![BannedIpEntry {
            ip: "203.0.113.240".into(),
            created: "2026-11-08".into(),
            source: "Server".into(),
            expires: "forever".into(),
            reason: "Automated bot flood".into(),
        }];
        app.servers.addon_kind = ContentKind::Mod;
        app.servers.server_tunnels.insert(
            srv_id,
            TunnelStatus::Running {
                local_port: 25565,
                remote_addr: "bore.pub:25435".into(),
                remote_port: 25435,
            },
        );

        let all_tabs = [
            ServerTab::Console,
            ServerTab::Files,
            ServerTab::PluginsMods,
            ServerTab::Properties,
            ServerTab::AccessControl,
            ServerTab::Network,
        ];
        let viewports = [
            ("xiaomi_portrait_393", 393.0_f32, 852.0_f32, true),
            ("narrow_portrait_360", 360.0_f32, 800.0_f32, true),
            ("xiaomi_landscape_852", 852.0_f32, 393.0_f32, false),
            ("desktop_compact_720", 720.0_f32, 640.0_f32, false),
        ];

        for &(vp_name, phys_w, phys_h, is_portrait) in &viewports {
            if is_portrait {
                app.android_permissions.safe_inset_left_px = 0;
                app.android_permissions.safe_inset_right_px = 0;
                app.android_permissions.safe_inset_top_px = 47;
                app.android_permissions.safe_inset_bottom_px = 19;
            } else {
                app.android_permissions.safe_inset_left_px = 47;
                app.android_permissions.safe_inset_right_px = 19;
                app.android_permissions.safe_inset_top_px = 0;
                app.android_permissions.safe_inset_bottom_px = 16;
            }

            let scales: &[f32] = if vp_name == "xiaomi_portrait_393" {
                &[1.0, 1.35, 2.0]
            } else {
                &[1.0, 1.35]
            };

            for &scale in scales {
                app.settings.font_scale = scale;
                for &tab in &all_tabs {
                    app.servers.active_tab = tab;
                    app.servers.editing_file_rel = None;

                    let props_tab_lbl = app.i18n.t("server.tab.properties").to_owned();
                    let safe_rect = std::cell::Cell::new(Rect::NOTHING);
                    let eff_ppp = std::cell::Cell::new(1.0_f32);
                    let h = harness_scaled(
                        phys_w / scale,
                        phys_h / scale,
                        scale,
                        ThemeMode::Light,
                        |ui| {
                            eff_ppp.set(ui.ctx().pixels_per_point());
                            safe_rect.set(crate::views::safe_content_rect(ui.ctx()));
                            crate::views::render_root_ui(app, ui, None);
                        },
                    );

                    let sr = safe_rect.get();
                    let ppp = eff_ppp.get();
                    let ctx_desc = format!("{vp_name}@{scale} tab={tab:?}");
                    assert_subtree_within_horizontal_bounds(h.root(), 0.0, phys_w, &ctx_desc);

                    if let Some(tab_node) =
                        h.query_by_role_and_label(egui::Role::Button, &props_tab_lbl)
                    {
                        let tr = tab_node.rect();
                        assert!(
                            tr.left() >= sr.left() * ppp - 2.5
                                && tr.right() <= sr.right() * ppp + 2.5,
                            "Properties tab pill clipped in {ctx_desc}: {tr:?} vs safe={sr:?} (ppp={ppp})"
                        );
                    }
                    drop(h);
                }
            }
        }

        // Also verify inline file editor on narrow portrait stays strictly within horizontal bounds
        app.android_permissions.safe_inset_left_px = 0;
        app.android_permissions.safe_inset_right_px = 0;
        app.android_permissions.safe_inset_top_px = 47;
        app.android_permissions.safe_inset_bottom_px = 19;
        app.settings.font_scale = 1.0;
        app.servers.active_tab = ServerTab::Files;
        app.servers.editing_file_rel = Some(
            "config/fancymenu/customization/extremely_long_medieval_layout_configuration.txt"
                .into(),
        );
        app.servers.editing_file_text = "server-ip=0.0.0.0\nmotd=Cisco Medieval RPG\n".into();
        let file_content_rect = std::cell::Cell::new(Rect::NOTHING);
        let file_h = harness_scaled(393.0, 852.0, 1.0, ThemeMode::Light, |ui| {
            file_content_rect.set(ui.ctx().content_rect());
            crate::views::render_root_ui(app, ui, None);
        });
        let fcr = file_content_rect.get();
        assert_subtree_within_horizontal_bounds(
            file_h.root(),
            fcr.left(),
            fcr.right(),
            "xiaomi_portrait_393 inline_file_editor",
        );
        drop(file_h);
        app.servers.editing_file_rel = None;

        // Verify persisted off-safe-area modal window is clamped into safe_content_rect (never overlaps camera cutout)
        app.show_settings_modal = true;
        let settings_win_title = app
            .i18n
            .tr("Cài đặt Hệ thống & Cập nhật", "System Settings & Updates")
            .to_owned();
        let mut modal_safe_rect = Rect::NOTHING;
        let mut modal_seeded = false;
        let modal_h = harness_scaled(393.0, 852.0, 1.0, ThemeMode::Light, |ui| {
            if !modal_seeded {
                modal_seeded = true;
                let win_id = egui::Id::unique("thk_modal_settings");
                ui.ctx().data_mut(|d| {
                    let area = egui::AreaState {
                        pivot_pos: Some(pos2(0.0, 2.0)),
                        ..Default::default()
                    };
                    d.insert_persisted(win_id, area);
                });
            }
            crate::views::render_root_ui(app, ui, None);
            modal_safe_rect = crate::views::safe_content_rect(ui.ctx());
        });
        let win_rect = modal_h
            .get_by_role_and_label(egui::Role::Window, &settings_win_title)
            .rect();
        assert_subtree_within_horizontal_bounds(
            modal_h.root(),
            0.0,
            393.0,
            "xiaomi_portrait_393 settings_modal",
        );
        drop(modal_h);
        assert!(
            win_rect.top() >= modal_safe_rect.top() - 1.5
                && win_rect.bottom() <= modal_safe_rect.bottom() + 1.5
                && win_rect.left() >= modal_safe_rect.left() - 1.5
                && win_rect.right() <= modal_safe_rect.right() + 1.5,
            "settings modal escaped safe_content_rect (camera cutout / gesture bar): win={win_rect:?} vs safe={modal_safe_rect:?}"
        );
    });
}

#[test]
fn servers_landscape_and_portrait_never_drift_or_jitter_across_frames() {
    use crate::state::ServerTab;
    use egui_kittest::kittest::Queryable;

    let _render_guard = VISUAL_RENDER_LOCK.lock().expect("visual renderer lock");
    with_isolated_app(Language::Vi, |app| {
        app.settings.setup_completed = true;
        app.settings.theme = ThemeMode::Light;
        app.section = crate::state::ActiveSection::Servers;
        app.android_permissions.safe_inset_left_px = 47;
        app.android_permissions.safe_inset_right_px = 19;
        app.android_permissions.safe_inset_top_px = 0;
        app.android_permissions.safe_inset_bottom_px = 16;

        let nav_servers_lbl = app.i18n.t("nav.servers").to_owned();
        let create_srv_lbl = app.i18n.t("server.create").to_owned();
        let compact_new_lbl = app.i18n.tr("Tạo", "New").to_owned();
        let start_srv_lbl = app.i18n.t("server.start").to_owned();
        let props_tab_lbl = app.i18n.t("server.properties").to_owned();

        // 1. Empty Servers view in Xiaomi Landscape (1093x505 @ fractional ppp=2.535 = 3.25 * 0.78)
        // where both LeftNavRail and ServersSidebar are active simultaneously.
        app.servers.servers.clear();
        app.servers.selected_server_id = None;
        let mut empty_h = Harness::builder()
            .with_size(vec2(1093.0, 505.0))
            .with_pixels_per_point(2.535)
            .with_theme(egui::Theme::Light)
            .with_os(egui::os::OperatingSystem::Android)
            .with_accessibility_check(false)
            .with_context_setup(|ctx| {
                crate::theme::install_unicode_fonts(ctx);
                crate::theme::apply_theme(ctx, ThemeMode::Light, AccentPreset::Emerald);
            })
            .build_ui(|ui| {
                crate::views::render_root_ui(app, ui, None);
            });

        let mut sim_t = 1.0_f64;
        for _ in 0..16 {
            sim_t += 1.0 / 60.0;
            empty_h.input_mut().time = Some(sim_t);
            empty_h.step();
        }
        let base_empty_nav = empty_h
            .get_by_role_and_label(egui::Role::Button, &nav_servers_lbl)
            .rect();
        let base_empty_btns: Vec<Rect> = empty_h
            .get_all_by_role_and_label(egui::Role::Button, &create_srv_lbl)
            .map(|n| n.rect())
            .collect();
        assert_eq!(base_empty_btns.len(), 2);
        assert!(
            base_empty_btns[1].left() > base_empty_btns[0].right() + 20.0
                && base_empty_btns[1].right() < 1093.0 - 16.0,
            "empty servers main CTA was pushed off-center/right: {base_empty_btns:?}"
        );
        for frame_idx in 0..24 {
            sim_t += 1.0 / 60.0;
            empty_h.input_mut().time = Some(sim_t);
            empty_h.step();
            let cur_nav = empty_h
                .get_by_role_and_label(egui::Role::Button, &nav_servers_lbl)
                .rect();
            let cur_btns: Vec<Rect> = empty_h
                .get_all_by_role_and_label(egui::Role::Button, &create_srv_lbl)
                .map(|n| n.rect())
                .collect();
            assert!(
                (cur_nav.left() - base_empty_nav.left()).abs() < 0.01
                    && (cur_nav.right() - base_empty_nav.right()).abs() < 0.01,
                "LeftNavRail drifted at frame {frame_idx}: {cur_nav:?} vs {base_empty_nav:?}"
            );
            for (b_idx, (cur_r, base_r)) in cur_btns.iter().zip(&base_empty_btns).enumerate() {
                assert!(
                    (cur_r.left() - base_r.left()).abs() < 0.01
                        && (cur_r.right() - base_r.right()).abs() < 0.01
                        && (cur_r.top() - base_r.top()).abs() < 0.01,
                    "Servers button[{b_idx}] drifted/jittered at frame {frame_idx}: {cur_r:?} vs {base_r:?}"
                );
            }
        }
        drop(empty_h);

        // 2. Populated Servers view across tabs in dual-sidebar Landscape (1093x505),
        // compact Landscape (852x393), and Portrait (393x852) under fractional ppp=2.535.
        let mut srv = server_fixture();
        srv.name = "Cisco's Fantasy Medieval RPG [Ultimate] Dedicated Server".into();
        let srv_id = srv.id.clone();
        app.servers.selected_server_id = Some(srv_id.clone());
        app.servers.servers = vec![srv];
        app.servers
            .server_logs
            .insert(srv_id, vec!["[12:00:00 INFO]: Done (2.1s)!".into()]);

        for (vp_w, vp_h, is_port, has_dual_sidebar) in [
            (1093.0_f32, 505.0_f32, false, true),
            (852.0_f32, 393.0_f32, false, false),
            (393.0_f32, 852.0_f32, true, false),
        ] {
            if is_port {
                app.android_permissions.safe_inset_left_px = 0;
                app.android_permissions.safe_inset_right_px = 0;
                app.android_permissions.safe_inset_top_px = 47;
                app.android_permissions.safe_inset_bottom_px = 19;
            } else {
                app.android_permissions.safe_inset_left_px = 47;
                app.android_permissions.safe_inset_right_px = 19;
                app.android_permissions.safe_inset_top_px = 0;
                app.android_permissions.safe_inset_bottom_px = 16;
            }

            let switcher_btn_lbl = if has_dual_sidebar {
                &create_srv_lbl
            } else {
                &compact_new_lbl
            };

            for tab in [
                ServerTab::Console,
                ServerTab::Properties,
                ServerTab::PluginsMods,
                ServerTab::Network,
            ] {
                app.servers.active_tab = tab;
                let mut h = Harness::builder()
                    .with_size(vec2(vp_w, vp_h))
                    .with_pixels_per_point(2.535)
                    .with_theme(egui::Theme::Light)
                    .with_os(egui::os::OperatingSystem::Android)
                    .with_accessibility_check(false)
                    .with_context_setup(|ctx| {
                        crate::theme::install_unicode_fonts(ctx);
                        crate::theme::apply_theme(ctx, ThemeMode::Light, AccentPreset::Emerald);
                    })
                    .build_ui(|ui| {
                        crate::views::render_root_ui(app, ui, None);
                    });

                let mut t = 2.0_f64;
                for _ in 0..16 {
                    t += 1.0 / 60.0;
                    h.input_mut().time = Some(t);
                    h.step();
                }

                let base_start = h
                    .get_by_role_and_label(egui::Role::Button, &start_srv_lbl)
                    .rect();
                let base_prop_tab = h
                    .get_by_role_and_label(egui::Role::Tab, &props_tab_lbl)
                    .rect();
                let base_create = h
                    .get_by_role_and_label(egui::Role::Button, switcher_btn_lbl)
                    .rect();

                if has_dual_sidebar {
                    assert!(
                        base_create.right() < vp_w * 0.55 && base_start.right() <= vp_w,
                        "Landscape Servers sidebar pushed too far right on tab {tab:?}: sidebar_btn={base_create:?}, start_btn={base_start:?}"
                    );
                }

                for frame_idx in 0..24 {
                    t += 1.0 / 60.0;
                    h.input_mut().time = Some(t);
                    h.step();

                    let cur_start = h
                        .get_by_role_and_label(egui::Role::Button, &start_srv_lbl)
                        .rect();
                    let cur_prop_tab = h
                        .get_by_role_and_label(egui::Role::Tab, &props_tab_lbl)
                        .rect();
                    let cur_create = h
                        .get_by_role_and_label(egui::Role::Button, switcher_btn_lbl)
                        .rect();

                    for (name, cur_r, base_r) in [
                        ("start_btn", cur_start, base_start),
                        ("props_tab", cur_prop_tab, base_prop_tab),
                        ("create_btn", cur_create, base_create),
                    ] {
                        assert!(
                            (cur_r.left() - base_r.left()).abs() < 0.01
                                && (cur_r.right() - base_r.right()).abs() < 0.01
                                && (cur_r.top() - base_r.top()).abs() < 0.01
                                && (cur_r.bottom() - base_r.bottom()).abs() < 0.01,
                            "Servers {name} drifted/jittered at {vp_w}x{vp_h} tab={tab:?} frame={frame_idx}: {cur_r:?} vs {base_r:?}"
                        );
                    }
                }
                drop(h);
            }
        }
    });
}
