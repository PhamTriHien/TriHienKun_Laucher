use crate::modrinth::{
    get_modrinth_project_with_base, get_modrinth_projects_batch_with_base,
    get_modrinth_version_by_id_with_base, get_modrinth_versions_for_loaders_with_base,
    MODRINTH_API_BASE,
};
use crate::provider::{DependencyRelation, ProjectSummary, ProjectVersion, VersionFile};
use chrono::Utc;
use futures_util::future::join_all;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use thk_core::models::{
    ContentKind, DownloadProgress, InstalledContentRecord, LoaderType, ProviderSource, ServerEngine,
};
use thk_core::paths::SafePath;
use thk_core::{Database, DownloadManager, DownloadTask, ThkError, ThkResult};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OptimizationTarget {
    Client(LoaderType),
    Server {
        engine: ServerEngine,
        mod_loader: Option<LoaderType>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptimizationCatalogEntry {
    pub project_id: &'static str,
    pub slug: &'static str,
    pub title: &'static str,
    pub content_kind: ContentKind,
    pub summary_vi: &'static str,
    pub summary_en: &'static str,
    pub recommended_by_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedOptimizationSuggestion {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub content_kind: ContentKind,
    pub summary_vi: String,
    pub summary_en: String,
    pub recommended_by_default: bool,
    pub project_summary: Option<ProjectSummary>,
    pub compatible_version: Option<ProjectVersion>,
    pub error: Option<String>,
}

#[derive(Clone, Copy)]
struct CatalogMeta {
    project_id: &'static str,
    slug: &'static str,
    title: &'static str,
    summary_vi: &'static str,
    summary_en: &'static str,
}

impl CatalogMeta {
    const fn entry(
        self,
        content_kind: ContentKind,
        recommended_by_default: bool,
    ) -> OptimizationCatalogEntry {
        OptimizationCatalogEntry {
            project_id: self.project_id,
            slug: self.slug,
            title: self.title,
            content_kind,
            summary_vi: self.summary_vi,
            summary_en: self.summary_en,
            recommended_by_default,
        }
    }
}

const FABRIC_API: CatalogMeta = CatalogMeta {
    project_id: "P7dR8mSH",
    slug: "fabric-api",
    title: "Fabric API",
    summary_vi: "Thư viện nền tảng bắt buộc cho hầu hết mod Fabric.",
    summary_en: "Essential core library required by most Fabric mods.",
};

const QSL: CatalogMeta = CatalogMeta {
    project_id: "qvIfYCYJ",
    slug: "qsl",
    title: "Quilted Fabric API / QSL",
    summary_vi: "Thư viện lõi giúp chạy mượt các mod Quilt và Fabric.",
    summary_en: "Core compatibility library for Quilt and Fabric mods.",
};

const SODIUM: CatalogMeta = CatalogMeta {
    project_id: "AANobbMI",
    slug: "sodium",
    title: "Sodium",
    summary_vi: "Tối ưu sâu bộ kết xuất đồ họa giúp tăng mạnh FPS.",
    summary_en: "Modern rendering engine rewrite for dramatic FPS gains.",
};

const EMBEDDIUM: CatalogMeta = CatalogMeta {
    project_id: "sk9rgfiA",
    slug: "embeddium",
    title: "Embeddium",
    summary_vi: "Tăng mạnh FPS và độ ổn định đồ họa trên Forge.",
    summary_en: "High-performance rendering optimization for Forge.",
};

const LITHIUM: CatalogMeta = CatalogMeta {
    project_id: "gvQqBUqZ",
    slug: "lithium",
    title: "Lithium",
    summary_vi: "Tối ưu vật lý, AI sinh vật và chu kỳ tick thế giới.",
    summary_en: "Optimizes game physics, mob AI, and server tick logic.",
};

const RADIUM: CatalogMeta = CatalogMeta {
    project_id: "2gvRmQXx",
    slug: "radium",
    title: "Radium",
    summary_vi: "Tối ưu vật lý, AI sinh vật và chu kỳ tick cho Forge.",
    summary_en: "Optimizes game physics, mob AI, and tick logic on Forge.",
};

const FERRITE_CORE: CatalogMeta = CatalogMeta {
    project_id: "uXXizFIs",
    slug: "ferrite-core",
    title: "FerriteCore",
    summary_vi: "Giảm đáng kể mức tiêu thụ RAM của Minecraft.",
    summary_en: "Significantly reduces Minecraft memory usage.",
};

const ENTITY_CULLING: CatalogMeta = CatalogMeta {
    project_id: "NNAgCjsB",
    slug: "entityculling",
    title: "Entity Culling",
    summary_vi: "Bỏ qua vẽ sinh vật và khối bị che khuất khỏi tầm nhìn.",
    summary_en: "Skips rendering hidden entities and block entities.",
};

const IMMEDIATELY_FAST: CatalogMeta = CatalogMeta {
    project_id: "5ZwdcRci",
    slug: "immediatelyfast",
    title: "ImmediatelyFast",
    summary_vi: "Tăng tốc độ vẽ giao diện, chữ, bản đồ và thực thể.",
    summary_en: "Speeds up rendering for HUD, text, maps, and entities.",
};

const MODERN_FIX: CatalogMeta = CatalogMeta {
    project_id: "nmDcB62a",
    slug: "modernfix",
    title: "ModernFix",
    summary_vi: "Giảm RAM, tăng tốc khởi động và sửa nhiều điểm nghẽn hiệu năng.",
    summary_en: "Reduces memory footprint, speeds up launch, and fixes performance bottlenecks.",
};

const DYNAMIC_FPS: CatalogMeta = CatalogMeta {
    project_id: "LQ3K71Q1",
    slug: "dynamic-fps",
    title: "Dynamic FPS",
    summary_vi: "Tự hạ mức sử dụng GPU và CPU khi cửa sổ game chạy nền.",
    summary_en: "Reduces GPU and CPU usage while the game window is unfocused.",
};

const KRYPTON: CatalogMeta = CatalogMeta {
    project_id: "fQEb0iXm",
    slug: "krypton",
    title: "Krypton",
    summary_vi: "Tối ưu hiệu năng xử lý và nén gói tin mạng Minecraft.",
    summary_en: "Optimizes the Minecraft networking stack and packet compression.",
};

const EBE: CatalogMeta = CatalogMeta {
    project_id: "OVuFYfre",
    slug: "ebe",
    title: "Enhanced Block Entities",
    summary_vi: "Tăng tốc kết xuất rương, biển hiệu và các khối đặc biệt.",
    summary_en: "Speeds up rendering for chests, signs, and block entities.",
};

const CLUMPS_MOD: CatalogMeta = CatalogMeta {
    project_id: "Wnxd13zP",
    slug: "clumps",
    title: "Clumps",
    summary_vi: "Gộp các quả cầu kinh nghiệm gần nhau để giảm giật lag.",
    summary_en: "Groups nearby XP orbs together to reduce entity lag.",
};

const CLUMPS_PLUGIN: CatalogMeta = CatalogMeta {
    project_id: "IahtO7tM",
    slug: "clumps-plugin",
    title: "Clumps",
    summary_vi: "Gộp các quả cầu kinh nghiệm gần nhau giúp giảm tải thực thể.",
    summary_en: "Groups nearby XP orbs together to reduce entity overhead.",
};

const CHUNKY: CatalogMeta = CatalogMeta {
    project_id: "fALzjamp",
    slug: "chunky",
    title: "Chunky",
    summary_vi: "Tạo sẵn các vùng bản đồ để tránh tụt TPS khi người chơi khám phá.",
    summary_en: "Pre-generates world chunks smoothly to prevent exploration lag.",
};

const LET_ME_DESPAWN: CatalogMeta = CatalogMeta {
    project_id: "vE2FN5qn",
    slug: "lmd",
    title: "Let Me Despawn",
    summary_vi: "Cho phép quái vật nhặt đồ tự biến mất đúng quy tắc tránh ứ đọng.",
    summary_en: "Allows mobs holding picked-up items to despawn normally.",
};

const CLEARLAG_PLUS_PLUS: CatalogMeta = CatalogMeta {
    project_id: "7bhTp28M",
    slug: "clearlag-plus-plus",
    title: "ClearLag++",
    summary_vi: "Dọn vật phẩm rơi và thực thể dư thừa giúp giữ ổn định TPS.",
    summary_en: "Cleans up excess dropped items and entities to keep TPS stable.",
};

const LAG_FIXER: CatalogMeta = CatalogMeta {
    project_id: "fD1X4Kwd",
    slug: "lagfixer",
    title: "LagFixer",
    summary_vi: "Phát hiện và giảm tải các nguyên nhân gây lag phổ biến trên máy chủ.",
    summary_en: "Detects and mitigates common server lag sources.",
};

const VILLAGER_OPTIMIZER: CatalogMeta = CatalogMeta {
    project_id: "zxzjuSWi",
    slug: "villageroptimizer",
    title: "VillagerOptimizer",
    summary_vi: "Giảm tải AI tìm đường cho dân làng trong khu vực buôn bán.",
    summary_en: "Reduces AI pathfinding overhead from trading-hall villagers.",
};

const SLUMBER: CatalogMeta = CatalogMeta {
    project_id: "ksm6XRZ9",
    slug: "slumber",
    title: "Slumber",
    summary_vi: "Giảm xử lý thừa tại các vùng thế giới trống không có người chơi.",
    summary_en: "Reduces idle tick overhead in empty world regions.",
};

const POTATOPTIMIZE: CatalogMeta = CatalogMeta {
    project_id: "tCUVz9xy",
    slug: "potatoptimize",
    title: "Potatoptimize",
    summary_vi: "Tối ưu mật độ sinh vật và vật phẩm rơi cho máy chủ Sponge.",
    summary_en: "Optimizes mob density and dropped items on Sponge servers.",
};

const SLEDGEHAMMER: CatalogMeta = CatalogMeta {
    project_id: "QItC3hpi",
    slug: "sledgehammer",
    title: "Sledgehammer",
    summary_vi: "Sửa các lỗi ổn định và hiệu năng thường gặp trên Sponge.",
    summary_en: "Fixes common stability and performance issues on Sponge.",
};

const SERVER_CORE: CatalogMeta = CatalogMeta {
    project_id: "4WWQxlQP",
    slug: "servercore",
    title: "ServerCore",
    summary_vi: "Tự động điều chỉnh tầm nhìn và nhịp tick giữ TPS ổn định.",
    summary_en: "Dynamically tunes view distance and tick pacing to preserve TPS.",
};

const C2ME_FABRIC: CatalogMeta = CatalogMeta {
    project_id: "VSNURh3q",
    slug: "c2me-fabric",
    title: "C2ME (Fabric)",
    summary_vi: "Tăng tốc tải và tạo chunk song song trên nhiều nhân CPU.",
    summary_en: "Speeds up chunk generation and loading across multiple CPU cores.",
};

const C2ME_NEOFORGE: CatalogMeta = CatalogMeta {
    project_id: "COlSi5iR",
    slug: "c2me-neoforge",
    title: "C2ME (NeoForge)",
    summary_vi: "Tăng tốc tải và tạo chunk song song trên nhiều nhân CPU.",
    summary_en: "Speeds up chunk generation and loading across multiple CPU cores.",
};

const NOISIUM: CatalogMeta = CatalogMeta {
    project_id: "KuNKN7d2",
    slug: "noisium",
    title: "Noisium",
    summary_vi: "Tăng tốc quá trình tính toán địa hình khi tạo thế giới mới.",
    summary_en: "Speeds up terrain noise calculations during world generation.",
};

const SPARK: CatalogMeta = CatalogMeta {
    project_id: "l6YH9Als",
    slug: "spark",
    title: "spark",
    summary_vi: "Công cụ đo hiệu năng giúp tìm chính xác nguyên nhân gây tụt TPS.",
    summary_en: "Performance profiler for pinpointing TPS drops and lag spikes.",
};

const ALTERNATE_CURRENT: CatalogMeta = CatalogMeta {
    project_id: "r0v8vy1s",
    slug: "alternate-current",
    title: "Alternate Current",
    summary_vi: "Giảm mạnh lag từ các hệ thống mạch Redstone phức tạp.",
    summary_en: "Drastically reduces lag from complex Redstone circuits.",
};

const SODIUM_EXTRA: CatalogMeta = CatalogMeta {
    project_id: "PtjYWJkn",
    slug: "sodium-extra",
    title: "Sodium Extra",
    summary_vi: "Bổ sung tùy chỉnh đồ họa, hạt hiệu ứng và hiển thị FPS cho Sodium.",
    summary_en: "Adds extra graphics, particle controls, and FPS overlay options for Sodium.",
};

const REESES_SODIUM_OPTIONS: CatalogMeta = CatalogMeta {
    project_id: "Bh37bMuy",
    slug: "reeses-sodium-options",
    title: "Reese's Sodium Options",
    summary_vi: "Sắp xếp lại bảng cài đặt hình ảnh Sodium gọn và dễ tìm hơn.",
    summary_en: "Reorganizes Sodium video settings into a cleaner scrollable layout.",
};

const MORE_CULLING: CatalogMeta = CatalogMeta {
    project_id: "51shyZVL",
    slug: "moreculling",
    title: "More Culling",
    summary_vi: "Giảm số mặt khối và vật thể không nhìn thấy để tăng FPS.",
    summary_en: "Cuts unnecessary hidden block faces and models to boost FPS.",
};

const BAD_OPTIMIZATIONS: CatalogMeta = CatalogMeta {
    project_id: "g96Z4WVZ",
    slug: "badoptimizations",
    title: "BadOptimizations",
    summary_vi: "Tối ưu hàng loạt các phép tính nhỏ giúp khung hình mượt hơn.",
    summary_en: "Collection of lightweight client micro-optimizations for smoother frames.",
};

const RRLS: CatalogMeta = CatalogMeta {
    project_id: "ZP7xHXtw",
    slug: "rrls",
    title: "Remove Reloading Screen",
    summary_vi: "Tải gói tài nguyên ngầm để không bị kẹt màn hình chờ.",
    summary_en: "Loads resource packs in the background without blocking the screen.",
};

const FASTQUIT: CatalogMeta = CatalogMeta {
    project_id: "x1hIzbuY",
    slug: "fastquit",
    title: "FastQuit",
    summary_vi: "Cho phép quay lại menu ngay trong khi thế giới lưu ngầm.",
    summary_en: "Lets you return to the title screen immediately while worlds save.",
};

const FAST_IP_PING: CatalogMeta = CatalogMeta {
    project_id: "9mtu0sUO",
    slug: "fast-ip-ping",
    title: "Fast IP Ping",
    summary_vi: "Rút ngắn thời gian hiển thị danh sách máy chủ nhiều người chơi.",
    summary_en: "Speeds up multiplayer server list pings and reduces menu freeze.",
};

const MEMORY_LEAK_FIX: CatalogMeta = CatalogMeta {
    project_id: "NRjRiSSD",
    slug: "memoryleakfix",
    title: "Memory Leak Fix",
    summary_vi: "Khắc phục các lỗi rò rỉ bộ nhớ giúp giảm giật sau thời gian dài.",
    summary_en: "Fixes memory leaks to keep RAM usage stable during long sessions.",
};

const SCALABLE_LUX: CatalogMeta = CatalogMeta {
    project_id: "Ps1zyz6x",
    slug: "scalablelux",
    title: "ScalableLux",
    summary_vi: "Tăng tốc tính toán ánh sáng đa nhân khi tạo và cập nhật thế giới.",
    summary_en: "Multi-threaded lighting engine optimization for chunk loading and updates.",
};

const VMP_FABRIC: CatalogMeta = CatalogMeta {
    project_id: "wnEe9KBa",
    slug: "vmp-fabric",
    title: "Very Many Players",
    summary_vi: "Giữ TPS ổn định khi máy chủ có đông người chơi cùng lúc.",
    summary_en: "Improves server TPS and stability when many players are online.",
};

const SATURN: CatalogMeta = CatalogMeta {
    project_id: "2eT495vq",
    slug: "saturn",
    title: "Saturn",
    summary_vi: "Giảm dung lượng RAM tiêu thụ bằng tối ưu cấu trúc bộ nhớ.",
    summary_en: "Lowers RAM usage by optimizing internal memory structures.",
};

const STARLIGHT_FORGE: CatalogMeta = CatalogMeta {
    project_id: "iRfIGC1s",
    slug: "starlight-forge",
    title: "Starlight (Forge)",
    summary_vi: "Tăng tốc bộ xử lý ánh sáng giúp tải và sinh vùng bản đồ nhanh hơn.",
    summary_en: "Rewrites the light engine for faster chunk loading and lighting updates.",
};

const AI_IMPROVEMENTS: CatalogMeta = CatalogMeta {
    project_id: "DSVgwcji",
    slug: "ai-improvements",
    title: "AI Improvements",
    summary_vi: "Giảm tải xử lý AI tìm đường và quan sát của sinh vật trên máy chủ.",
    summary_en: "Reduces mob AI pathfinding and look-logic overhead on servers.",
};

const SEE_MORE: CatalogMeta = CatalogMeta {
    project_id: "IEt1Yy3F",
    slug: "seemore",
    title: "SeeMore",
    summary_vi: "Mở rộng tầm nhìn thực tế cho người chơi mà ít ảnh hưởng TPS.",
    summary_en: "Extends visible player view distance with minimal TPS impact.",
};

const FAST_LEAF_DECAY: CatalogMeta = CatalogMeta {
    project_id: "PcKMtamx",
    slug: "fast-leaf-decay",
    title: "Fast Leaf Decay",
    summary_vi: "Làm lá cây biến mất nhanh và nhẹ hơn sau khi chặt gỗ.",
    summary_en: "Speeds up leaf decay after chopping trees while reducing block ticks.",
};

const TRADE_CYCLE: CatalogMeta = CatalogMeta {
    project_id: "GpyBM69F",
    slug: "tradecycle",
    title: "Trade Cycle",
    summary_vi: "Đổi giao dịch dân làng bằng nút bấm thay vì phá và đặt khối liên tục.",
    summary_en: "Cycles villager trades cleanly without repeated block breaking lag.",
};

const FARM_CONTROL: CatalogMeta = CatalogMeta {
    project_id: "rRqO39TI",
    slug: "farmcontrol",
    title: "FarmControl",
    summary_vi: "Tự động giới hạn các trang trại sinh vật gây tụt TPS máy chủ.",
    summary_en: "Automatically throttles lag-heavy mob farms to protect server TPS.",
};

const STACK_MOB: CatalogMeta = CatalogMeta {
    project_id: "EqaioisK",
    slug: "stackmob",
    title: "StackMob",
    summary_vi: "Gộp các sinh vật giống nhau đứng gần để giảm tải thực thể.",
    summary_en: "Stacks nearby identical mobs together to reduce entity lag.",
};

const TABTPS: CatalogMeta = CatalogMeta {
    project_id: "cUhi3iB2",
    slug: "tabtps",
    title: "TabTPS",
    summary_vi: "Hiển thị trực tiếp TPS, MSPT và độ trễ trên bảng người chơi.",
    summary_en: "Displays live TPS, MSPT, and ping directly in the player tab list.",
};

pub fn target_supports_optimizations(target: OptimizationTarget) -> bool {
    !matches!(
        target,
        OptimizationTarget::Client(LoaderType::Vanilla)
            | OptimizationTarget::Server {
                engine: ServerEngine::Vanilla,
                ..
            }
    )
}

pub fn loaders_for_optimization_entry(
    target: OptimizationTarget,
    kind: ContentKind,
) -> Vec<&'static str> {
    match (target, kind) {
        (OptimizationTarget::Client(LoaderType::Vanilla), ContentKind::Mod) => Vec::new(),
        (OptimizationTarget::Client(LoaderType::Quilt), ContentKind::Mod) => {
            vec!["quilt", "fabric"]
        }
        (OptimizationTarget::Client(ldr), ContentKind::Mod) => vec![ldr.as_str()],
        (
            OptimizationTarget::Server {
                engine: ServerEngine::Paper | ServerEngine::Mohist | ServerEngine::Arclight,
                ..
            },
            ContentKind::Plugin,
        ) => vec!["paper", "spigot", "bukkit"],
        (
            OptimizationTarget::Server {
                engine: ServerEngine::Purpur,
                ..
            },
            ContentKind::Plugin,
        ) => vec!["purpur", "paper", "spigot", "bukkit"],
        (
            OptimizationTarget::Server {
                engine: ServerEngine::Folia,
                ..
            },
            ContentKind::Plugin,
        ) => vec!["folia"],
        (
            OptimizationTarget::Server {
                engine: ServerEngine::Sponge,
                ..
            },
            ContentKind::Plugin,
        ) => vec!["sponge"],
        (
            OptimizationTarget::Server {
                engine: ServerEngine::Fabric,
                ..
            },
            ContentKind::Mod,
        ) => vec!["fabric"],
        (
            OptimizationTarget::Server {
                engine: ServerEngine::NeoForge,
                ..
            },
            ContentKind::Mod,
        ) => vec!["neoforge"],
        (
            OptimizationTarget::Server {
                engine: ServerEngine::Forge | ServerEngine::Mohist,
                ..
            },
            ContentKind::Mod,
        ) => vec!["forge"],
        (
            OptimizationTarget::Server {
                engine: ServerEngine::Arclight,
                mod_loader,
            },
            ContentKind::Mod,
        ) => {
            let resolved = mod_loader
                .filter(|l| *l != LoaderType::Vanilla)
                .unwrap_or(LoaderType::NeoForge);
            vec![resolved.as_str()]
        }
        _ => Vec::new(),
    }
}

pub fn catalog_for_optimization_target(
    target: OptimizationTarget,
) -> Vec<OptimizationCatalogEntry> {
    match target {
        OptimizationTarget::Client(LoaderType::Vanilla) => Vec::new(),
        OptimizationTarget::Client(LoaderType::Fabric) => vec![
            FABRIC_API.entry(ContentKind::Mod, true),
            SODIUM.entry(ContentKind::Mod, true),
            LITHIUM.entry(ContentKind::Mod, true),
            FERRITE_CORE.entry(ContentKind::Mod, true),
            ENTITY_CULLING.entry(ContentKind::Mod, true),
            IMMEDIATELY_FAST.entry(ContentKind::Mod, true),
            MODERN_FIX.entry(ContentKind::Mod, true),
            SODIUM_EXTRA.entry(ContentKind::Mod, false),
            REESES_SODIUM_OPTIONS.entry(ContentKind::Mod, false),
            MORE_CULLING.entry(ContentKind::Mod, false),
            BAD_OPTIMIZATIONS.entry(ContentKind::Mod, false),
            EBE.entry(ContentKind::Mod, false),
            RRLS.entry(ContentKind::Mod, false),
            DYNAMIC_FPS.entry(ContentKind::Mod, false),
            FASTQUIT.entry(ContentKind::Mod, false),
            KRYPTON.entry(ContentKind::Mod, false),
            NOISIUM.entry(ContentKind::Mod, false),
            CLUMPS_MOD.entry(ContentKind::Mod, false),
            FAST_IP_PING.entry(ContentKind::Mod, false),
        ],
        OptimizationTarget::Client(LoaderType::Quilt) => vec![
            QSL.entry(ContentKind::Mod, true),
            SODIUM.entry(ContentKind::Mod, true),
            LITHIUM.entry(ContentKind::Mod, true),
            FERRITE_CORE.entry(ContentKind::Mod, true),
            ENTITY_CULLING.entry(ContentKind::Mod, true),
            IMMEDIATELY_FAST.entry(ContentKind::Mod, true),
            MODERN_FIX.entry(ContentKind::Mod, true),
            SODIUM_EXTRA.entry(ContentKind::Mod, false),
            REESES_SODIUM_OPTIONS.entry(ContentKind::Mod, false),
            MORE_CULLING.entry(ContentKind::Mod, false),
            BAD_OPTIMIZATIONS.entry(ContentKind::Mod, false),
            EBE.entry(ContentKind::Mod, false),
            RRLS.entry(ContentKind::Mod, false),
            DYNAMIC_FPS.entry(ContentKind::Mod, false),
            FASTQUIT.entry(ContentKind::Mod, false),
            KRYPTON.entry(ContentKind::Mod, false),
            NOISIUM.entry(ContentKind::Mod, false),
            CLUMPS_MOD.entry(ContentKind::Mod, false),
            FAST_IP_PING.entry(ContentKind::Mod, false),
        ],
        OptimizationTarget::Client(LoaderType::NeoForge) => vec![
            SODIUM.entry(ContentKind::Mod, true),
            LITHIUM.entry(ContentKind::Mod, true),
            FERRITE_CORE.entry(ContentKind::Mod, true),
            ENTITY_CULLING.entry(ContentKind::Mod, true),
            IMMEDIATELY_FAST.entry(ContentKind::Mod, true),
            MODERN_FIX.entry(ContentKind::Mod, true),
            SODIUM_EXTRA.entry(ContentKind::Mod, false),
            REESES_SODIUM_OPTIONS.entry(ContentKind::Mod, false),
            MORE_CULLING.entry(ContentKind::Mod, false),
            BAD_OPTIMIZATIONS.entry(ContentKind::Mod, false),
            RRLS.entry(ContentKind::Mod, false),
            CLUMPS_MOD.entry(ContentKind::Mod, false),
            DYNAMIC_FPS.entry(ContentKind::Mod, false),
            NOISIUM.entry(ContentKind::Mod, false),
            FAST_IP_PING.entry(ContentKind::Mod, false),
            SCALABLE_LUX.entry(ContentKind::Mod, false),
        ],
        OptimizationTarget::Client(LoaderType::Forge) => vec![
            EMBEDDIUM.entry(ContentKind::Mod, true),
            RADIUM.entry(ContentKind::Mod, true),
            FERRITE_CORE.entry(ContentKind::Mod, true),
            ENTITY_CULLING.entry(ContentKind::Mod, true),
            IMMEDIATELY_FAST.entry(ContentKind::Mod, true),
            MODERN_FIX.entry(ContentKind::Mod, true),
            SATURN.entry(ContentKind::Mod, false),
            BAD_OPTIMIZATIONS.entry(ContentKind::Mod, false),
            CLUMPS_MOD.entry(ContentKind::Mod, false),
            DYNAMIC_FPS.entry(ContentKind::Mod, false),
            NOISIUM.entry(ContentKind::Mod, false),
            FAST_IP_PING.entry(ContentKind::Mod, false),
            STARLIGHT_FORGE.entry(ContentKind::Mod, false),
            MEMORY_LEAK_FIX.entry(ContentKind::Mod, false),
        ],
        OptimizationTarget::Server {
            engine: ServerEngine::Vanilla,
            ..
        } => Vec::new(),
        OptimizationTarget::Server {
            engine: ServerEngine::Paper | ServerEngine::Purpur,
            ..
        } => vec![
            CHUNKY.entry(ContentKind::Plugin, true),
            LET_ME_DESPAWN.entry(ContentKind::Plugin, true),
            CLUMPS_PLUGIN.entry(ContentKind::Plugin, true),
            SEE_MORE.entry(ContentKind::Plugin, false),
            FAST_LEAF_DECAY.entry(ContentKind::Plugin, false),
            TRADE_CYCLE.entry(ContentKind::Plugin, false),
            FARM_CONTROL.entry(ContentKind::Plugin, false),
            STACK_MOB.entry(ContentKind::Plugin, false),
            TABTPS.entry(ContentKind::Plugin, false),
            CLEARLAG_PLUS_PLUS.entry(ContentKind::Plugin, false),
            LAG_FIXER.entry(ContentKind::Plugin, false),
            VILLAGER_OPTIMIZER.entry(ContentKind::Plugin, false),
        ],
        OptimizationTarget::Server {
            engine: ServerEngine::Folia,
            ..
        } => vec![
            CHUNKY.entry(ContentKind::Plugin, true),
            LET_ME_DESPAWN.entry(ContentKind::Plugin, true),
            CLUMPS_PLUGIN.entry(ContentKind::Plugin, true),
            SEE_MORE.entry(ContentKind::Plugin, false),
            FAST_LEAF_DECAY.entry(ContentKind::Plugin, false),
            TRADE_CYCLE.entry(ContentKind::Plugin, false),
            FARM_CONTROL.entry(ContentKind::Plugin, false),
            STACK_MOB.entry(ContentKind::Plugin, false),
            VILLAGER_OPTIMIZER.entry(ContentKind::Plugin, false),
            CLEARLAG_PLUS_PLUS.entry(ContentKind::Plugin, false),
            LAG_FIXER.entry(ContentKind::Plugin, false),
        ],
        OptimizationTarget::Server {
            engine: ServerEngine::Sponge,
            ..
        } => vec![
            CHUNKY.entry(ContentKind::Plugin, true),
            TABTPS.entry(ContentKind::Plugin, true),
            SLUMBER.entry(ContentKind::Plugin, true),
            POTATOPTIMIZE.entry(ContentKind::Plugin, false),
            SLEDGEHAMMER.entry(ContentKind::Plugin, false),
        ],
        OptimizationTarget::Server {
            engine: ServerEngine::Fabric,
            ..
        } => vec![
            FABRIC_API.entry(ContentKind::Mod, true),
            LITHIUM.entry(ContentKind::Mod, true),
            FERRITE_CORE.entry(ContentKind::Mod, true),
            SERVER_CORE.entry(ContentKind::Mod, true),
            KRYPTON.entry(ContentKind::Mod, true),
            VMP_FABRIC.entry(ContentKind::Mod, false),
            C2ME_FABRIC.entry(ContentKind::Mod, false),
            NOISIUM.entry(ContentKind::Mod, false),
            SCALABLE_LUX.entry(ContentKind::Mod, false),
            ALTERNATE_CURRENT.entry(ContentKind::Mod, false),
            CLUMPS_MOD.entry(ContentKind::Mod, false),
            LET_ME_DESPAWN.entry(ContentKind::Mod, false),
            CHUNKY.entry(ContentKind::Mod, false),
            TABTPS.entry(ContentKind::Mod, false),
            SPARK.entry(ContentKind::Mod, false),
        ],
        OptimizationTarget::Server {
            engine: ServerEngine::NeoForge,
            ..
        } => vec![
            LITHIUM.entry(ContentKind::Mod, true),
            FERRITE_CORE.entry(ContentKind::Mod, true),
            SERVER_CORE.entry(ContentKind::Mod, true),
            MODERN_FIX.entry(ContentKind::Mod, true),
            C2ME_NEOFORGE.entry(ContentKind::Mod, false),
            NOISIUM.entry(ContentKind::Mod, false),
            SCALABLE_LUX.entry(ContentKind::Mod, false),
            ALTERNATE_CURRENT.entry(ContentKind::Mod, false),
            CLUMPS_MOD.entry(ContentKind::Mod, false),
            LET_ME_DESPAWN.entry(ContentKind::Mod, false),
            SATURN.entry(ContentKind::Mod, false),
            AI_IMPROVEMENTS.entry(ContentKind::Mod, false),
            CHUNKY.entry(ContentKind::Mod, false),
            TABTPS.entry(ContentKind::Mod, false),
            SPARK.entry(ContentKind::Mod, false),
        ],
        OptimizationTarget::Server {
            engine: ServerEngine::Forge,
            ..
        } => vec![
            RADIUM.entry(ContentKind::Mod, true),
            FERRITE_CORE.entry(ContentKind::Mod, true),
            MODERN_FIX.entry(ContentKind::Mod, true),
            SERVER_CORE.entry(ContentKind::Mod, true),
            ALTERNATE_CURRENT.entry(ContentKind::Mod, false),
            CLUMPS_MOD.entry(ContentKind::Mod, false),
            LET_ME_DESPAWN.entry(ContentKind::Mod, false),
            SATURN.entry(ContentKind::Mod, false),
            AI_IMPROVEMENTS.entry(ContentKind::Mod, false),
            NOISIUM.entry(ContentKind::Mod, false),
            STARLIGHT_FORGE.entry(ContentKind::Mod, false),
            MEMORY_LEAK_FIX.entry(ContentKind::Mod, false),
            CHUNKY.entry(ContentKind::Mod, false),
            SPARK.entry(ContentKind::Mod, false),
        ],
        OptimizationTarget::Server {
            engine: ServerEngine::Mohist,
            ..
        } => vec![
            RADIUM.entry(ContentKind::Mod, true),
            FERRITE_CORE.entry(ContentKind::Mod, true),
            MODERN_FIX.entry(ContentKind::Mod, true),
            SERVER_CORE.entry(ContentKind::Mod, true),
            ALTERNATE_CURRENT.entry(ContentKind::Mod, false),
            SATURN.entry(ContentKind::Mod, false),
            AI_IMPROVEMENTS.entry(ContentKind::Mod, false),
            SPARK.entry(ContentKind::Mod, false),
            CHUNKY.entry(ContentKind::Plugin, true),
            LET_ME_DESPAWN.entry(ContentKind::Plugin, true),
            CLUMPS_PLUGIN.entry(ContentKind::Plugin, false),
            SEE_MORE.entry(ContentKind::Plugin, false),
            FAST_LEAF_DECAY.entry(ContentKind::Plugin, false),
            CLEARLAG_PLUS_PLUS.entry(ContentKind::Plugin, false),
            LAG_FIXER.entry(ContentKind::Plugin, false),
        ],
        OptimizationTarget::Server {
            engine: ServerEngine::Arclight,
            mod_loader,
        } => {
            let mut items = match mod_loader {
                Some(LoaderType::Fabric) => vec![
                    FABRIC_API.entry(ContentKind::Mod, true),
                    LITHIUM.entry(ContentKind::Mod, true),
                    FERRITE_CORE.entry(ContentKind::Mod, true),
                    SERVER_CORE.entry(ContentKind::Mod, true),
                    KRYPTON.entry(ContentKind::Mod, false),
                    VMP_FABRIC.entry(ContentKind::Mod, false),
                    C2ME_FABRIC.entry(ContentKind::Mod, false),
                    NOISIUM.entry(ContentKind::Mod, false),
                    ALTERNATE_CURRENT.entry(ContentKind::Mod, false),
                    SPARK.entry(ContentKind::Mod, false),
                ],
                Some(LoaderType::NeoForge) => vec![
                    LITHIUM.entry(ContentKind::Mod, true),
                    FERRITE_CORE.entry(ContentKind::Mod, true),
                    SERVER_CORE.entry(ContentKind::Mod, true),
                    MODERN_FIX.entry(ContentKind::Mod, true),
                    C2ME_NEOFORGE.entry(ContentKind::Mod, false),
                    NOISIUM.entry(ContentKind::Mod, false),
                    ALTERNATE_CURRENT.entry(ContentKind::Mod, false),
                    AI_IMPROVEMENTS.entry(ContentKind::Mod, false),
                    SPARK.entry(ContentKind::Mod, false),
                ],
                _ => vec![
                    RADIUM.entry(ContentKind::Mod, true),
                    FERRITE_CORE.entry(ContentKind::Mod, true),
                    MODERN_FIX.entry(ContentKind::Mod, true),
                    SERVER_CORE.entry(ContentKind::Mod, true),
                    ALTERNATE_CURRENT.entry(ContentKind::Mod, false),
                    SATURN.entry(ContentKind::Mod, false),
                    AI_IMPROVEMENTS.entry(ContentKind::Mod, false),
                    SPARK.entry(ContentKind::Mod, false),
                ],
            };
            items.extend([
                CHUNKY.entry(ContentKind::Plugin, true),
                LET_ME_DESPAWN.entry(ContentKind::Plugin, true),
                CLUMPS_PLUGIN.entry(ContentKind::Plugin, false),
                SEE_MORE.entry(ContentKind::Plugin, false),
                FAST_LEAF_DECAY.entry(ContentKind::Plugin, false),
            ]);
            items
        }
    }
}

fn select_primary_version_file(ver: &ProjectVersion) -> Option<&VersionFile> {
    let file = ver
        .files
        .iter()
        .find(|f| f.primary)
        .or_else(|| ver.files.first())?;
    if file.url.trim().is_empty() || SafePath::validate_single_name(&file.filename).is_err() {
        return None;
    }
    Some(file)
}

fn validate_primary_version_file<'a>(
    ver: &'a ProjectVersion,
    label: &str,
) -> ThkResult<&'a VersionFile> {
    let file = ver
        .files
        .iter()
        .find(|f| f.primary)
        .or_else(|| ver.files.first())
        .ok_or_else(|| ThkError::Provider(format!("No downloadable file for {label}")))?;
    if file.url.trim().is_empty() {
        return Err(ThkError::Provider(format!(
            "Empty download URL for {label}"
        )));
    }
    SafePath::validate_single_name(&file.filename)?;
    Ok(file)
}

fn best_loader_rank(ver: &ProjectVersion, clean_loaders: &[&str]) -> Option<usize> {
    if clean_loaders.is_empty() {
        return Some(0);
    }
    clean_loaders.iter().position(|allowed| {
        ver.loaders
            .iter()
            .any(|vl| vl.trim().eq_ignore_ascii_case(allowed))
    })
}

fn version_matches_loaders(ver: &ProjectVersion, clean_loaders: &[&str]) -> bool {
    best_loader_rank(ver, clean_loaders).is_some()
}

pub fn select_best_compatible_version(
    versions: &[ProjectVersion],
    game_version: &str,
    allowed_loaders: &[&str],
) -> Option<ProjectVersion> {
    let clean_gv = game_version.trim();
    if clean_gv.is_empty() {
        return None;
    }
    let clean_loaders: Vec<&str> = allowed_loaders
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    let mut best: Option<((u8, usize), &ProjectVersion)> = None;

    for ver in versions {
        let gv_ok = ver
            .game_versions
            .iter()
            .any(|gv| gv.trim().eq_ignore_ascii_case(clean_gv));
        if !gv_ok {
            continue;
        }
        let Some(loader_rank) = best_loader_rank(ver, &clean_loaders) else {
            continue;
        };
        if select_primary_version_file(ver).is_none() {
            continue;
        }

        let channel = ver.release_channel.trim();
        let channel_tier = if channel.eq_ignore_ascii_case("release") {
            0u8
        } else if channel.eq_ignore_ascii_case("beta") {
            1u8
        } else {
            2u8
        };
        let score = (channel_tier, loader_rank);
        if score == (0, 0) {
            return Some(ver.clone());
        }
        if best
            .as_ref()
            .is_none_or(|(best_score, _)| score < *best_score)
        {
            best = Some((score, ver));
        }
    }

    best.map(|(_, ver)| ver.clone())
}

pub(crate) async fn resolve_optimization_suggestions_with_base(
    downloader: &DownloadManager,
    base_url: &str,
    target: OptimizationTarget,
    game_version: &str,
) -> ThkResult<Vec<ResolvedOptimizationSuggestion>> {
    let clean_gv = game_version.trim();
    if clean_gv.is_empty() || !target_supports_optimizations(target) {
        return Ok(Vec::new());
    }

    let catalog = catalog_for_optimization_target(target);
    if catalog.is_empty() {
        return Ok(Vec::new());
    }

    let project_ids: Vec<&str> = catalog.iter().map(|entry| entry.project_id).collect();
    let batch_fut = get_modrinth_projects_batch_with_base(downloader, base_url, &project_ids);
    let versions_fut = join_all(catalog.iter().map(|entry| async move {
        let loaders = loaders_for_optimization_entry(target, entry.content_kind);
        get_modrinth_versions_for_loaders_with_base(
            downloader,
            base_url,
            entry.project_id,
            Some(clean_gv),
            &loaders,
        )
        .await
    }));

    let (batch_res, version_results) = futures_util::future::join(batch_fut, versions_fut).await;
    if version_results.iter().all(Result::is_err) {
        if let Some(first_err) = version_results.into_iter().find_map(Result::err) {
            return Err(first_err);
        }
        return Ok(Vec::new());
    }

    let projects_map = match batch_res {
        Ok(map) => map,
        Err(err) => {
            tracing::warn!(
                error = %err,
                target = ?target,
                game_version = clean_gv,
                "Modrinth batch project metadata lookup failed; proceeding with resolved versions"
            );
            HashMap::new()
        }
    };
    let mut out = Vec::with_capacity(catalog.len());

    for (entry, ver_res) in catalog.into_iter().zip(version_results) {
        let project_summary = projects_map.get(entry.project_id).cloned();
        let (compatible_version, error) = match ver_res {
            Ok(versions) => {
                let allowed = loaders_for_optimization_entry(target, entry.content_kind);
                (
                    select_best_compatible_version(&versions, clean_gv, &allowed),
                    None,
                )
            }
            Err(err) => (None, Some(err.to_string())),
        };

        out.push(ResolvedOptimizationSuggestion {
            project_id: entry.project_id.to_string(),
            slug: entry.slug.to_string(),
            title: entry.title.to_string(),
            content_kind: entry.content_kind,
            summary_vi: entry.summary_vi.to_string(),
            summary_en: entry.summary_en.to_string(),
            recommended_by_default: entry.recommended_by_default,
            project_summary,
            compatible_version,
            error,
        });
    }

    Ok(out)
}

pub async fn resolve_optimization_suggestions(
    downloader: &DownloadManager,
    target: OptimizationTarget,
    game_version: &str,
) -> ThkResult<Vec<ResolvedOptimizationSuggestion>> {
    resolve_optimization_suggestions_with_base(downloader, MODRINTH_API_BASE, target, game_version)
        .await
}

struct BatchInstallContext<'a> {
    downloader: &'a DownloadManager,
    db: &'a Database,
    base_url: &'a str,
    target_profile_id: &'a str,
    root_dir: &'a Path,
    target: OptimizationTarget,
    game_version: &'a str,
}

struct PreparedKindBatch {
    kind: ContentKind,
    dest_subdir: PathBuf,
    tasks: Vec<DownloadTask>,
    pending_records: Vec<(PathBuf, InstalledContentRecord)>,
}

fn check_cancelled(cancel: Option<&CancellationToken>) -> ThkResult<()> {
    if cancel.is_some_and(CancellationToken::is_cancelled) {
        return Err(ThkError::Cancelled);
    }
    Ok(())
}

async fn resolve_cached_project_summary(
    downloader: &DownloadManager,
    base_url: &str,
    project_id: &str,
    summary_cache: &mut HashMap<String, ProjectSummary>,
) -> ThkResult<ProjectSummary> {
    if let Some(cached) = summary_cache.get(project_id) {
        return Ok(cached.clone());
    }
    let detail = get_modrinth_project_with_base(downloader, base_url, project_id).await?;
    summary_cache.insert(project_id.to_string(), detail.summary.clone());
    Ok(detail.summary)
}

fn pinned_dependency_compatible(
    ver: &ProjectVersion,
    clean_gv: &str,
    allowed_loaders: &[&str],
) -> bool {
    let gv_ok = clean_gv.is_empty()
        || ver.game_versions.is_empty()
        || ver
            .game_versions
            .iter()
            .any(|gv| gv.trim().eq_ignore_ascii_case(clean_gv));
    let ldr_ok = ver.loaders.is_empty() || version_matches_loaders(ver, allowed_loaders);
    gv_ok && ldr_ok && select_primary_version_file(ver).is_some()
}

struct IncompatibleEdge {
    source_project_id: String,
    target_project_id: String,
    target_version_id: Option<String>,
}

fn installed_record_is_active_on_disk(dest_subdir: &Path, rec: &InstalledContentRecord) -> bool {
    rec.enabled
        && SafePath::resolve_child(dest_subdir, &rec.file_name).is_ok_and(|path| path.is_file())
}

fn collect_incompatible_edges(
    source_project_id: &str,
    ver: &ProjectVersion,
    out: &mut Vec<IncompatibleEdge>,
) {
    for dep in &ver.dependencies {
        if dep.relation == DependencyRelation::Incompatible && dep.project_id != source_project_id {
            out.push(IncompatibleEdge {
                source_project_id: source_project_id.to_string(),
                target_project_id: dep.project_id.clone(),
                target_version_id: dep.version_id.clone(),
            });
        }
    }
}

fn validate_incompatibilities(
    incompatibilities: &[IncompatibleEdge],
    visited: &HashMap<String, String>,
    installed_by_project: &HashMap<String, InstalledContentRecord>,
    dest_subdir: &Path,
) -> ThkResult<()> {
    for edge in incompatibilities {
        if let Some(conflict_vid) = visited.get(&edge.target_project_id) {
            if edge
                .target_version_id
                .as_ref()
                .is_none_or(|req_vid| req_vid == conflict_vid)
            {
                return Err(ThkError::Provider(format!(
                    "Incompatible optimization packages: {} conflicts with {}",
                    edge.source_project_id, edge.target_project_id
                )));
            }
        } else if let Some(installed) = installed_by_project.get(&edge.target_project_id) {
            if installed_record_is_active_on_disk(dest_subdir, installed)
                && edge
                    .target_version_id
                    .as_ref()
                    .is_none_or(|req_vid| req_vid == &installed.version_id)
            {
                return Err(ThkError::Provider(format!(
                    "Incompatible optimization packages: {} conflicts with installed {} ({})",
                    edge.source_project_id, edge.target_project_id, installed.title
                )));
            }
        }
    }
    Ok(())
}

async fn prepare_kind_batch(
    ctx: &BatchInstallContext<'_>,
    kind: ContentKind,
    items: &[&ResolvedOptimizationSuggestion],
    summary_cache: &mut HashMap<String, ProjectSummary>,
    cancel: Option<&CancellationToken>,
) -> ThkResult<Option<PreparedKindBatch>> {
    check_cancelled(cancel)?;
    let dest_subdir = SafePath::resolve_child(ctx.root_dir, kind.instance_subdir())?;
    let existing = ctx.db.list_installed_content(ctx.target_profile_id, kind)?;
    let mut installed_by_project: HashMap<String, InstalledContentRecord> = HashMap::new();
    for rec in existing {
        if rec.provider != ProviderSource::Modrinth || rec.project_id.is_empty() {
            continue;
        }
        let rec_active = installed_record_is_active_on_disk(&dest_subdir, &rec);
        match installed_by_project.get(&rec.project_id) {
            Some(prev) if installed_record_is_active_on_disk(&dest_subdir, prev) && !rec_active => {
            }
            _ => {
                installed_by_project.insert(rec.project_id.clone(), rec);
            }
        }
    }

    let mut visited: HashMap<String, String> = HashMap::new();
    let mut to_install: Vec<(ProjectSummary, ProjectVersion)> = Vec::new();
    let mut dep_queue: VecDeque<ProjectVersion> = VecDeque::new();
    let mut incompatibilities: Vec<IncompatibleEdge> = Vec::new();

    for item in items {
        check_cancelled(cancel)?;
        let Some(ref ver) = item.compatible_version else {
            continue;
        };
        validate_primary_version_file(ver, &item.title)?;

        if let Some(selected_vid) = visited.get(&item.project_id) {
            if selected_vid != &ver.id {
                return Err(ThkError::Provider(format!(
                    "Conflicting versions for project {}: {} and {}",
                    item.project_id, selected_vid, ver.id
                )));
            }
            continue;
        }

        collect_incompatible_edges(&item.project_id, ver, &mut incompatibilities);

        if installed_by_project
            .get(&item.project_id)
            .is_some_and(|rec| {
                rec.version_id == ver.id && installed_record_is_active_on_disk(&dest_subdir, rec)
            })
        {
            visited.insert(item.project_id.clone(), ver.id.clone());
            dep_queue.push_back(ver.clone());
            continue;
        }

        let summary = resolve_cached_project_summary(
            ctx.downloader,
            ctx.base_url,
            &item.project_id,
            summary_cache,
        )
        .await?;

        visited.insert(item.project_id.clone(), ver.id.clone());
        to_install.push((summary, ver.clone()));
        dep_queue.push_back(ver.clone());
    }

    let allowed_loaders = loaders_for_optimization_entry(ctx.target, kind);
    let clean_gv = ctx.game_version.trim();

    while let Some(ver) = dep_queue.pop_front() {
        check_cancelled(cancel)?;
        for dep in &ver.dependencies {
            if dep.relation != DependencyRelation::Required {
                continue;
            }
            check_cancelled(cancel)?;

            // QSL provides Fabric API compatibility on Quilt; installing both crashes at launch.
            let qsl_satisfies_fabric_api = ctx.target
                == OptimizationTarget::Client(LoaderType::Quilt)
                && dep.project_id == FABRIC_API.project_id
                && (visited.contains_key(QSL.project_id)
                    || installed_by_project
                        .get(QSL.project_id)
                        .is_some_and(|rec| installed_record_is_active_on_disk(&dest_subdir, rec)));
            if qsl_satisfies_fabric_api {
                continue;
            }

            if let Some(selected_vid) = visited.get(&dep.project_id) {
                if dep
                    .version_id
                    .as_ref()
                    .is_some_and(|required| required != selected_vid)
                {
                    return Err(ThkError::Provider(format!(
                        "Conflicting required version for project {}",
                        dep.project_id
                    )));
                }
                continue;
            }

            if let Some(installed) = installed_by_project.get(&dep.project_id) {
                let satisfied = installed_record_is_active_on_disk(&dest_subdir, installed)
                    && dep
                        .version_id
                        .as_ref()
                        .is_none_or(|required| installed.version_id == *required);
                if satisfied {
                    visited.insert(dep.project_id.clone(), installed.version_id.clone());
                    continue;
                }
            }

            let resolved_dep_ver = if let Some(ref vid) = dep.version_id {
                let dependency =
                    get_modrinth_version_by_id_with_base(ctx.downloader, ctx.base_url, vid).await?;
                if dependency.project_id != dep.project_id
                    || !pinned_dependency_compatible(&dependency, clean_gv, &allowed_loaders)
                {
                    return Err(ThkError::Provider(format!(
                        "Required dependency {} is incompatible with target {}",
                        dep.project_id, clean_gv
                    )));
                }
                dependency
            } else {
                let gv_opt = (!clean_gv.is_empty()).then_some(clean_gv);
                let dep_vers = get_modrinth_versions_for_loaders_with_base(
                    ctx.downloader,
                    ctx.base_url,
                    &dep.project_id,
                    gv_opt,
                    &allowed_loaders,
                )
                .await?;
                select_best_compatible_version(&dep_vers, clean_gv, &allowed_loaders).ok_or_else(
                    || {
                        ThkError::Provider(format!(
                            "No compatible version for required dependency {}",
                            dep.project_id
                        ))
                    },
                )?
            };

            collect_incompatible_edges(&dep.project_id, &resolved_dep_ver, &mut incompatibilities);

            let dep_proj = resolve_cached_project_summary(
                ctx.downloader,
                ctx.base_url,
                &dep.project_id,
                summary_cache,
            )
            .await?;

            visited.insert(dep.project_id.clone(), resolved_dep_ver.id.clone());
            to_install.push((dep_proj, resolved_dep_ver.clone()));
            dep_queue.push_back(resolved_dep_ver);
        }
    }

    validate_incompatibilities(
        &incompatibilities,
        &visited,
        &installed_by_project,
        &dest_subdir,
    )?;

    if to_install.is_empty() {
        return Ok(None);
    }

    let now = Utc::now().timestamp();
    let mut tasks = Vec::with_capacity(to_install.len());
    let mut pending_records = Vec::with_capacity(to_install.len());

    for (proj, ver) in to_install {
        let file = validate_primary_version_file(&ver, &proj.title)?;
        let clean_name = SafePath::validate_single_name(&file.filename)?;
        let dest_path = SafePath::resolve_child(&dest_subdir, clean_name)?;

        let mut task = DownloadTask::new(file.url.clone(), dest_path.clone());
        task.expected_sha1 = file.sha1.clone();
        task.expected_sha512 = file.sha512.clone();
        task.expected_size = (file.size > 0).then_some(file.size);
        task.label = proj.title.clone();
        tasks.push(task);

        pending_records.push((
            dest_path,
            InstalledContentRecord {
                profile_id: ctx.target_profile_id.to_string(),
                content_kind: kind,
                file_name: clean_name.to_string(),
                sha1: file.sha1.clone().unwrap_or_default(),
                provider: ProviderSource::Modrinth,
                project_id: proj.project_id,
                version_id: ver.id,
                title: proj.title,
                version_number: ver.version_number,
                icon_url: proj.icon_url,
                enabled: true,
                installed_at: now,
            },
        ));
    }

    Ok(Some(PreparedKindBatch {
        kind,
        dest_subdir,
        tasks,
        pending_records,
    }))
}

async fn install_selected_optimizations_with_base<F>(
    ctx: BatchInstallContext<'_>,
    selected: &[ResolvedOptimizationSuggestion],
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<Vec<PathBuf>>
where
    F: Fn(DownloadProgress) + Clone + Send + Sync + 'static,
{
    check_cancelled(cancel.as_ref())?;

    let mut summary_cache = HashMap::new();
    let mut mod_items = Vec::new();
    let mut plugin_items = Vec::new();

    for item in selected {
        if let Some(ref summary) = item.project_summary {
            summary_cache.insert(item.project_id.clone(), summary.clone());
        }
        if item.compatible_version.is_none() {
            continue;
        }
        match item.content_kind {
            ContentKind::Mod => mod_items.push(item),
            ContentKind::Plugin => plugin_items.push(item),
            _ => {}
        }
    }

    let mut prepared = Vec::new();
    for (kind, items) in [
        (ContentKind::Mod, mod_items),
        (ContentKind::Plugin, plugin_items),
    ] {
        if items.is_empty() {
            continue;
        }
        if let Some(batch) =
            prepare_kind_batch(&ctx, kind, &items, &mut summary_cache, cancel.as_ref()).await?
        {
            prepared.push(batch);
        }
    }

    let mut committed = Vec::new();
    for batch in prepared {
        check_cancelled(cancel.as_ref())?;
        tokio::fs::create_dir_all(&batch.dest_subdir).await?;
        let batch_title = match batch.kind {
            ContentKind::Plugin => "Installing optimization plugins",
            _ => "Installing optimization mods",
        };
        let installed = crate::install::install_content_batch(
            ctx.downloader,
            ctx.db,
            &batch.dest_subdir,
            batch_title,
            batch.tasks,
            batch.pending_records,
            cancel.clone(),
            on_progress.clone(),
        )
        .await?;
        committed.extend(installed);
    }

    Ok(committed)
}

#[allow(clippy::too_many_arguments)]
pub async fn install_selected_optimizations<F>(
    downloader: &DownloadManager,
    db: &Database,
    target_profile_id: &str,
    root_dir: &Path,
    target: OptimizationTarget,
    game_version: &str,
    selected: &[ResolvedOptimizationSuggestion],
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<Vec<PathBuf>>
where
    F: Fn(DownloadProgress) + Clone + Send + Sync + 'static,
{
    let ctx = BatchInstallContext {
        downloader,
        db,
        base_url: MODRINTH_API_BASE,
        target_profile_id,
        root_dir,
        target,
        game_version,
    };
    install_selected_optimizations_with_base(ctx, selected, cancel, on_progress).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn sample_version(
        id: &str,
        release_channel: &str,
        game_versions: &[&str],
        loaders: &[&str],
        filename: &str,
        url: &str,
    ) -> ProjectVersion {
        ProjectVersion {
            id: id.into(),
            project_id: "proj".into(),
            provider: ProviderSource::Modrinth,
            name: id.into(),
            version_number: id.into(),
            release_channel: release_channel.into(),
            published_at: None,
            game_versions: game_versions.iter().map(|s| (*s).to_string()).collect(),
            loaders: loaders.iter().map(|s| (*s).to_string()).collect(),
            files: vec![VersionFile {
                url: url.into(),
                filename: filename.into(),
                size: 0,
                sha1: None,
                sha512: None,
                primary: true,
            }],
            dependencies: Vec::new(),
            server_pack_file_id: None,
        }
    }

    #[test]
    fn catalogs_are_unique_non_empty_and_safe_per_target() {
        assert!(!target_supports_optimizations(OptimizationTarget::Client(
            LoaderType::Vanilla
        )));
        assert!(
            catalog_for_optimization_target(OptimizationTarget::Client(LoaderType::Vanilla))
                .is_empty()
        );
        assert!(!target_supports_optimizations(OptimizationTarget::Server {
            engine: ServerEngine::Vanilla,
            mod_loader: None,
        }));
        assert!(catalog_for_optimization_target(OptimizationTarget::Server {
            engine: ServerEngine::Vanilla,
            mod_loader: None,
        })
        .is_empty());

        let client_targets = [
            OptimizationTarget::Client(LoaderType::Fabric),
            OptimizationTarget::Client(LoaderType::Quilt),
            OptimizationTarget::Client(LoaderType::NeoForge),
            OptimizationTarget::Client(LoaderType::Forge),
        ];

        let server_targets = [
            OptimizationTarget::Server {
                engine: ServerEngine::Paper,
                mod_loader: None,
            },
            OptimizationTarget::Server {
                engine: ServerEngine::Purpur,
                mod_loader: None,
            },
            OptimizationTarget::Server {
                engine: ServerEngine::Folia,
                mod_loader: None,
            },
            OptimizationTarget::Server {
                engine: ServerEngine::Sponge,
                mod_loader: None,
            },
            OptimizationTarget::Server {
                engine: ServerEngine::Fabric,
                mod_loader: None,
            },
            OptimizationTarget::Server {
                engine: ServerEngine::NeoForge,
                mod_loader: None,
            },
            OptimizationTarget::Server {
                engine: ServerEngine::Forge,
                mod_loader: None,
            },
            OptimizationTarget::Server {
                engine: ServerEngine::Mohist,
                mod_loader: None,
            },
            OptimizationTarget::Server {
                engine: ServerEngine::Arclight,
                mod_loader: None,
            },
            OptimizationTarget::Server {
                engine: ServerEngine::Arclight,
                mod_loader: Some(LoaderType::Fabric),
            },
            OptimizationTarget::Server {
                engine: ServerEngine::Arclight,
                mod_loader: Some(LoaderType::NeoForge),
            },
            OptimizationTarget::Server {
                engine: ServerEngine::Arclight,
                mod_loader: Some(LoaderType::Forge),
            },
        ];

        let client_only_ids: HashSet<&str> = [
            "AANobbMI", "sk9rgfiA", "NNAgCjsB", "5ZwdcRci", "OVuFYfre", "LQ3K71Q1", "PtjYWJkn",
            "Bh37bMuy", "51shyZVL", "g96Z4WVZ", "ZP7xHXtw", "x1hIzbuY", "9mtu0sUO",
        ]
        .into_iter()
        .collect();

        for target in client_targets.iter().chain(server_targets.iter()).copied() {
            assert!(target_supports_optimizations(target));
            let entries = catalog_for_optimization_target(target);
            assert!(!entries.is_empty(), "empty catalog for {target:?}");

            let mut seen = HashSet::new();
            let mut seen_titles = HashSet::new();
            let mut seen_slugs = HashSet::new();
            for entry in &entries {
                assert!(
                    seen.insert(entry.project_id),
                    "duplicate project_id {} in {target:?}",
                    entry.project_id
                );
                assert!(
                    seen_titles.insert(entry.title.to_ascii_lowercase()),
                    "duplicate title {} in {target:?}",
                    entry.title
                );
                assert!(
                    seen_slugs.insert(entry.slug),
                    "duplicate slug {} in {target:?}",
                    entry.slug
                );
                assert!(!entry.summary_vi.trim().is_empty());
                assert!(!entry.summary_en.trim().is_empty());
            }

            let has_radium_canary_conflict =
                seen.contains(RADIUM.project_id) && seen.contains("qa2H4BS9");
            assert!(
                !has_radium_canary_conflict,
                "incompatible Radium+Canary in {target:?}"
            );
            let has_sodium_embeddium_conflict =
                seen.contains(SODIUM.project_id) && seen.contains(EMBEDDIUM.project_id);
            assert!(
                !has_sodium_embeddium_conflict,
                "incompatible Sodium+Embeddium in {target:?}"
            );
            assert!(
                !(seen.contains(CLUMPS_MOD.project_id) && seen.contains(CLUMPS_PLUGIN.project_id)),
                "duplicate Clumps mod+plugin in {target:?}"
            );
            assert!(
                !(seen.contains(QSL.project_id) && seen.contains(FABRIC_API.project_id)),
                "incompatible QSL+FabricAPI in {target:?}"
            );
            assert!(
                !(seen.contains(C2ME_FABRIC.project_id) && seen.contains(C2ME_NEOFORGE.project_id)),
                "duplicate C2ME Fabric+NeoForge in {target:?}"
            );
            assert!(
                !(seen.contains(STARLIGHT_FORGE.project_id)
                    && seen.contains(SCALABLE_LUX.project_id)),
                "incompatible Starlight+ScalableLux in {target:?}"
            );
        }

        for target in server_targets {
            let entries = catalog_for_optimization_target(target);
            for entry in entries {
                assert!(
                    !client_only_ids.contains(entry.project_id),
                    "client-only mod {} leaked into server target {target:?}",
                    entry.project_id
                );
            }
        }
    }

    #[test]
    fn loaders_for_optimization_entry_covers_all_targets() {
        assert!(loaders_for_optimization_entry(
            OptimizationTarget::Client(LoaderType::Vanilla),
            ContentKind::Mod
        )
        .is_empty());
        assert_eq!(
            loaders_for_optimization_entry(
                OptimizationTarget::Client(LoaderType::Fabric),
                ContentKind::Mod
            ),
            vec!["fabric"]
        );
        assert_eq!(
            loaders_for_optimization_entry(
                OptimizationTarget::Client(LoaderType::Quilt),
                ContentKind::Mod
            ),
            vec!["quilt", "fabric"]
        );
        assert_eq!(
            loaders_for_optimization_entry(
                OptimizationTarget::Client(LoaderType::Forge),
                ContentKind::Mod
            ),
            vec!["forge"]
        );
        assert_eq!(
            loaders_for_optimization_entry(
                OptimizationTarget::Client(LoaderType::NeoForge),
                ContentKind::Mod
            ),
            vec!["neoforge"]
        );
        assert_eq!(
            loaders_for_optimization_entry(
                OptimizationTarget::Server {
                    engine: ServerEngine::Purpur,
                    mod_loader: None
                },
                ContentKind::Plugin
            ),
            vec!["purpur", "paper", "spigot", "bukkit"]
        );
        assert_eq!(
            loaders_for_optimization_entry(
                OptimizationTarget::Server {
                    engine: ServerEngine::Paper,
                    mod_loader: None
                },
                ContentKind::Plugin
            ),
            vec!["paper", "spigot", "bukkit"]
        );
        assert_eq!(
            loaders_for_optimization_entry(
                OptimizationTarget::Server {
                    engine: ServerEngine::Folia,
                    mod_loader: None
                },
                ContentKind::Plugin
            ),
            vec!["folia"]
        );
        assert_eq!(
            loaders_for_optimization_entry(
                OptimizationTarget::Server {
                    engine: ServerEngine::Sponge,
                    mod_loader: None
                },
                ContentKind::Plugin
            ),
            vec!["sponge"]
        );
        assert_eq!(
            loaders_for_optimization_entry(
                OptimizationTarget::Server {
                    engine: ServerEngine::Mohist,
                    mod_loader: None
                },
                ContentKind::Plugin
            ),
            vec!["paper", "spigot", "bukkit"]
        );
        assert_eq!(
            loaders_for_optimization_entry(
                OptimizationTarget::Server {
                    engine: ServerEngine::Mohist,
                    mod_loader: None
                },
                ContentKind::Mod
            ),
            vec!["forge"]
        );
        assert_eq!(
            loaders_for_optimization_entry(
                OptimizationTarget::Server {
                    engine: ServerEngine::Arclight,
                    mod_loader: Some(LoaderType::Fabric)
                },
                ContentKind::Plugin
            ),
            vec!["paper", "spigot", "bukkit"]
        );
        assert_eq!(
            loaders_for_optimization_entry(
                OptimizationTarget::Server {
                    engine: ServerEngine::Arclight,
                    mod_loader: Some(LoaderType::Fabric)
                },
                ContentKind::Mod
            ),
            vec!["fabric"]
        );
        assert_eq!(
            loaders_for_optimization_entry(
                OptimizationTarget::Server {
                    engine: ServerEngine::Arclight,
                    mod_loader: Some(LoaderType::Forge)
                },
                ContentKind::Mod
            ),
            vec!["forge"]
        );
        assert_eq!(
            loaders_for_optimization_entry(
                OptimizationTarget::Server {
                    engine: ServerEngine::Arclight,
                    mod_loader: None
                },
                ContentKind::Mod
            ),
            vec!["neoforge"]
        );
        assert_eq!(
            loaders_for_optimization_entry(
                OptimizationTarget::Server {
                    engine: ServerEngine::Arclight,
                    mod_loader: Some(LoaderType::Vanilla)
                },
                ContentKind::Mod
            ),
            vec!["neoforge"]
        );
    }

    #[test]
    fn select_best_compatible_version_enforces_exact_game_version_loader_safepath_and_priority() {
        let bad_traversal = sample_version(
            "bad-path",
            "release",
            &["1.21.4"],
            &["fabric"],
            "../evil.jar",
            "https://example.invalid/evil.jar",
        );
        let empty_url = sample_version(
            "empty-url",
            "release",
            &["1.21.4"],
            &["fabric"],
            "ok.jar",
            "   ",
        );
        let wrong_gv = sample_version(
            "wrong-gv",
            "release",
            &["1.21.1"],
            &["fabric"],
            "old.jar",
            "https://example.invalid/old.jar",
        );
        let wrong_loader = sample_version(
            "wrong-ldr",
            "release",
            &["1.21.4"],
            &["forge"],
            "forge.jar",
            "https://example.invalid/forge.jar",
        );
        let alpha = sample_version(
            "alpha-1",
            "alpha",
            &["1.21.4"],
            &["Fabric"],
            "alpha.jar",
            "https://example.invalid/alpha.jar",
        );
        let beta = sample_version(
            "beta-1",
            "beta",
            &["1.21.4"],
            &["fabric"],
            "beta.jar",
            "https://example.invalid/beta.jar",
        );
        let release = sample_version(
            "rel-1",
            "release",
            &["1.21.4"],
            &["fabric"],
            "release.jar",
            "https://example.invalid/release.jar",
        );

        let all = vec![
            bad_traversal.clone(),
            empty_url.clone(),
            wrong_gv.clone(),
            wrong_loader.clone(),
            alpha.clone(),
            beta.clone(),
            release.clone(),
        ];
        assert_eq!(
            select_best_compatible_version(&all, " 1.21.4 ", &["fabric"])
                .map(|v| v.id)
                .as_deref(),
            Some("rel-1")
        );

        let no_release = vec![alpha.clone(), beta.clone()];
        assert_eq!(
            select_best_compatible_version(&no_release, "1.21.4", &["fabric"])
                .map(|v| v.id)
                .as_deref(),
            Some("beta-1")
        );

        let only_alpha = vec![alpha];
        assert_eq!(
            select_best_compatible_version(&only_alpha, "1.21.4", &["fabric"])
                .map(|v| v.id)
                .as_deref(),
            Some("alpha-1")
        );

        assert!(select_best_compatible_version(&all, "", &["fabric"]).is_none());
        assert!(
            select_best_compatible_version(&[bad_traversal, empty_url], "1.21.4", &["fabric"])
                .is_none()
        );
        assert!(select_best_compatible_version(&[wrong_gv], "1.21.4", &["fabric"]).is_none());
        assert!(select_best_compatible_version(&[wrong_loader], "1.21.4", &["fabric"]).is_none());
        assert!(
            select_best_compatible_version(&[release], "1.21.4", &[])
                .map(|v| v.id)
                .as_deref()
                == Some("rel-1")
        );
    }

    async fn spawn_test_server(
        build_routes: impl FnOnce(&str) -> HashMap<String, (u16, String, Vec<u8>)>,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let shared = Arc::new(build_routes(&base_url));
        let handle = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let map = Arc::clone(&shared);
                tokio::spawn(async move {
                    let mut req_buf = [0u8; 4096];
                    let n = stream.read(&mut req_buf).await.unwrap_or(0);
                    let req_line = String::from_utf8_lossy(&req_buf[..n]);
                    let raw_target = req_line
                        .lines()
                        .next()
                        .and_then(|l| l.split_whitespace().nth(1))
                        .unwrap_or("/");
                    let path_only = raw_target.split('?').next().unwrap_or(raw_target);
                    if let Some((status, ctype, body)) =
                        map.get(raw_target).or_else(|| map.get(path_only))
                    {
                        let status_text = if *status == 200 { "OK" } else { "Error" };
                        let header = format!(
                            "HTTP/1.1 {status} {status_text}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(header.as_bytes()).await;
                        let _ = stream.write_all(body).await;
                    } else {
                        let _ = stream
                            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                            .await;
                    }
                });
            }
        });
        (base_url, handle)
    }

    #[tokio::test]
    async fn resolve_optimization_suggestions_handles_batch_partial_errors_and_all_failed() {
        let mut routes: HashMap<String, (u16, String, Vec<u8>)> = HashMap::new();
        let projects_json = serde_json::json!([
            {
                "id": "fALzjamp",
                "slug": "chunky",
                "title": "Chunky",
                "description": "Pre-generates chunks",
                "downloads": 1000,
                "project_type": "plugin"
            },
            {
                "id": "ksm6XRZ9",
                "slug": "slumber",
                "title": "Slumber",
                "description": "Pauses idle chunks",
                "downloads": 500,
                "project_type": "plugin"
            }
        ])
        .to_string();
        routes.insert(
            "/projects".into(),
            (200, "application/json".into(), projects_json.into_bytes()),
        );
        let chunky_vers = serde_json::json!([
            {
                "id": "v-chunky",
                "project_id": "fALzjamp",
                "name": "1.4.0",
                "version_number": "1.4.0",
                "version_type": "release",
                "game_versions": ["1.21.4"],
                "loaders": ["sponge"],
                "files": [{
                    "url": "https://example.invalid/chunky.jar",
                    "filename": "chunky-1.4.0.jar",
                    "size": 10,
                    "primary": true
                }],
                "dependencies": []
            }
        ])
        .to_string();
        routes.insert(
            "/project/fALzjamp/version".into(),
            (200, "application/json".into(), chunky_vers.into_bytes()),
        );
        routes.insert(
            "/project/cUhi3iB2/version".into(),
            (200, "application/json".into(), b"[]".to_vec()),
        );
        routes.insert(
            "/project/ksm6XRZ9/version".into(),
            (200, "application/json".into(), b"[]".to_vec()),
        );
        routes.insert(
            "/project/tCUVz9xy/version".into(),
            (404, "application/json".into(), b"{}".to_vec()),
        );
        routes.insert(
            "/project/QItC3hpi/version".into(),
            (200, "application/json".into(), b"[]".to_vec()),
        );

        let (base_url, handle) = spawn_test_server(|_| routes).await;
        let dl = DownloadManager::new().unwrap();
        let sponge_target = OptimizationTarget::Server {
            engine: ServerEngine::Sponge,
            mod_loader: None,
        };

        assert!(resolve_optimization_suggestions_with_base(
            &dl,
            &base_url,
            OptimizationTarget::Client(LoaderType::Vanilla),
            "1.21.4",
        )
        .await
        .unwrap()
        .is_empty());

        assert!(
            resolve_optimization_suggestions_with_base(&dl, &base_url, sponge_target, "  ")
                .await
                .unwrap()
                .is_empty()
        );

        let resolved =
            resolve_optimization_suggestions_with_base(&dl, &base_url, sponge_target, "1.21.4")
                .await
                .unwrap();
        assert_eq!(resolved.len(), 5);
        assert_eq!(resolved[0].project_id, "fALzjamp");
        assert!(resolved[0].project_summary.is_some());
        assert_eq!(
            resolved[0]
                .compatible_version
                .as_ref()
                .map(|v| v.id.as_str()),
            Some("v-chunky")
        );
        assert!(resolved[0].error.is_none());

        assert_eq!(resolved[1].project_id, "cUhi3iB2");
        assert!(resolved[1].compatible_version.is_none());
        assert!(resolved[1].error.is_none());

        assert_eq!(resolved[2].project_id, "ksm6XRZ9");
        assert!(resolved[2].compatible_version.is_none());
        assert!(resolved[2].error.is_none());

        assert_eq!(resolved[3].project_id, "tCUVz9xy");
        assert!(resolved[3].compatible_version.is_none());
        assert!(resolved[3].error.is_some());

        assert_eq!(resolved[4].project_id, "QItC3hpi");
        assert!(resolved[4].compatible_version.is_none());
        assert!(resolved[4].error.is_none());

        handle.abort();

        let err = resolve_optimization_suggestions_with_base(
            &dl,
            "http://127.0.0.1:1",
            sponge_target,
            "1.21.4",
        )
        .await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn install_selected_optimizations_groups_by_kind_dedups_resolves_deps_and_skips_installed(
    ) {
        let tmp = std::env::temp_dir().join(format!("thk-opt-inst-{}", uuid::Uuid::new_v4()));
        let srv_dir = tmp.join("server");
        std::fs::create_dir_all(&srv_dir).unwrap();
        let db = Database::open_in_memory().unwrap();
        let profile_id = "server:s1";

        let (base_url, server_task) = spawn_test_server(|base_url| {
            let mut routes: HashMap<String, (u16, String, Vec<u8>)> = HashMap::new();
            routes.insert(
                "/project/AANobbMI".into(),
                (
                    200,
                    "application/json".into(),
                    serde_json::json!({
                        "id": "AANobbMI",
                        "slug": "sodium",
                        "title": "Sodium",
                        "description": "Renderer",
                        "downloads": 10,
                        "project_type": "mod"
                    })
                    .to_string()
                    .into_bytes(),
                ),
            );
            routes.insert(
                "/project/P7dR8mSH".into(),
                (
                    200,
                    "application/json".into(),
                    serde_json::json!({
                        "id": "P7dR8mSH",
                        "slug": "fabric-api",
                        "title": "Fabric API",
                        "description": "Core",
                        "downloads": 10,
                        "project_type": "mod"
                    })
                    .to_string()
                    .into_bytes(),
                ),
            );
            routes.insert(
                "/project/uXXizFIs".into(),
                (
                    200,
                    "application/json".into(),
                    serde_json::json!({
                        "id": "uXXizFIs",
                        "slug": "ferrite-core",
                        "title": "FerriteCore",
                        "description": "RAM",
                        "downloads": 10,
                        "project_type": "mod"
                    })
                    .to_string()
                    .into_bytes(),
                ),
            );
            routes.insert(
                "/project/P7dR8mSH/version".into(),
                (
                    200,
                    "application/json".into(),
                    serde_json::json!([{
                        "id": "v-fapi",
                        "project_id": "P7dR8mSH",
                        "name": "0.100.0",
                        "version_number": "0.100.0",
                        "version_type": "release",
                        "game_versions": ["1.21.4"],
                        "loaders": ["fabric"],
                        "files": [{
                            "url": format!("{base_url}/files/fabric-api.jar"),
                            "filename": "fabric-api-0.100.0.jar",
                            "size": 4,
                            "primary": true
                        }],
                        "dependencies": []
                    }])
                    .to_string()
                    .into_bytes(),
                ),
            );
            routes.insert(
                "/files/fabric-api.jar".into(),
                (200, "application/java-archive".into(), b"fapi".to_vec()),
            );
            routes.insert(
                "/files/ferrite.jar".into(),
                (200, "application/java-archive".into(), b"ferr".to_vec()),
            );
            routes.insert(
                "/files/chunky.jar".into(),
                (200, "application/java-archive".into(), b"chnk".to_vec()),
            );
            routes
        })
        .await;

        // Pre-seed already-installed plugin ("vE2FN5qn") with file on disk so it is skipped without hitting HTTP.
        std::fs::create_dir_all(srv_dir.join("plugins")).unwrap();
        std::fs::write(
            srv_dir.join("plugins").join("lmd-existing.jar"),
            b"existing-lmd",
        )
        .unwrap();
        db.upsert_installed_content(&InstalledContentRecord {
            profile_id: profile_id.into(),
            content_kind: ContentKind::Plugin,
            file_name: "lmd-existing.jar".into(),
            sha1: String::new(),
            provider: ProviderSource::Modrinth,
            project_id: "vE2FN5qn".into(),
            version_id: "v-lmd".into(),
            title: "Let Me Despawn".into(),
            version_number: "1.0".into(),
            icon_url: None,
            enabled: true,
            installed_at: 0,
        })
        .unwrap();

        let ferrite_ver = ProjectVersion {
            id: "v-ferrite".into(),
            project_id: "uXXizFIs".into(),
            provider: ProviderSource::Modrinth,
            name: "7.0.0".into(),
            version_number: "7.0.0".into(),
            release_channel: "release".into(),
            published_at: None,
            game_versions: vec!["1.21.4".into()],
            loaders: vec!["fabric".into()],
            files: vec![VersionFile {
                url: format!("{base_url}/files/ferrite.jar"),
                filename: "ferritecore-7.0.0.jar".into(),
                size: 4,
                sha1: None,
                sha512: None,
                primary: true,
            }],
            dependencies: vec![crate::provider::DependencySpec {
                project_id: "P7dR8mSH".into(),
                version_id: None,
                relation: DependencyRelation::Required,
            }],
            server_pack_file_id: None,
        };

        let chunky_ver = sample_version(
            "v-chunky",
            "release",
            &["1.21.4"],
            &["paper"],
            "chunky-1.4.0.jar",
            &format!("{base_url}/files/chunky.jar"),
        );

        let lmd_ver = sample_version(
            "v-lmd",
            "release",
            &["1.21.4"],
            &["paper"],
            "lmd-1.0.jar",
            "http://127.0.0.1:1/should-not-download",
        );

        let selected = vec![
            ResolvedOptimizationSuggestion {
                project_id: "uXXizFIs".into(),
                slug: "ferrite-core".into(),
                title: "FerriteCore".into(),
                content_kind: ContentKind::Mod,
                summary_vi: String::new(),
                summary_en: String::new(),
                recommended_by_default: true,
                project_summary: None,
                compatible_version: Some(ferrite_ver.clone()),
                error: None,
            },
            // Duplicate selection should deduplicate cleanly
            ResolvedOptimizationSuggestion {
                project_id: "uXXizFIs".into(),
                slug: "ferrite-core".into(),
                title: "FerriteCore".into(),
                content_kind: ContentKind::Mod,
                summary_vi: String::new(),
                summary_en: String::new(),
                recommended_by_default: true,
                project_summary: None,
                compatible_version: Some(ferrite_ver),
                error: None,
            },
            ResolvedOptimizationSuggestion {
                project_id: "fALzjamp".into(),
                slug: "chunky".into(),
                title: "Chunky".into(),
                content_kind: ContentKind::Plugin,
                summary_vi: String::new(),
                summary_en: String::new(),
                recommended_by_default: true,
                project_summary: Some(ProjectSummary {
                    provider: ProviderSource::Modrinth,
                    project_id: "fALzjamp".into(),
                    slug: "chunky".into(),
                    title: "Chunky".into(),
                    summary: String::new(),
                    author: String::new(),
                    downloads: 100,
                    icon_url: None,
                    categories: Vec::new(),
                    updated_at: None,
                    content_kind: ContentKind::Plugin,
                }),
                compatible_version: Some(chunky_ver),
                error: None,
            },
            ResolvedOptimizationSuggestion {
                project_id: "vE2FN5qn".into(),
                slug: "lmd".into(),
                title: "Let Me Despawn".into(),
                content_kind: ContentKind::Plugin,
                summary_vi: String::new(),
                summary_en: String::new(),
                recommended_by_default: true,
                project_summary: None,
                compatible_version: Some(lmd_ver),
                error: None,
            },
        ];

        let dl = DownloadManager::new().unwrap();
        let target = OptimizationTarget::Server {
            engine: ServerEngine::Arclight,
            mod_loader: Some(LoaderType::Fabric),
        };

        let cancelled = CancellationToken::new();
        cancelled.cancel();
        let cancel_res = install_selected_optimizations_with_base(
            BatchInstallContext {
                downloader: &dl,
                db: &db,
                base_url: &base_url,
                target_profile_id: profile_id,
                root_dir: &srv_dir,
                target,
                game_version: "1.21.4",
            },
            &selected,
            Some(cancelled),
            |_| {},
        )
        .await;
        assert!(matches!(cancel_res, Err(ThkError::Cancelled)));

        let committed = install_selected_optimizations_with_base(
            BatchInstallContext {
                downloader: &dl,
                db: &db,
                base_url: &base_url,
                target_profile_id: profile_id,
                root_dir: &srv_dir,
                target,
                game_version: "1.21.4",
            },
            &selected,
            None,
            |_| {},
        )
        .await
        .unwrap();

        assert_eq!(committed.len(), 3);
        assert_eq!(
            std::fs::read(srv_dir.join("mods").join("ferritecore-7.0.0.jar")).unwrap(),
            b"ferr"
        );
        assert_eq!(
            std::fs::read(srv_dir.join("mods").join("fabric-api-0.100.0.jar")).unwrap(),
            b"fapi"
        );
        assert_eq!(
            std::fs::read(srv_dir.join("plugins").join("chunky-1.4.0.jar")).unwrap(),
            b"chnk"
        );
        assert_eq!(
            db.list_installed_content(profile_id, ContentKind::Mod)
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            db.list_installed_content(profile_id, ContentKind::Plugin)
                .unwrap()
                .len(),
            2
        );

        server_task.abort();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn modrinth_batch_and_multi_loader_query_encoding_and_empty_inputs() {
        let dl = DownloadManager::new().unwrap();
        let empty = crate::modrinth::get_modrinth_projects_batch_with_base(
            &dl,
            "http://127.0.0.1:1",
            &["", "   "],
        )
        .await
        .unwrap();
        assert!(empty.is_empty());

        let expected_ids_q = urlencoding::encode(r#"["P7dR8mSH","AANobbMI"]"#).into_owned();
        let expected_gv_q = urlencoding::encode(r#"["1.21.4"]"#).into_owned();
        let expected_ldr_q = urlencoding::encode(r#"["paper","spigot"]"#).into_owned();
        let batch_route = format!("/projects?ids={expected_ids_q}");
        let vers_route = format!(
            "/project/fALzjamp/version?game_versions={expected_gv_q}&loaders={expected_ldr_q}"
        );

        let (base_url, handle) = spawn_test_server(move |_| {
            let mut routes: HashMap<String, (u16, String, Vec<u8>)> = HashMap::new();
            routes.insert(
                batch_route,
                (
                    200,
                    "application/json".into(),
                    serde_json::json!([
                        {
                            "id": "P7dR8mSH",
                            "slug": "fabric-api",
                            "title": "Fabric API",
                            "description": "Core API",
                            "downloads": 42,
                            "project_type": "mod"
                        },
                        {
                            "id": "AANobbMI",
                            "slug": "sodium",
                            "title": "Sodium",
                            "description": "Renderer",
                            "downloads": 99,
                            "project_type": "mod"
                        }
                    ])
                    .to_string()
                    .into_bytes(),
                ),
            );
            routes.insert(
                vers_route,
                (
                    200,
                    "application/json".into(),
                    serde_json::json!([{
                        "id": "v-paper-1",
                        "project_id": "fALzjamp",
                        "name": "1.4.2",
                        "version_number": "1.4.2",
                        "version_type": "release",
                        "game_versions": ["1.21.4"],
                        "loaders": ["paper", "spigot"],
                        "files": [{
                            "url": "https://example.invalid/chunky-1.4.2.jar",
                            "filename": "chunky-1.4.2.jar",
                            "size": 123,
                            "primary": true
                        }],
                        "dependencies": []
                    }])
                    .to_string()
                    .into_bytes(),
                ),
            );
            routes
        })
        .await;

        let map = crate::modrinth::get_modrinth_projects_batch_with_base(
            &dl,
            &base_url,
            &["P7dR8mSH", "  ", "AANobbMI"],
        )
        .await
        .unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(map["P7dR8mSH"].title, "Fabric API");
        assert_eq!(map["AANobbMI"].slug, "sodium");

        let versions = crate::modrinth::get_modrinth_versions_for_loaders_with_base(
            &dl,
            &base_url,
            "fALzjamp",
            Some(" 1.21.4 "),
            &["paper", "", "spigot"],
        )
        .await
        .unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].id, "v-paper-1");
        assert_eq!(versions[0].loaders, vec!["paper", "spigot"]);

        handle.abort();
    }

    #[tokio::test]
    async fn install_selected_optimizations_resolves_pinned_dependency_and_skips_when_matching() {
        let tmp = std::env::temp_dir().join(format!("thk-opt-pinned-{}", uuid::Uuid::new_v4()));
        let inst_dir = tmp.join("instance");
        std::fs::create_dir_all(&inst_dir).unwrap();
        let db = Database::open_in_memory().unwrap();
        let profile_id = "profile-fabric-1";

        db.upsert_installed_content(&InstalledContentRecord {
            profile_id: profile_id.into(),
            content_kind: ContentKind::Mod,
            file_name: "fabric-api-old.jar".into(),
            sha1: String::new(),
            provider: ProviderSource::Modrinth,
            project_id: "P7dR8mSH".into(),
            version_id: "v-fapi-old".into(),
            title: "Fabric API".into(),
            version_number: "0.90.0".into(),
            icon_url: None,
            enabled: true,
            installed_at: 0,
        })
        .unwrap();

        let (base_url, handle) = spawn_test_server(|base_url| {
            let mut routes: HashMap<String, (u16, String, Vec<u8>)> = HashMap::new();
            routes.insert(
                "/project/AANobbMI".into(),
                (
                    200,
                    "application/json".into(),
                    serde_json::json!({
                        "id": "AANobbMI",
                        "slug": "sodium",
                        "title": "Sodium",
                        "description": "Renderer",
                        "downloads": 10,
                        "project_type": "mod"
                    })
                    .to_string()
                    .into_bytes(),
                ),
            );
            routes.insert(
                "/project/P7dR8mSH".into(),
                (
                    200,
                    "application/json".into(),
                    serde_json::json!({
                        "id": "P7dR8mSH",
                        "slug": "fabric-api",
                        "title": "Fabric API",
                        "description": "Core",
                        "downloads": 10,
                        "project_type": "mod"
                    })
                    .to_string()
                    .into_bytes(),
                ),
            );
            routes.insert(
                "/version/v-fapi-pinned".into(),
                (
                    200,
                    "application/json".into(),
                    serde_json::json!({
                        "id": "v-fapi-pinned",
                        "project_id": "P7dR8mSH",
                        "name": "0.105.0",
                        "version_number": "0.105.0",
                        "version_type": "release",
                        "game_versions": ["1.21.4"],
                        "loaders": ["fabric"],
                        "files": [{
                            "url": format!("{base_url}/files/fapi-pinned.jar"),
                            "filename": "fabric-api-0.105.0.jar",
                            "size": 6,
                            "primary": true
                        }],
                        "dependencies": []
                    })
                    .to_string()
                    .into_bytes(),
                ),
            );
            routes.insert(
                "/files/fapi-pinned.jar".into(),
                (200, "application/java-archive".into(), b"pinned".to_vec()),
            );
            routes.insert(
                "/files/sodium.jar".into(),
                (200, "application/java-archive".into(), b"sodium".to_vec()),
            );
            routes
        })
        .await;

        let sodium_ver = ProjectVersion {
            id: "v-sodium".into(),
            project_id: "AANobbMI".into(),
            provider: ProviderSource::Modrinth,
            name: "0.6.0".into(),
            version_number: "0.6.0".into(),
            release_channel: "release".into(),
            published_at: None,
            game_versions: vec!["1.21.4".into()],
            loaders: vec!["fabric".into()],
            files: vec![VersionFile {
                url: format!("{base_url}/files/sodium.jar"),
                filename: "sodium-0.6.0.jar".into(),
                size: 6,
                sha1: None,
                sha512: None,
                primary: true,
            }],
            dependencies: vec![crate::provider::DependencySpec {
                project_id: "P7dR8mSH".into(),
                version_id: Some("v-fapi-pinned".into()),
                relation: DependencyRelation::Required,
            }],
            server_pack_file_id: None,
        };

        let selected = vec![ResolvedOptimizationSuggestion {
            project_id: "AANobbMI".into(),
            slug: "sodium".into(),
            title: "Sodium".into(),
            content_kind: ContentKind::Mod,
            summary_vi: String::new(),
            summary_en: String::new(),
            recommended_by_default: true,
            project_summary: None,
            compatible_version: Some(sodium_ver),
            error: None,
        }];

        let dl = DownloadManager::new().unwrap();
        let target = OptimizationTarget::Client(LoaderType::Fabric);
        let committed = install_selected_optimizations_with_base(
            BatchInstallContext {
                downloader: &dl,
                db: &db,
                base_url: &base_url,
                target_profile_id: profile_id,
                root_dir: &inst_dir,
                target,
                game_version: "1.21.4",
            },
            &selected,
            None,
            |_| {},
        )
        .await
        .unwrap();

        assert_eq!(committed.len(), 2);
        let installed_mods = db
            .list_installed_content(profile_id, ContentKind::Mod)
            .unwrap();
        let fapi = installed_mods
            .iter()
            .find(|r| r.project_id == "P7dR8mSH")
            .unwrap();
        assert_eq!(fapi.version_id, "v-fapi-pinned");

        handle.abort();

        // Re-running once both pinned dependency and selected mod match DB must skip without network.
        let skipped = install_selected_optimizations_with_base(
            BatchInstallContext {
                downloader: &dl,
                db: &db,
                base_url: "http://127.0.0.1:1",
                target_profile_id: profile_id,
                root_dir: &inst_dir,
                target,
                game_version: "1.21.4",
            },
            &selected,
            None,
            |_| {},
        )
        .await
        .unwrap();
        assert!(skipped.is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    fn make_suggestion(
        project_id: &str,
        slug: &str,
        title: &str,
        content_kind: ContentKind,
        version: ProjectVersion,
    ) -> ResolvedOptimizationSuggestion {
        ResolvedOptimizationSuggestion {
            project_id: project_id.into(),
            slug: slug.into(),
            title: title.into(),
            content_kind,
            summary_vi: String::new(),
            summary_en: String::new(),
            recommended_by_default: true,
            project_summary: Some(ProjectSummary {
                provider: ProviderSource::Modrinth,
                project_id: project_id.into(),
                slug: slug.into(),
                title: title.into(),
                summary: String::new(),
                author: String::new(),
                downloads: 0,
                icon_url: None,
                categories: Vec::new(),
                updated_at: None,
                content_kind,
            }),
            compatible_version: Some(version),
            error: None,
        }
    }

    fn project_meta_json(id: &str, slug: &str, title: &str, project_type: &str) -> Vec<u8> {
        serde_json::json!({
            "id": id,
            "slug": slug,
            "title": title,
            "description": title,
            "downloads": 100,
            "project_type": project_type
        })
        .to_string()
        .into_bytes()
    }

    fn modrinth_version_json(
        id: &str,
        project_id: &str,
        version_type: &str,
        loaders: &[&str],
        (filename, url, size): (&str, &str, u64),
        dependencies: Vec<serde_json::Value>,
    ) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "project_id": project_id,
            "name": id,
            "version_number": "1.0.0",
            "version_type": version_type,
            "game_versions": ["1.21.1", "1.21.4"],
            "loaders": loaders,
            "files": [{
                "url": url,
                "filename": filename,
                "size": size,
                "primary": true
            }],
            "dependencies": dependencies
        })
    }

    #[test]
    fn select_best_compatible_version_prioritizes_channel_then_loader_rank() {
        let fabric_release = sample_version(
            "rel-fabric",
            "release",
            &["1.21.4"],
            &["fabric"],
            "mod-fabric.jar",
            "https://example.invalid/mod-fabric.jar",
        );
        let quilt_release_first = sample_version(
            "rel-quilt-1",
            "release",
            &["1.21.4"],
            &["quilt"],
            "mod-quilt-1.jar",
            "https://example.invalid/mod-quilt-1.jar",
        );
        let quilt_release_second = sample_version(
            "rel-quilt-2",
            "release",
            &["1.21.4"],
            &["quilt"],
            "mod-quilt-2.jar",
            "https://example.invalid/mod-quilt-2.jar",
        );
        let quilt_beta = sample_version(
            "beta-quilt",
            "beta",
            &["1.21.4"],
            &["quilt"],
            "mod-quilt-beta.jar",
            "https://example.invalid/mod-quilt-beta.jar",
        );

        let quilt_loaders = ["quilt", "fabric"];
        let best_native = select_best_compatible_version(
            &[
                fabric_release.clone(),
                quilt_release_first.clone(),
                quilt_release_second,
            ],
            "1.21.4",
            &quilt_loaders,
        )
        .unwrap();
        assert_eq!(best_native.id, "rel-quilt-1");

        let best_channel_over_loader =
            select_best_compatible_version(&[quilt_beta, fabric_release], "1.21.4", &quilt_loaders)
                .unwrap();
        assert_eq!(best_channel_over_loader.id, "rel-fabric");

        let paper_release = sample_version(
            "rel-paper",
            "release",
            &["1.21.4"],
            &["paper"],
            "plug-paper.jar",
            "https://example.invalid/plug-paper.jar",
        );
        let purpur_beta = sample_version(
            "beta-purpur",
            "beta",
            &["1.21.4"],
            &["purpur"],
            "plug-purpur-beta.jar",
            "https://example.invalid/plug-purpur-beta.jar",
        );
        let purpur_release = sample_version(
            "rel-purpur",
            "release",
            &["1.21.4"],
            &["purpur"],
            "plug-purpur.jar",
            "https://example.invalid/plug-purpur.jar",
        );
        let purpur_loaders = ["purpur", "paper", "spigot"];
        assert_eq!(
            select_best_compatible_version(
                &[purpur_beta.clone(), paper_release.clone()],
                "1.21.4",
                &purpur_loaders,
            )
            .unwrap()
            .id,
            "rel-paper"
        );
        assert_eq!(
            select_best_compatible_version(
                &[paper_release, purpur_release],
                "1.21.4",
                &purpur_loaders,
            )
            .unwrap()
            .id,
            "rel-purpur"
        );
    }

    #[tokio::test]
    async fn quilt_qsl_satisfies_required_fabric_api_without_downloading_fabric_api() {
        let tmp = std::env::temp_dir().join(format!("thk-opt-qsl-{}", uuid::Uuid::new_v4()));
        let inst_dir = tmp.join("instance");
        std::fs::create_dir_all(&inst_dir).unwrap();
        let db = Database::open_in_memory().unwrap();
        let dl = DownloadManager::new().unwrap();
        let target = OptimizationTarget::Client(LoaderType::Quilt);

        let (base_url, _server) = spawn_test_server(|_base| {
            let mut routes = HashMap::new();
            routes.insert(
                "/project/P7dR8mSH/version".into(),
                (500, "application/json".into(), b"[]".to_vec()),
            );
            routes.insert(
                "/files/qsl.jar".into(),
                (
                    200,
                    "application/octet-stream".into(),
                    b"qsl-bytes".to_vec(),
                ),
            );
            routes.insert(
                "/files/sodium.jar".into(),
                (
                    200,
                    "application/octet-stream".into(),
                    b"sodium-bytes".to_vec(),
                ),
            );
            routes.insert(
                "/files/extra.jar".into(),
                (
                    200,
                    "application/octet-stream".into(),
                    b"extra-bytes".to_vec(),
                ),
            );
            routes.insert(
                "/files/fapi.jar".into(),
                (
                    200,
                    "application/octet-stream".into(),
                    b"fapi-bytes".to_vec(),
                ),
            );
            routes
        })
        .await;

        let qsl_ver = sample_version(
            "v-qsl",
            "release",
            &["1.21.1"],
            &["quilt"],
            "qsl-1.0.jar",
            &format!("{base_url}/files/qsl.jar"),
        );
        let mut sodium_ver = sample_version(
            "v-sodium",
            "release",
            &["1.21.1"],
            &["fabric"],
            "sodium-1.0.jar",
            &format!("{base_url}/files/sodium.jar"),
        );
        sodium_ver
            .dependencies
            .push(crate::provider::DependencySpec {
                project_id: FABRIC_API.project_id.into(),
                version_id: None,
                relation: crate::provider::DependencyRelation::Required,
            });

        // Case 1: QSL and a Fabric API dependent mod selected together -> skips Fabric API.
        let committed = install_selected_optimizations_with_base(
            BatchInstallContext {
                downloader: &dl,
                db: &db,
                base_url: &base_url,
                target_profile_id: "quilt-batch",
                root_dir: &inst_dir,
                target,
                game_version: "1.21.1",
            },
            &[
                make_suggestion(
                    QSL.project_id,
                    QSL.slug,
                    QSL.title,
                    ContentKind::Mod,
                    qsl_ver,
                ),
                make_suggestion(
                    SODIUM.project_id,
                    SODIUM.slug,
                    SODIUM.title,
                    ContentKind::Mod,
                    sodium_ver,
                ),
            ],
            None,
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(committed.len(), 2);
        assert!(inst_dir.join("mods").join("qsl-1.0.jar").is_file());
        assert!(inst_dir.join("mods").join("sodium-1.0.jar").is_file());

        // Case 2: QSL already installed & active on disk -> Fabric API is still satisfied cleanly.
        let profile_preseeded = "quilt-preseeded";
        std::fs::write(inst_dir.join("mods").join("qsl-existing.jar"), b"qsl-disk").unwrap();
        db.upsert_installed_content(&InstalledContentRecord {
            profile_id: profile_preseeded.into(),
            content_kind: ContentKind::Mod,
            file_name: "qsl-existing.jar".into(),
            sha1: String::new(),
            provider: ProviderSource::Modrinth,
            project_id: QSL.project_id.into(),
            version_id: "v-qsl-existing".into(),
            title: QSL.title.into(),
            version_number: "1.0.0".into(),
            icon_url: None,
            enabled: true,
            installed_at: 0,
        })
        .unwrap();

        let mut extra_ver = sample_version(
            "v-extra",
            "release",
            &["1.21.1"],
            &["fabric"],
            "sodium-extra-1.0.jar",
            &format!("{base_url}/files/extra.jar"),
        );
        extra_ver
            .dependencies
            .push(crate::provider::DependencySpec {
                project_id: FABRIC_API.project_id.into(),
                version_id: None,
                relation: crate::provider::DependencyRelation::Required,
            });
        let committed_pre = install_selected_optimizations_with_base(
            BatchInstallContext {
                downloader: &dl,
                db: &db,
                base_url: &base_url,
                target_profile_id: profile_preseeded,
                root_dir: &inst_dir,
                target,
                game_version: "1.21.1",
            },
            &[make_suggestion(
                SODIUM_EXTRA.project_id,
                SODIUM_EXTRA.slug,
                SODIUM_EXTRA.title,
                ContentKind::Mod,
                extra_ver.clone(),
            )],
            None,
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(committed_pre.len(), 1);
        assert!(inst_dir.join("mods").join("sodium-extra-1.0.jar").is_file());

        // Case 3: QSL DB record exists, but file was deleted from disk -> must NOT treat QSL as active.
        let profile_missing_qsl = "quilt-missing-qsl";
        db.upsert_installed_content(&InstalledContentRecord {
            profile_id: profile_missing_qsl.into(),
            content_kind: ContentKind::Mod,
            file_name: "missing-qsl.jar".into(),
            sha1: String::new(),
            provider: ProviderSource::Modrinth,
            project_id: QSL.project_id.into(),
            version_id: "v-qsl-missing".into(),
            title: QSL.title.into(),
            version_number: "1.0.0".into(),
            icon_url: None,
            enabled: true,
            installed_at: 0,
        })
        .unwrap();
        let (fapi_url, _fapi_server) = spawn_test_server(|base| {
            let mut routes = HashMap::new();
            routes.insert(
                format!("/project/{}", FABRIC_API.project_id),
                (
                    200,
                    "application/json".into(),
                    project_meta_json(
                        FABRIC_API.project_id,
                        FABRIC_API.slug,
                        FABRIC_API.title,
                        "mod",
                    ),
                ),
            );
            routes.insert(
                format!("/project/{}/version", FABRIC_API.project_id),
                (
                    200,
                    "application/json".into(),
                    serde_json::json!([modrinth_version_json(
                        "v-fapi",
                        FABRIC_API.project_id,
                        "release",
                        &["fabric"],
                        (
                            "fabric-api-from-fallback.jar",
                            &format!("{base}/files/fapi.jar"),
                            b"fapi-bytes".len() as u64,
                        ),
                        vec![],
                    )])
                    .to_string()
                    .into_bytes(),
                ),
            );
            routes.insert(
                "/files/fapi.jar".into(),
                (
                    200,
                    "application/octet-stream".into(),
                    b"fapi-bytes".to_vec(),
                ),
            );
            routes.insert(
                "/files/extra.jar".into(),
                (
                    200,
                    "application/octet-stream".into(),
                    b"extra-bytes".to_vec(),
                ),
            );
            routes
        })
        .await;
        let inst_dir_c3 = tmp.join("instance-c3");
        std::fs::create_dir_all(&inst_dir_c3).unwrap();
        extra_ver.files[0].url = format!("{fapi_url}/files/extra.jar");
        let committed_fallback = install_selected_optimizations_with_base(
            BatchInstallContext {
                downloader: &dl,
                db: &db,
                base_url: &fapi_url,
                target_profile_id: profile_missing_qsl,
                root_dir: &inst_dir_c3,
                target,
                game_version: "1.21.1",
            },
            &[make_suggestion(
                SODIUM_EXTRA.project_id,
                SODIUM_EXTRA.slug,
                SODIUM_EXTRA.title,
                ContentKind::Mod,
                extra_ver,
            )],
            None,
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(committed_fallback.len(), 2);
        assert!(inst_dir_c3
            .join("mods")
            .join("fabric-api-from-fallback.jar")
            .is_file());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn incompatible_dependencies_reject_before_writing_any_files() {
        let tmp = std::env::temp_dir().join(format!("thk-opt-incompat-{}", uuid::Uuid::new_v4()));
        let inst_dir = tmp.join("instance");
        let mods_dir = inst_dir.join("mods");
        std::fs::create_dir_all(&mods_dir).unwrap();
        let db = Database::open_in_memory().unwrap();
        let dl = DownloadManager::new().unwrap();
        let target = OptimizationTarget::Client(LoaderType::Fabric);

        // Case 1: Two selected items declare incompatibility -> rejected before download or disk write.
        let mut ver_a = sample_version(
            "v-a",
            "release",
            &["1.21.4"],
            &["fabric"],
            "mod-a.jar",
            "http://127.0.0.1:1/mod-a.jar",
        );
        ver_a.dependencies.push(crate::provider::DependencySpec {
            project_id: "proj-b".into(),
            version_id: None,
            relation: crate::provider::DependencyRelation::Incompatible,
        });
        let ver_b = sample_version(
            "v-b",
            "release",
            &["1.21.4"],
            &["fabric"],
            "mod-b.jar",
            "http://127.0.0.1:1/mod-b.jar",
        );

        let err_selected = install_selected_optimizations_with_base(
            BatchInstallContext {
                downloader: &dl,
                db: &db,
                base_url: "http://127.0.0.1:1",
                target_profile_id: "p-incompat-selected",
                root_dir: &inst_dir,
                target,
                game_version: "1.21.4",
            },
            &[
                make_suggestion("proj-a", "mod-a", "Mod A", ContentKind::Mod, ver_a.clone()),
                make_suggestion("proj-b", "mod-b", "Mod B", ContentKind::Mod, ver_b.clone()),
            ],
            None,
            |_| {},
        )
        .await
        .unwrap_err();
        assert!(matches!(err_selected, thk_core::ThkError::Provider(_)));
        assert_eq!(std::fs::read_dir(&mods_dir).unwrap().count(), 0);

        // Case 2: Transitive pinned required dependency declares incompatibility with another selected mod.
        let (base_url, _server) = spawn_test_server(|base| {
            let mut routes = HashMap::new();
            routes.insert(
                "/project/dep-x".into(),
                (
                    200,
                    "application/json".into(),
                    project_meta_json("dep-x", "dep-x", "Dep X", "mod"),
                ),
            );
            routes.insert(
                "/version/v-dep-x".into(),
                (
                    200,
                    "application/json".into(),
                    modrinth_version_json(
                        "v-dep-x",
                        "dep-x",
                        "release",
                        &["fabric"],
                        (
                            "dep-x.jar",
                            &format!("{base}/files/dep-x.jar"),
                            b"depx-bytes".len() as u64,
                        ),
                        vec![serde_json::json!({
                            "project_id": "proj-b",
                            "version_id": null,
                            "dependency_type": "incompatible"
                        })],
                    )
                    .to_string()
                    .into_bytes(),
                ),
            );
            routes.insert(
                "/files/parent.jar".into(),
                (
                    200,
                    "application/octet-stream".into(),
                    b"parent-bytes".to_vec(),
                ),
            );
            routes.insert(
                "/files/mod-b.jar".into(),
                (200, "application/octet-stream".into(), b"b-bytes".to_vec()),
            );
            routes.insert(
                "/files/dep-x.jar".into(),
                (
                    200,
                    "application/octet-stream".into(),
                    b"depx-bytes".to_vec(),
                ),
            );
            routes
        })
        .await;

        let mut parent_ver = sample_version(
            "v-parent",
            "release",
            &["1.21.4"],
            &["fabric"],
            "parent.jar",
            &format!("{base_url}/files/parent.jar"),
        );
        parent_ver
            .dependencies
            .push(crate::provider::DependencySpec {
                project_id: "dep-x".into(),
                version_id: Some("v-dep-x".into()),
                relation: crate::provider::DependencyRelation::Required,
            });
        let mut ver_b_live = ver_b;
        ver_b_live.files[0].url = format!("{base_url}/files/mod-b.jar");

        let err_transitive = install_selected_optimizations_with_base(
            BatchInstallContext {
                downloader: &dl,
                db: &db,
                base_url: &base_url,
                target_profile_id: "p-incompat-transitive",
                root_dir: &inst_dir,
                target,
                game_version: "1.21.4",
            },
            &[
                make_suggestion(
                    "proj-parent",
                    "parent",
                    "Parent",
                    ContentKind::Mod,
                    parent_ver,
                ),
                make_suggestion("proj-b", "mod-b", "Mod B", ContentKind::Mod, ver_b_live),
            ],
            None,
            |_| {},
        )
        .await
        .unwrap_err();
        assert!(matches!(err_transitive, thk_core::ThkError::Provider(_)));
        assert_eq!(std::fs::read_dir(&mods_dir).unwrap().count(), 0);

        // Case 3: Active incompatible DB record with file on disk rejects; missing/disabled DB record allows install.
        let profile_disk = "p-incompat-disk";
        db.upsert_installed_content(&InstalledContentRecord {
            profile_id: profile_disk.into(),
            content_kind: ContentKind::Mod,
            file_name: "b-existing.jar".into(),
            sha1: String::new(),
            provider: ProviderSource::Modrinth,
            project_id: "proj-b".into(),
            version_id: "v-b".into(),
            title: "Mod B".into(),
            version_number: "1.0".into(),
            icon_url: None,
            enabled: true,
            installed_at: 0,
        })
        .unwrap();
        let existing_b_path = mods_dir.join("b-existing.jar");
        std::fs::write(&existing_b_path, b"b-disk").unwrap();

        let err_disk = install_selected_optimizations_with_base(
            BatchInstallContext {
                downloader: &dl,
                db: &db,
                base_url: &base_url,
                target_profile_id: profile_disk,
                root_dir: &inst_dir,
                target,
                game_version: "1.21.4",
            },
            &[make_suggestion(
                "proj-a",
                "mod-a",
                "Mod A",
                ContentKind::Mod,
                ver_a.clone(),
            )],
            None,
            |_| {},
        )
        .await
        .unwrap_err();
        assert!(matches!(err_disk, thk_core::ThkError::Provider(_)));
        assert!(!mods_dir.join("mod-a.jar").exists());

        // Removing conflicting jar from disk clears the active conflict and lets Mod A install.
        std::fs::remove_file(&existing_b_path).unwrap();
        ver_a.files[0].url = format!("{base_url}/files/parent.jar");
        let ok_after_delete = install_selected_optimizations_with_base(
            BatchInstallContext {
                downloader: &dl,
                db: &db,
                base_url: &base_url,
                target_profile_id: profile_disk,
                root_dir: &inst_dir,
                target,
                game_version: "1.21.4",
            },
            &[make_suggestion(
                "proj-a",
                "mod-a",
                "Mod A",
                ContentKind::Mod,
                ver_a,
            )],
            None,
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(ok_after_delete.len(), 1);
        assert!(mods_dir.join("mod-a.jar").is_file());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn disk_file_and_enabled_checks_trigger_reinstall_for_missing_or_disabled_items() {
        let tmp = std::env::temp_dir().join(format!("thk-opt-disk-{}", uuid::Uuid::new_v4()));
        let inst_dir = tmp.join("instance");
        let mods_dir = inst_dir.join("mods");
        std::fs::create_dir_all(&mods_dir).unwrap();
        let db = Database::open_in_memory().unwrap();
        let dl = DownloadManager::new().unwrap();
        let target = OptimizationTarget::Client(LoaderType::Fabric);

        let (base_url, _server) = spawn_test_server(|base| {
            let mut routes = HashMap::new();
            routes.insert(
                "/project/dep-missing".into(),
                (
                    200,
                    "application/json".into(),
                    project_meta_json("dep-missing", "dep-missing", "Dep Missing", "mod"),
                ),
            );
            routes.insert(
                "/project/dep-missing/version".into(),
                (
                    200,
                    "application/json".into(),
                    serde_json::json!([modrinth_version_json(
                        "v-dep-missing",
                        "dep-missing",
                        "release",
                        &["fabric"],
                        (
                            "dep-missing-restored.jar",
                            &format!("{base}/files/dep-missing.jar"),
                            b"dep-missing-bytes".len() as u64,
                        ),
                        vec![],
                    )])
                    .to_string()
                    .into_bytes(),
                ),
            );
            routes.insert(
                "/project/dep-disabled".into(),
                (
                    200,
                    "application/json".into(),
                    project_meta_json("dep-disabled", "dep-disabled", "Dep Disabled", "mod"),
                ),
            );
            routes.insert(
                "/project/dep-disabled/version".into(),
                (
                    200,
                    "application/json".into(),
                    serde_json::json!([modrinth_version_json(
                        "v-dep-disabled-new",
                        "dep-disabled",
                        "release",
                        &["fabric"],
                        (
                            "dep-disabled-new.jar",
                            &format!("{base}/files/dep-disabled.jar"),
                            b"dep-disabled-bytes".len() as u64,
                        ),
                        vec![],
                    )])
                    .to_string()
                    .into_bytes(),
                ),
            );
            routes.insert(
                "/files/main.jar".into(),
                (
                    200,
                    "application/octet-stream".into(),
                    b"main-bytes".to_vec(),
                ),
            );
            routes.insert(
                "/files/dep-missing.jar".into(),
                (
                    200,
                    "application/octet-stream".into(),
                    b"dep-missing-bytes".to_vec(),
                ),
            );
            routes.insert(
                "/files/dep-disabled.jar".into(),
                (
                    200,
                    "application/octet-stream".into(),
                    b"dep-disabled-bytes".to_vec(),
                ),
            );
            routes
        })
        .await;

        // Case 1: Selected item matches DB version_id + enabled=true, but file is missing on disk -> re-downloads.
        let p_missing_self = "p-missing-self";
        db.upsert_installed_content(&InstalledContentRecord {
            profile_id: p_missing_self.into(),
            content_kind: ContentKind::Mod,
            file_name: "main-mod.jar".into(),
            sha1: String::new(),
            provider: ProviderSource::Modrinth,
            project_id: "proj-main".into(),
            version_id: "v-main".into(),
            title: "Main Mod".into(),
            version_number: "1.0".into(),
            icon_url: None,
            enabled: true,
            installed_at: 0,
        })
        .unwrap();
        let main_ver_simple = sample_version(
            "v-main",
            "release",
            &["1.21.4"],
            &["fabric"],
            "main-mod.jar",
            &format!("{base_url}/files/main.jar"),
        );
        let restored_self = install_selected_optimizations_with_base(
            BatchInstallContext {
                downloader: &dl,
                db: &db,
                base_url: &base_url,
                target_profile_id: p_missing_self,
                root_dir: &inst_dir,
                target,
                game_version: "1.21.4",
            },
            &[make_suggestion(
                "proj-main",
                "main-mod",
                "Main Mod",
                ContentKind::Mod,
                main_ver_simple.clone(),
            )],
            None,
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(restored_self.len(), 1);
        assert!(mods_dir.join("main-mod.jar").is_file());

        // Case 2: Main mod is present on disk, unpinned required dep has missing file & disabled dep has .disabled row.
        let p_deps = "p-deps-check";
        std::fs::write(mods_dir.join("main-present.jar"), b"main-present").unwrap();
        db.upsert_installed_content(&InstalledContentRecord {
            profile_id: p_deps.into(),
            content_kind: ContentKind::Mod,
            file_name: "main-present.jar".into(),
            sha1: String::new(),
            provider: ProviderSource::Modrinth,
            project_id: "proj-main-present".into(),
            version_id: "v-main-present".into(),
            title: "Main Present".into(),
            version_number: "1.0".into(),
            icon_url: None,
            enabled: true,
            installed_at: 0,
        })
        .unwrap();
        db.upsert_installed_content(&InstalledContentRecord {
            profile_id: p_deps.into(),
            content_kind: ContentKind::Mod,
            file_name: "dep-missing-gone.jar".into(),
            sha1: String::new(),
            provider: ProviderSource::Modrinth,
            project_id: "dep-missing".into(),
            version_id: "v-dep-missing-old".into(),
            title: "Dep Missing".into(),
            version_number: "0.9".into(),
            icon_url: None,
            enabled: true,
            installed_at: 0,
        })
        .unwrap();
        std::fs::write(
            mods_dir.join("dep-disabled-old.jar.disabled"),
            b"disabled-old",
        )
        .unwrap();
        db.upsert_installed_content(&InstalledContentRecord {
            profile_id: p_deps.into(),
            content_kind: ContentKind::Mod,
            file_name: "dep-disabled-old.jar".into(),
            sha1: String::new(),
            provider: ProviderSource::Modrinth,
            project_id: "dep-disabled".into(),
            version_id: "v-dep-disabled-old".into(),
            title: "Dep Disabled".into(),
            version_number: "0.9".into(),
            icon_url: None,
            enabled: false,
            installed_at: 0,
        })
        .unwrap();

        let mut main_present_ver = sample_version(
            "v-main-present",
            "release",
            &["1.21.4"],
            &["fabric"],
            "main-present.jar",
            &format!("{base_url}/files/main.jar"),
        );
        main_present_ver
            .dependencies
            .push(crate::provider::DependencySpec {
                project_id: "dep-missing".into(),
                version_id: None,
                relation: crate::provider::DependencyRelation::Required,
            });
        main_present_ver
            .dependencies
            .push(crate::provider::DependencySpec {
                project_id: "dep-disabled".into(),
                version_id: None,
                relation: crate::provider::DependencyRelation::Required,
            });

        let dep_commits = install_selected_optimizations_with_base(
            BatchInstallContext {
                downloader: &dl,
                db: &db,
                base_url: &base_url,
                target_profile_id: p_deps,
                root_dir: &inst_dir,
                target,
                game_version: "1.21.4",
            },
            &[make_suggestion(
                "proj-main-present",
                "main-present",
                "Main Present",
                ContentKind::Mod,
                main_present_ver,
            )],
            None,
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(dep_commits.len(), 2);
        assert!(mods_dir.join("dep-missing-restored.jar").is_file());
        let rows = db.list_installed_content(p_deps, ContentKind::Mod).unwrap();
        let disabled_row = rows
            .iter()
            .find(|r| r.project_id == "dep-disabled")
            .expect("disabled dep updated");
        assert_eq!(disabled_row.version_id, "v-dep-disabled-new");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
