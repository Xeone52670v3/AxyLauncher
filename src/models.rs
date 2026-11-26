#![allow(dead_code)]
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const BUILTIN_CF_KEY: &str =
    "$2a$10$bL4bIL5pUWqfcO7KQtnMReakwtfHbNKh6v1uTpKlzhwoueEJQnPnm";

pub const DEFAULT_FREESM_META_URL: &str = "https://meta.freesmlauncher.org/v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum AuthAgentBackend {
    #[default]
    AuthlibInjector,
    Loki,
    Disabled,
}

impl AuthAgentBackend {
    pub fn from_index(idx: i32) -> Self {
        match idx {
            1 => AuthAgentBackend::Loki,
            2 => AuthAgentBackend::Disabled,
            _ => AuthAgentBackend::AuthlibInjector,
        }
    }

    pub fn to_index(&self) -> i32 {
        match self {
            AuthAgentBackend::AuthlibInjector => 0,
            AuthAgentBackend::Loki => 1,
            AuthAgentBackend::Disabled => 2,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            AuthAgentBackend::AuthlibInjector => "authlib-injector",
            AuthAgentBackend::Loki => "unmojang/Loki",
            AuthAgentBackend::Disabled => "Отключён",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    PrismZip,
    CurseForgeZip,
    ModrinthMrpack,
}

impl ExportFormat {
    pub fn from_str_tag(s: &str) -> Self {
        match s {
            "curseforge" | "cf" => ExportFormat::CurseForgeZip,
            "modrinth" | "mrpack" => ExportFormat::ModrinthMrpack,
            _ => ExportFormat::PrismZip,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ExportFormat::PrismZip => "Prism / Freesm (.zip)",
            ExportFormat::CurseForgeZip => "CurseForge (.zip)",
            ExportFormat::ModrinthMrpack => "Modrinth (.mrpack)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    Modpack,
    Mod,
    ResourcePack,
    Shader,
    DataPack,
    World,
}

impl AssetKind {
    pub fn from_str_tag(s: &str) -> Self {
        match s {
            "modpack" => AssetKind::Modpack,
            "mod" => AssetKind::Mod,
            "resourcepack" => AssetKind::ResourcePack,
            "shader" => AssetKind::Shader,
            "datapack" => AssetKind::DataPack,
            "world" => AssetKind::World,
            _ => AssetKind::Mod,
        }
    }

    pub fn modrinth_type(&self) -> &'static str {
        match self {
            AssetKind::Modpack => "modpack",
            AssetKind::Mod => "mod",
            AssetKind::ResourcePack => "resourcepack",
            AssetKind::Shader => "shader",
            AssetKind::DataPack => "datapack",
            AssetKind::World => "datapack",
        }
    }

    pub fn curseforge_class_id(&self) -> &'static str {
        match self {
            AssetKind::Modpack => "4471",
            AssetKind::Mod => "6",
            AssetKind::ResourcePack => "12",
            AssetKind::Shader => "6552",
            AssetKind::DataPack => "6945",
            AssetKind::World => "17",
        }
    }

    pub fn subfolder(&self) -> &'static str {
        match self {
            AssetKind::Modpack => ".",
            AssetKind::Mod => "mods",
            AssetKind::ResourcePack => "resourcepacks",
            AssetKind::Shader => "shaderpacks",
            AssetKind::DataPack => "datapacks",
            AssetKind::World => "saves",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LoaderType {
    Vanilla,
    Fabric,
    Forge,
    NeoForge,
    Quilt,
    LiteLoader,
    LabyMod,
    OptiFine,
    Horizon,
}

impl LoaderType {
    pub fn as_str(&self) -> &'static str {
        match self {
            LoaderType::Vanilla => "Vanilla",
            LoaderType::Fabric => "Fabric",
            LoaderType::Forge => "Forge",
            LoaderType::NeoForge => "NeoForge",
            LoaderType::Quilt => "Quilt",
            LoaderType::LiteLoader => "LiteLoader",
            LoaderType::LabyMod => "LabyMod",
            LoaderType::OptiFine => "OptiFine",
            LoaderType::Horizon => "Horizon (Rust)",
        }
    }

    pub fn from_index(idx: i32) -> Self {
        match idx {
            1 => LoaderType::Fabric,
            2 => LoaderType::Forge,
            3 => LoaderType::NeoForge,
            4 => LoaderType::Quilt,
            5 => LoaderType::LiteLoader,
            6 => LoaderType::LabyMod,
            7 => LoaderType::OptiFine,
            8 => LoaderType::Horizon,
            _ => LoaderType::Vanilla,
        }
    }

    pub fn to_index(&self) -> i32 {
        match self {
            LoaderType::Vanilla => 0,
            LoaderType::Fabric => 1,
            LoaderType::Forge => 2,
            LoaderType::NeoForge => 3,
            LoaderType::Quilt => 4,
            LoaderType::LiteLoader => 5,
            LoaderType::LabyMod => 6,
            LoaderType::OptiFine => 7,
            LoaderType::Horizon => 8,
        }
    }

    pub fn prism_uid(&self) -> Option<&'static str> {
        match self {
            LoaderType::Vanilla => Some("net.minecraft"),
            LoaderType::Fabric => Some("net.fabricmc.fabric-loader"),
            LoaderType::Forge => Some("net.minecraftforge"),
            LoaderType::NeoForge => Some("net.neoforged"),
            LoaderType::Quilt => Some("org.quiltmc.quilt-loader"),
            LoaderType::LiteLoader => Some("com.mumfrey.liteloader"),
            LoaderType::LabyMod | LoaderType::OptiFine | LoaderType::Horizon => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalProvider {
    Modrinth,
    CurseForge,
    Ftb,
    Technic,
    AtLauncher,
}

impl ExternalProvider {
    pub fn from_index(idx: i32) -> Self {
        match idx {
            1 => ExternalProvider::CurseForge,
            2 => ExternalProvider::Ftb,
            3 => ExternalProvider::Technic,
            4 => ExternalProvider::AtLauncher,
            _ => ExternalProvider::Modrinth,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ExternalProvider::Modrinth => "Modrinth",
            ExternalProvider::CurseForge => "CurseForge",
            ExternalProvider::Ftb => "FTB",
            ExternalProvider::Technic => "Technic",
            ExternalProvider::AtLauncher => "ATLauncher",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalPackItem {
    pub id: String,
    pub provider: String,
    pub title: String,
    pub author: String,
    pub description: String,
    pub downloads: String,
    pub mc_version: String,
    pub loader: String,
    pub download_url: Option<String>,
    #[serde(default)]
    pub icon_url: String,
}

#[derive(Debug, Clone)]
pub struct NewsItem {
    pub title: String,
    pub summary: String,
    pub date: String,
    pub source: String,
    pub url: String,
    pub image_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AccountType {
    Offline,
    Microsoft,
    ElyBy,
    LittleSkin,
    CustomYggdrasil,
}

impl AccountType {
    pub fn label(&self) -> &'static str {
        match self {
            AccountType::Offline => "Offline",
            AccountType::Microsoft => "Microsoft",
            AccountType::ElyBy => "Ely.by",
            AccountType::LittleSkin => "LittleSkin",
            AccountType::CustomYggdrasil => "Yggdrasil",
        }
    }

    pub fn user_type_arg(&self) -> &'static str {
        match self {
            AccountType::Microsoft => "msa",
            AccountType::Offline => "legacy",
            AccountType::ElyBy | AccountType::LittleSkin | AccountType::CustomYggdrasil => "mojang",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub account_type: AccountType,
    pub auth_server_url: Option<String>,
    #[serde(default)]
    pub auth_login_url: Option<String>,
    #[serde(default)]
    pub auth_refresh_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ComponentKind {
    Lwjgl,
    Minecraft,
    Mappings,
    Loader,
    JarMod,
    ReplaceJar,
    JavaAgent,
    CustomPatch,
}

impl ComponentKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ComponentKind::Lwjgl => "LWJGL",
            ComponentKind::Minecraft => "Minecraft",
            ComponentKind::Mappings => "Mappings",
            ComponentKind::Loader => "Loader",
            ComponentKind::JarMod => "JarMod",
            ComponentKind::ReplaceJar => "ReplaceJar",
            ComponentKind::JavaAgent => "JavaAgent",
            ComponentKind::CustomPatch => "CustomPatch",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceComponent {
    pub uid: String,
    pub name: String,
    pub version: String,
    pub kind: ComponentKind,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub file_path: String,
    #[serde(default)]
    pub extra_args: String,
    #[serde(default)]
    pub customized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub group: String,
    pub mc_version: String,
    pub loader: LoaderType,
    pub loader_version: String,
    #[serde(default)]
    pub icon_id: i32,
    #[serde(default)]
    pub custom_icon_path: String,
    #[serde(default)]
    pub optifine_enabled: bool,
    #[serde(default)]
    pub optifine_version: Option<String>,
    #[serde(default)]
    pub notes: String,

    // Auto-join (Freesm / Prism MinecraftSettingsWidget: server OR singleplayer world)
    #[serde(default)]
    pub enable_auto_join: bool,
    #[serde(default)]
    pub quick_play_server: String,
    #[serde(default)]
    pub quick_play_world: String,

    // Per-instance Account override
    #[serde(default)]
    pub override_account: bool,
    #[serde(default)]
    pub custom_account_id: Option<String>,

    // Per-instance Ely.by & Skin Patcher & Discord override (Freesm)
    #[serde(default)]
    pub override_elyby: bool,
    #[serde(default = "default_elyby_mode")]
    pub custom_elyby_skin_mode: i32, // 0=Never, 1=Always, 2=With Ely.by, 3=Ely.by & Offline
    #[serde(default)]
    pub custom_auth_backend: i32, // 0=authlib-injector, 1=Loki, 2=Disabled
    #[serde(default = "default_patcher_ver")]
    pub custom_auth_version: String,
    #[serde(default)]
    pub override_discord: bool,
    #[serde(default = "default_true")]
    pub custom_discord_rpc: bool,

    // Per-instance Global DataPacks & Playtime override
    #[serde(default)]
    pub override_datapacks: bool,
    #[serde(default)]
    pub custom_datapacks_path: String,
    #[serde(default)]
    pub override_playtime: bool,
    #[serde(default = "default_true")]
    pub custom_record_playtime: bool,

    // Java overrides (Freesm JavaSettingsWidget)
    #[serde(default)]
    pub override_java: bool,
    #[serde(default)]
    pub custom_java_path: Option<String>,
    #[serde(default)]
    pub custom_skip_java_compat: bool,

    #[serde(default)]
    pub override_memory: bool,
    #[serde(default)]
    pub custom_min_memory_mb: Option<u32>,
    #[serde(default)]
    pub custom_max_memory_mb: Option<u32>,
    #[serde(default)]
    pub custom_permgen_mb: Option<u32>,

    #[serde(default)]
    pub override_jvm_args: bool,
    #[serde(default)]
    pub custom_jvm_args: Option<String>,

    // Window & Screenshots overrides (Freesm MinecraftSettingsWidget)
    #[serde(default)]
    pub override_window: bool,
    #[serde(default)]
    pub custom_window_width: Option<u32>,
    #[serde(default)]
    pub custom_window_height: Option<u32>,
    #[serde(default)]
    pub custom_start_maximized: bool,
    #[serde(default)]
    pub custom_quit_after_close: bool,
    #[serde(default = "default_true")]
    pub custom_copy_screenshots: bool,

    // Commands override
    #[serde(default)]
    pub override_commands: bool,
    #[serde(default)]
    pub custom_pre_launch_cmd: Option<String>,
    #[serde(default)]
    pub custom_wrapper_cmd: Option<String>,
    #[serde(default)]
    pub custom_post_exit_cmd: Option<String>,

    // Environment override
    #[serde(default)]
    pub override_env: bool,
    #[serde(default)]
    pub custom_env_vars: Option<String>,

    // Native Libraries, Legacy Tweaks & Performance overrides (Freesm Tweaks)
    #[serde(default)]
    pub override_linux_tweaks: bool,
    #[serde(default)]
    pub custom_legacy_online_fixes: bool,
    #[serde(default)]
    pub custom_use_system_glfw: bool,
    #[serde(default)]
    pub custom_glfw_path: String,
    #[serde(default)]
    pub custom_use_system_openal: bool,
    #[serde(default)]
    pub custom_openal_path: String,
    #[serde(default)]
    pub custom_use_discrete_gpu: bool,
    #[serde(default)]
    pub custom_enable_gamemode: bool,
    #[serde(default)]
    pub custom_enable_mangohud: bool,
    #[serde(default)]
    pub custom_use_zink: bool,

    // Console overrides
    #[serde(default)]
    pub override_console: bool,
    #[serde(default)]
    pub custom_show_console_on_launch: bool,
    #[serde(default)]
    pub custom_show_console_on_error: bool,
    #[serde(default)]
    pub custom_hide_console_on_exit: bool,
    #[serde(default)]
    pub custom_close_after_launch: bool,

    // Advanced: Custom Loader override & Prism-style Components Stack
    #[serde(default)]
    pub use_custom_loader: bool,
    #[serde(default)]
    pub custom_loader_name: String,
    #[serde(default)]
    pub custom_loader_path: String,
    #[serde(default)]
    pub custom_loader_main_class: String,
    #[serde(default)]
    pub components: Vec<InstanceComponent>,

    // Managed Modpack metadata (for version updates / rollbacks in Instance Editor)
    #[serde(default)]
    pub modpack_provider: String,
    #[serde(default)]
    pub modpack_id: String,
    #[serde(default)]
    pub modpack_version_id: String,
    #[serde(default)]
    pub modpack_version_name: String,

    #[serde(default)]
    pub last_played: Option<String>,
    #[serde(default)]
    pub last_played_ts: i64,
    #[serde(default)]
    pub total_playtime_minutes: u64,
}

impl Instance {
    pub fn ensure_default_components(&mut self) {
        if !self.components.is_empty() {
            return;
        }
        if self.loader != LoaderType::Horizon {
            self.components.push(InstanceComponent {
                uid: "org.lwjgl3".to_string(),
                name: "LWJGL 3".to_string(),
                version: "auto".to_string(),
                kind: ComponentKind::Lwjgl,
                enabled: true,
                file_path: String::new(),
                extra_args: String::new(),
                customized: false,
            });
        }
        self.components.push(InstanceComponent {
            uid: "net.minecraft".to_string(),
            name: "Minecraft".to_string(),
            version: self.mc_version.clone(),
            kind: ComponentKind::Minecraft,
            enabled: true,
            file_path: String::new(),
            extra_args: String::new(),
            customized: false,
        });
        if matches!(self.loader, LoaderType::Fabric | LoaderType::Quilt) {
            self.components.push(InstanceComponent {
                uid: "net.fabricmc.intermediary".to_string(),
                name: "Intermediary Mappings".to_string(),
                version: self.mc_version.clone(),
                kind: ComponentKind::Mappings,
                enabled: true,
                file_path: String::new(),
                extra_args: String::new(),
                customized: false,
            });
        }
        if self.loader != LoaderType::Vanilla {
            let uid = self
                .loader
                .prism_uid()
                .unwrap_or(self.loader.as_str())
                .to_string();
            let name = match self.loader {
                LoaderType::Fabric => "Fabric Loader",
                LoaderType::Forge => "Forge",
                LoaderType::NeoForge => "NeoForge",
                LoaderType::Quilt => "Quilt Loader",
                LoaderType::LiteLoader => "LiteLoader",
                LoaderType::LabyMod => "LabyMod 4",
                LoaderType::OptiFine => "OptiFine",
                LoaderType::Horizon => "Horizon (Rust)",
                LoaderType::Vanilla => "Vanilla",
            };
            self.components.push(InstanceComponent {
                uid,
                name: name.to_string(),
                version: self.loader_version.clone(),
                kind: ComponentKind::Loader,
                enabled: true,
                file_path: String::new(),
                extra_args: String::new(),
                customized: false,
            });
        }
        if self.optifine_enabled && self.loader == LoaderType::Forge {
            self.components.push(InstanceComponent {
                uid: "optifine.OptiFine".to_string(),
                name: "OptiFine".to_string(),
                version: self.optifine_version.clone().unwrap_or_else(|| "auto".to_string()),
                kind: ComponentKind::CustomPatch,
                enabled: true,
                file_path: String::new(),
                extra_args: String::new(),
                customized: false,
            });
        }
    }

    pub fn has_custom_overrides(&self) -> bool {
        self.override_account
            || self.override_java
            || self.override_memory
            || self.override_jvm_args
            || self.override_window
            || self.override_commands
            || self.override_env
            || self.override_linux_tweaks
            || self.override_console
            || self.override_elyby
            || self.override_discord
            || self.override_datapacks
            || self.override_playtime
            || self.enable_auto_join
            || self.use_custom_loader
            || !self.quick_play_server.trim().is_empty()
            || !self.quick_play_world.trim().is_empty()
    }

    pub fn playtime_label(&self, always_hours: bool) -> String {
        if self.total_playtime_minutes == 0 {
            self.last_played
                .clone()
                .unwrap_or_else(|| "Готов".to_string())
        } else if always_hours {
            format!("{:.1} ч", self.total_playtime_minutes as f64 / 60.0)
        } else if self.total_playtime_minutes >= 60 {
            format!(
                "{} ч {} мин",
                self.total_playtime_minutes / 60,
                self.total_playtime_minutes % 60
            )
        } else {
            format!("{} мин", self.total_playtime_minutes)
        }
    }

    pub fn new_default(
        id: String,
        name: String,
        group: String,
        mc_version: String,
        loader: LoaderType,
        loader_version: String,
        optifine_enabled: bool,
        optifine_version: Option<String>,
        status: &str,
    ) -> Self {
        let icon_id = crate::auth::random_icon_id();
        Self {
            id,
            name,
            group,
            mc_version,
            loader,
            loader_version,
            icon_id,
            custom_icon_path: String::new(),
            optifine_enabled,
            optifine_version,
            notes: String::new(),
            enable_auto_join: false,
            quick_play_server: String::new(),
            quick_play_world: String::new(),
            override_account: false,
            custom_account_id: None,
            override_elyby: false,
            custom_elyby_skin_mode: default_elyby_mode(),
            custom_auth_backend: 0,
            custom_auth_version: default_patcher_ver(),
            override_discord: false,
            custom_discord_rpc: true,
            override_datapacks: false,
            custom_datapacks_path: String::new(),
            override_playtime: false,
            custom_record_playtime: true,
            override_java: false,
            custom_java_path: None,
            custom_skip_java_compat: false,
            override_memory: false,
            custom_min_memory_mb: None,
            custom_max_memory_mb: None,
            custom_permgen_mb: None,
            override_jvm_args: false,
            custom_jvm_args: None,
            override_window: false,
            custom_window_width: None,
            custom_window_height: None,
            custom_start_maximized: false,
            custom_quit_after_close: false,
            custom_copy_screenshots: true,
            override_commands: false,
            custom_pre_launch_cmd: None,
            custom_wrapper_cmd: None,
            custom_post_exit_cmd: None,
            override_env: false,
            custom_env_vars: None,
            override_linux_tweaks: false,
            custom_legacy_online_fixes: false,
            custom_use_system_glfw: false,
            custom_glfw_path: String::new(),
            custom_use_system_openal: false,
            custom_openal_path: String::new(),
            custom_use_discrete_gpu: false,
            custom_enable_gamemode: false,
            custom_enable_mangohud: false,
            custom_use_zink: false,
            override_console: false,
            custom_show_console_on_launch: false,
            custom_show_console_on_error: true,
            custom_hide_console_on_exit: false,
            custom_close_after_launch: false,
            use_custom_loader: false,
            custom_loader_name: String::new(),
            custom_loader_path: String::new(),
            custom_loader_main_class: String::new(),
            components: Vec::new(),
            modpack_provider: String::new(),
            modpack_id: String::new(),
            modpack_version_id: String::new(),
            modpack_version_name: String::new(),
            last_played: Some(status.to_string()),
            last_played_ts: 0,
            total_playtime_minutes: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherConfig {
    pub instances: Vec<Instance>,
    pub accounts: Vec<Account>,
    pub selected_instance_id: Option<String>,
    pub selected_account_id: Option<String>,

    // === 1. LAUNCHER & APPEARANCE (Freesm LauncherPage + AppearanceWidget) ===
    #[serde(default)]
    pub first_run_completed: bool,
    #[serde(default)]
    pub theme_scheme: i32,
    #[serde(default)]
    pub light_theme: bool,
    #[serde(default = "default_custom_accent")]
    pub custom_accent_hex: String,
    #[serde(default = "default_saved_colors")]
    pub custom_saved_colors: Vec<String>,
    #[serde(default = "default_lang")]
    pub language: String,
    #[serde(default)]
    pub use_system_locale: bool,
    #[serde(default = "default_true")]
    pub use_language_for_formats: bool,

    // Configurable Launcher Folders (exact FreesmLauncher LauncherPage -> Features -> Folders!)
    #[serde(default)]
    pub data_root_dir: String,
    #[serde(default = "default_instances_dir")]
    pub instances_dir: String,
    #[serde(default = "default_mods_dir")]
    pub mods_dir: String,
    #[serde(default = "default_icons_dir")]
    pub icons_dir: String,
    #[serde(default = "default_java_dir")]
    pub java_dir: String,
    #[serde(default = "default_skins_dir")]
    pub skins_dir: String,
    #[serde(default = "default_assets_dir")]
    pub assets_dir: String,
    #[serde(default = "default_libraries_dir")]
    pub libraries_dir: String,
    #[serde(default = "default_exports_dir")]
    pub exports_dir: String,

    #[serde(default = "default_widget_style")]
    pub ui_widget_style: String,
    #[serde(default = "default_icon_theme")]
    pub icon_theme: String,
    #[serde(default)]
    pub ui_font_family: String,
    #[serde(default = "default_card_size")]
    pub instance_card_size: i32, // 0=Compact, 1=Normal, 2=Large
    #[serde(default = "default_true")]
    pub wrap_instance_names: bool,
    #[serde(default)]
    pub lock_toolbars: bool,
    #[serde(default = "default_true")]
    pub show_status_bar: bool,

    #[serde(default = "default_true")]
    pub enable_ui_animations: bool,
    #[serde(default = "default_true")]
    pub wavy_progress_bar: bool,
    #[serde(default)]
    pub enable_snow_effect: bool,
    #[serde(default)]
    pub enable_cat_pack: bool,
    #[serde(default)]
    pub cat_pack_preset: i32,
    #[serde(default = "default_cat_opacity")]
    pub cat_pack_opacity: f32,
    #[serde(default)]
    pub cat_pack_custom_path: String,
    #[serde(default = "default_cat_scale")]
    pub cat_pack_scale_mode: i32, // 0=Fit, 1=Fill, 2=Cover, 3=Stretch
    #[serde(default)]
    pub enable_custom_bg: bool,
    #[serde(default)]
    pub custom_bg_mode: i32, // 0=Launcher, 1=Instances, 2=Both same, 3=Two separate
    #[serde(default)]
    pub custom_bg_launcher_path: String,
    #[serde(default = "default_bg_opacity")]
    pub custom_bg_launcher_opacity: f32,
    #[serde(default = "default_bg_scale")]
    pub custom_bg_launcher_scale: i32, // 0=Fit, 1=Cover, 2=Stretch
    #[serde(default)]
    pub custom_bg_instances_path: String,
    #[serde(default = "default_bg_opacity")]
    pub custom_bg_instances_opacity: f32,
    #[serde(default = "default_bg_scale")]
    pub custom_bg_instances_scale: i32,
    #[serde(default = "default_console_font")]
    pub console_font_family: String,
    #[serde(default = "default_console_font_size")]
    pub console_font_size: u32,
    #[serde(default = "default_true")]
    pub console_colorize_logs: bool,
    #[serde(default = "default_true")]
    pub console_auto_scroll: bool,

    #[serde(default = "default_sort")]
    pub instance_sort_mode: String,
    #[serde(default)]
    pub folder_rename_mode: i32, // 0=Ask, 1=Always rename, 2=Never rename
    #[serde(default = "default_true")]
    pub show_tray_icon: bool,
    #[serde(default)]
    pub minimize_to_tray: bool,
    #[serde(default)]
    pub close_action_mode: i32, // 0 = Спрашивать, 1 = Сворачивать в трей, 2 = Закрывать
    #[serde(default = "default_true")]
    pub show_news_toolbar: bool,
    #[serde(default = "default_true")]
    pub use_native_file_dialog: bool,
    #[serde(default = "default_true")]
    pub discord_rpc_launcher: bool,
    #[serde(default)]
    pub discord_client_id: String,
    #[serde(default = "default_true")]
    pub auto_check_updates: bool,
    #[serde(default = "default_update_hours")]
    pub update_check_interval_hours: u32,
    #[serde(default = "default_update_channel")]
    pub update_channel: String,

    // Mods & Modpacks behavior (Freesm LauncherPage)
    #[serde(default = "default_true")]
    pub keep_mod_metadata: bool,
    #[serde(default = "default_true")]
    pub install_mod_dependencies: bool,
    #[serde(default)]
    pub detect_mod_incompatibilities: bool,
    #[serde(default)]
    pub check_subfolders_blocked_mods: bool,
    #[serde(default)]
    pub move_blocked_mods: bool,
    #[serde(default = "default_true")]
    pub suggest_update_existing_modpack: bool,
    #[serde(default)]
    pub auto_update_mods: bool,
    #[serde(default)]
    pub skip_mod_loader_compat_check: bool,
    #[serde(default = "default_true")]
    pub show_mod_file_sizes: bool,

    // Console & Tasks (Freesm LauncherPage)
    #[serde(default = "default_log_limit")]
    pub log_history_limit: usize,
    #[serde(default)]
    pub stop_logging_on_overflow: bool,
    #[serde(default = "default_task_limit")]
    pub concurrent_task_limit: usize,
    #[serde(default = "default_retry_limit")]
    pub download_retry_limit: usize,
    #[serde(default = "default_true")]
    pub use_offline_meta_cache: bool,

    // === 2. MINECRAFT (Freesm MinecraftSettingsWidget) ===
    pub window_width: u32,
    pub window_height: u32,
    #[serde(default)]
    pub start_maximized: bool,
    #[serde(default)]
    pub close_after_launch: bool,
    #[serde(default)]
    pub quit_after_game_window_closed: bool,
    #[serde(default = "default_true")]
    pub auto_copy_screenshots: bool,
    #[serde(default)]
    pub show_console_on_launch: bool,
    #[serde(default = "default_true")]
    pub show_console_on_error: bool,
    #[serde(default)]
    pub hide_console_on_exit: bool,

    #[serde(default)]
    pub enable_global_datapacks: bool,
    #[serde(default = "default_datapacks_dir")]
    pub global_datapacks_path: String,
    #[serde(default = "default_true")]
    pub record_playtime: bool,
    #[serde(default = "default_true")]
    pub show_playtime_instances: bool,
    #[serde(default = "default_true")]
    pub show_total_playtime: bool,
    #[serde(default)]
    pub always_show_hours: bool,

    // Ely.by & Auth agent & Discord during gameplay (Freesm)
    #[serde(default = "default_elyby_mode")]
    pub elyby_skin_mode: i32, // 0=Never, 1=Always, 2=With Ely.by, 3=Ely.by & Offline
    #[serde(default)]
    pub auth_agent_backend: AuthAgentBackend,
    #[serde(default = "default_patcher_ver")]
    pub auth_agent_version: String,
    #[serde(default)]
    pub custom_patcher_path: String,
    #[serde(default = "default_true")]
    pub patcher_prefetch_metadata: bool,
    #[serde(default)]
    pub discord_rpc_game: bool,

    // Tweaks & Native Libraries & Linux Performance (Freesm)
    #[serde(default)]
    pub legacy_online_fixes: bool,
    #[serde(default)]
    pub force_x11_backend: bool,
    #[serde(default)]
    pub disable_hidpi_scaling: bool,
    #[serde(default)]
    pub use_discrete_gpu: bool,
    #[serde(default)]
    pub enable_gamemode: bool,
    #[serde(default)]
    pub enable_mangohud: bool,
    #[serde(default)]
    pub use_zink: bool,
    #[serde(default)]
    pub use_system_glfw: bool,
    #[serde(default)]
    pub custom_glfw_path: String,
    #[serde(default)]
    pub use_system_openal: bool,
    #[serde(default)]
    pub custom_openal_path: String,

    // === 3. JAVA (Freesm JavaSettingsWidget) ===
    #[serde(default = "default_true")]
    pub auto_detect_java: bool,
    #[serde(default = "default_true")]
    pub auto_download_java: bool,
    #[serde(default)]
    pub skip_java_compat_checks: bool,
    #[serde(default)]
    pub skip_java_startup_prompt: bool,
    pub java_path: String,
    pub min_memory_mb: u32,
    pub max_memory_mb: u32,
    #[serde(default = "default_permgen")]
    pub permgen_mb: u32,
    #[serde(default = "default_true")]
    pub warn_low_memory: bool,
    pub jvm_args: String,

    // === 4. CUSTOM COMMANDS & 5. ENVIRONMENT ===
    #[serde(default)]
    pub pre_launch_cmd: String,
    #[serde(default)]
    pub wrapper_cmd: String,
    #[serde(default)]
    pub post_exit_cmd: String,
    #[serde(default)]
    pub env_vars: String,

    // === 6. PROXY & EXTERNAL TOOLS (Freesm ProxyPage + ExternalToolsPage) ===
    #[serde(default)]
    pub proxy_mode: i32, // 0=None, 1=System, 2=HTTP, 3=SOCKS5
    #[serde(default)]
    pub proxy_enabled: bool,
    #[serde(default = "default_proxy_type")]
    pub proxy_type: String,
    #[serde(default)]
    pub proxy_host: String,
    #[serde(default = "default_proxy_port")]
    pub proxy_port: u16,
    #[serde(default)]
    pub proxy_user: String,
    #[serde(default)]
    pub proxy_pass: String,

    #[serde(default)]
    pub ext_text_editor: String,
    #[serde(default)]
    pub ext_mcedit: String,
    #[serde(default)]
    pub ext_jprofiler: String,
    #[serde(default)]
    pub ext_visualvm: String,

    // === 7. APIs & REPOSITORIES (Freesm APIPage) ===
    #[serde(default = "default_paste_type")]
    pub paste_service_type: String, // "mclo.gs", "0x0.st", "paste.ee"
    #[serde(default)]
    pub paste_base_url: String,
    #[serde(default = "default_meta_url", alias = "freesm_meta_url", alias = "prism_meta_url")]
    pub meta_url: String,
    #[serde(default = "default_assets_url")]
    pub assets_server_url: String,
    #[serde(default)]
    pub legacy_fml_libs_url: String,
    #[serde(default)]
    pub custom_user_agent: String,
    pub labymod_repo_url: String,
    pub optifine_repo_url: String,
    #[serde(default = "default_horizon_repo_url")]
    pub horizon_repo_url: String,
    #[serde(default)]
    pub custom_news_url: String,
    #[serde(default)]
    pub msa_client_id: String,
    #[serde(default)]
    pub modrinth_api_token: String,
    #[serde(default)]
    pub curseforge_key_confirmed: bool,
    #[serde(default = "default_cf_key")]
    pub curseforge_api_key: String,
    #[serde(default = "default_true")]
    pub cf_fallback_to_modrinth: bool,
    #[serde(default)]
    pub technic_api_key: String,
    pub download_concurrency: usize,
    #[serde(default = "default_timeout")]
    pub network_timeout_secs: u64,
}

fn default_true() -> bool { true }
fn default_instances_dir() -> String { "instances".to_string() }
fn default_mods_dir() -> String { "mods".to_string() }
fn default_icons_dir() -> String { "icons".to_string() }
fn default_java_dir() -> String { "java".to_string() }
fn default_skins_dir() -> String { "skins".to_string() }
fn default_assets_dir() -> String { "assets".to_string() }
fn default_libraries_dir() -> String { "libraries".to_string() }
fn default_exports_dir() -> String { "exports".to_string() }
fn default_widget_style() -> String { "Material 3".to_string() }
fn default_icon_theme() -> String { "Material".to_string() }
fn default_card_size() -> i32 { 1 }
fn default_update_channel() -> String { "Стабильный".to_string() }
fn default_custom_accent() -> String { "#7C5CBF".to_string() }
fn default_saved_colors() -> Vec<String> {
    vec![
        "#7C5CBF".to_string(),
        "#1D68C4".to_string(),
        "#E85D4A".to_string(),
        "#1E8E68".to_string(),
        "#D97706".to_string(),
        "#C026D3".to_string(),
    ]
}
fn default_cat_opacity() -> f32 { 0.25 }
fn default_cat_scale() -> i32 { 2 }
fn default_bg_opacity() -> f32 { 0.35 }
fn default_bg_scale() -> i32 { 1 }
fn default_console_font() -> String { "monospace".to_string() }
fn default_console_font_size() -> u32 { 12 }
fn default_update_hours() -> u32 { 24 }
fn default_log_limit() -> usize { 2000 }
fn default_task_limit() -> usize { 6 }
fn default_retry_limit() -> usize { 3 }
fn default_datapacks_dir() -> String { "datapacks".to_string() }
fn default_elyby_mode() -> i32 { 2 }
fn default_patcher_ver() -> String { "Последняя (авто)".to_string() }
fn default_permgen() -> u32 { 128 }
fn default_paste_type() -> String { "mclo.gs".to_string() }
fn default_assets_url() -> String { "https://resources.download.minecraft.net".to_string() }
fn default_lang() -> String { "English (US)".to_string() }
fn default_sort() -> String { "По имени".to_string() }
fn default_meta_url() -> String { DEFAULT_FREESM_META_URL.to_string() }
fn default_horizon_repo_url() -> String { "https://api.github.com/repos/horizon-rust/horizon/releases".to_string() }
fn default_cf_key() -> String { BUILTIN_CF_KEY.to_string() }
fn default_proxy_type() -> String { "HTTP".to_string() }
fn default_proxy_port() -> u16 { 8080 }
fn default_timeout() -> u64 { 60 }

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            instances: vec![],
            accounts: vec![],
            selected_instance_id: None,
            selected_account_id: None,
            first_run_completed: false,
            theme_scheme: 0,
            light_theme: false,
            custom_accent_hex: default_custom_accent(),
            custom_saved_colors: default_saved_colors(),
            language: default_lang(),
            use_system_locale: false,
            use_language_for_formats: true,
            data_root_dir: String::new(),
            instances_dir: default_instances_dir(),
            mods_dir: default_mods_dir(),
            icons_dir: default_icons_dir(),
            java_dir: default_java_dir(),
            skins_dir: default_skins_dir(),
            assets_dir: default_assets_dir(),
            libraries_dir: default_libraries_dir(),
            exports_dir: default_exports_dir(),
            ui_widget_style: default_widget_style(),
            icon_theme: default_icon_theme(),
            ui_font_family: String::new(),
            instance_card_size: default_card_size(),
            wrap_instance_names: true,
            lock_toolbars: false,
            show_status_bar: true,
            enable_ui_animations: true,
            wavy_progress_bar: true,
            enable_snow_effect: false,
            enable_cat_pack: false,
            cat_pack_preset: 1,
            cat_pack_opacity: default_cat_opacity(),
            cat_pack_custom_path: String::new(),
            cat_pack_scale_mode: default_cat_scale(),
            enable_custom_bg: false,
            custom_bg_mode: 0,
            custom_bg_launcher_path: String::new(),
            custom_bg_launcher_opacity: default_bg_opacity(),
            custom_bg_launcher_scale: default_bg_scale(),
            custom_bg_instances_path: String::new(),
            custom_bg_instances_opacity: default_bg_opacity(),
            custom_bg_instances_scale: default_bg_scale(),
            console_font_family: default_console_font(),
            console_font_size: default_console_font_size(),
            console_colorize_logs: true,
            console_auto_scroll: true,
            instance_sort_mode: default_sort(),
            folder_rename_mode: 0,
            show_tray_icon: true,
            minimize_to_tray: false,
            close_action_mode: 0,
            show_news_toolbar: true,
            use_native_file_dialog: true,
            discord_rpc_launcher: true,
            discord_client_id: String::new(),
            auto_check_updates: true,
            update_check_interval_hours: default_update_hours(),
            update_channel: default_update_channel(),
            keep_mod_metadata: true,
            install_mod_dependencies: true,
            detect_mod_incompatibilities: false,
            check_subfolders_blocked_mods: false,
            move_blocked_mods: false,
            suggest_update_existing_modpack: true,
            auto_update_mods: false,
            skip_mod_loader_compat_check: false,
            show_mod_file_sizes: true,
            log_history_limit: default_log_limit(),
            stop_logging_on_overflow: false,
            concurrent_task_limit: default_task_limit(),
            download_retry_limit: default_retry_limit(),
            use_offline_meta_cache: true,
            window_width: 1280,
            window_height: 720,
            start_maximized: false,
            close_after_launch: false,
            quit_after_game_window_closed: false,
            auto_copy_screenshots: true,
            show_console_on_launch: false,
            show_console_on_error: true,
            hide_console_on_exit: false,
            enable_global_datapacks: false,
            global_datapacks_path: default_datapacks_dir(),
            record_playtime: true,
            show_playtime_instances: true,
            show_total_playtime: true,
            always_show_hours: false,
            elyby_skin_mode: default_elyby_mode(),
            auth_agent_backend: AuthAgentBackend::AuthlibInjector,
            auth_agent_version: default_patcher_ver(),
            custom_patcher_path: String::new(),
            patcher_prefetch_metadata: true,
            discord_rpc_game: false,
            legacy_online_fixes: false,
            force_x11_backend: false,
            disable_hidpi_scaling: false,
            use_discrete_gpu: false,
            enable_gamemode: false,
            enable_mangohud: false,
            use_zink: false,
            use_system_glfw: false,
            custom_glfw_path: String::new(),
            use_system_openal: false,
            custom_openal_path: String::new(),
            auto_detect_java: true,
            auto_download_java: true,
            skip_java_compat_checks: false,
            skip_java_startup_prompt: false,
            java_path: "java".to_string(),
            min_memory_mb: 1024,
            max_memory_mb: 4096,
            permgen_mb: default_permgen(),
            warn_low_memory: true,
            jvm_args: "-XX:+UseG1GC -XX:+ParallelRefProcEnabled -XX:MaxGCPauseMillis=200 -XX:+UnlockExperimentalVMOptions -XX:+DisableExplicitGC".to_string(),
            pre_launch_cmd: String::new(),
            wrapper_cmd: String::new(),
            post_exit_cmd: String::new(),
            env_vars: String::new(),
            proxy_mode: 0,
            proxy_enabled: false,
            proxy_type: default_proxy_type(),
            proxy_host: String::new(),
            proxy_port: 8080,
            proxy_user: String::new(),
            proxy_pass: String::new(),
            ext_text_editor: String::new(),
            ext_mcedit: String::new(),
            ext_jprofiler: String::new(),
            ext_visualvm: String::new(),
            paste_service_type: default_paste_type(),
            paste_base_url: String::new(),
            meta_url: default_meta_url(),
            assets_server_url: default_assets_url(),
            legacy_fml_libs_url: String::new(),
            custom_user_agent: String::new(),
            labymod_repo_url: "https://releases.r2.labymod.net".to_string(),
            optifine_repo_url: "https://bmclapi2.bangbang93.com/optifine".to_string(),
            horizon_repo_url: default_horizon_repo_url(),
            custom_news_url: String::new(),
            msa_client_id: String::new(),
            modrinth_api_token: String::new(),
            curseforge_key_confirmed: false,
            curseforge_api_key: default_cf_key(),
            cf_fallback_to_modrinth: true,
            technic_api_key: String::new(),
            download_concurrency: 16,
            network_timeout_secs: 60,
        }
    }
}

impl LauncherConfig {
    fn resolve_subpath(root: &Path, configured: &str, default_sub: &str) -> PathBuf {
        let trimmed = configured.trim();
        if trimmed.is_empty() {
            root.join(default_sub)
        } else {
            let p = PathBuf::from(trimmed);
            if p.is_absolute() {
                p
            } else {
                root.join(p)
            }
        }
    }

    pub fn effective_data_root(&self, base_data_dir: &Path) -> PathBuf {
        let trimmed = self.data_root_dir.trim();
        if trimmed.is_empty() {
            base_data_dir.to_path_buf()
        } else {
            let p = PathBuf::from(trimmed);
            if p.is_absolute() {
                p
            } else {
                base_data_dir.join(p)
            }
        }
    }

    pub fn resolve_instances_dir(&self, base_data_dir: &Path) -> PathBuf {
        let root = self.effective_data_root(base_data_dir);
        Self::resolve_subpath(&root, &self.instances_dir, "instances")
    }

    pub fn resolve_mods_dir(&self, base_data_dir: &Path) -> PathBuf {
        let root = self.effective_data_root(base_data_dir);
        Self::resolve_subpath(&root, &self.mods_dir, "mods")
    }

    pub fn resolve_icons_dir(&self, base_data_dir: &Path) -> PathBuf {
        let root = self.effective_data_root(base_data_dir);
        Self::resolve_subpath(&root, &self.icons_dir, "icons")
    }

    pub fn resolve_java_dir(&self, base_data_dir: &Path) -> PathBuf {
        let root = self.effective_data_root(base_data_dir);
        Self::resolve_subpath(&root, &self.java_dir, "java")
    }

    pub fn resolve_skins_dir(&self, base_data_dir: &Path) -> PathBuf {
        let root = self.effective_data_root(base_data_dir);
        Self::resolve_subpath(&root, &self.skins_dir, "skins")
    }

    pub fn resolve_capes_dir(&self, base_data_dir: &Path) -> PathBuf {
        self.effective_data_root(base_data_dir).join("capes")
    }

    pub fn resolve_assets_dir(&self, base_data_dir: &Path) -> PathBuf {
        let root = self.effective_data_root(base_data_dir);
        Self::resolve_subpath(&root, &self.assets_dir, "assets")
    }

    pub fn resolve_libraries_dir(&self, base_data_dir: &Path) -> PathBuf {
        let root = self.effective_data_root(base_data_dir);
        Self::resolve_subpath(&root, &self.libraries_dir, "libraries")
    }

    pub fn resolve_exports_dir(&self, base_data_dir: &Path) -> PathBuf {
        let root = self.effective_data_root(base_data_dir);
        Self::resolve_subpath(&root, &self.exports_dir, "exports")
    }

    pub fn resolve_horizon_dir(&self, base_data_dir: &Path) -> PathBuf {
        self.effective_data_root(base_data_dir).join("horizon")
    }

    pub fn resolve_labymod_dir(&self, base_data_dir: &Path) -> PathBuf {
        self.effective_data_root(base_data_dir).join("labymod")
    }

    pub fn resolve_icon_packs_dir(&self, base_data_dir: &Path) -> PathBuf {
        self.resolve_icons_dir(base_data_dir).join("packs")
    }

    pub fn resolve_instance_icons_dir(&self, base_data_dir: &Path) -> PathBuf {
        self.resolve_icons_dir(base_data_dir).join("instance_icons")
    }

    pub fn reset_folder_paths(&mut self) {
        self.data_root_dir.clear();
        self.instances_dir = default_instances_dir();
        self.mods_dir = default_mods_dir();
        self.icons_dir = default_icons_dir();
        self.java_dir = default_java_dir();
        self.skins_dir = default_skins_dir();
        self.assets_dir = default_assets_dir();
        self.libraries_dir = default_libraries_dir();
        self.exports_dir = default_exports_dir();
    }
}

// ============================================================================
// Horizon, Meta, LabyMod, OptiFine, Modrinth Schemas
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct HorizonRelease {
    pub tag_name: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub assets: Vec<HorizonReleaseAsset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HorizonReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PrismIndex {
    pub uid: String,
    pub name: String,
    #[serde(default)]
    pub versions: Vec<PrismVersionEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PrismVersionEntry {
    pub version: String,
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub recommended: bool,
    #[serde(default, rename = "releaseTime")]
    pub release_time: Option<String>,
    #[serde(default)]
    pub requires: Vec<PrismRequirement>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PrismRequirement {
    pub uid: String,
    #[serde(default)]
    pub equals: Option<String>,
    #[serde(default)]
    pub suggests: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PrismComponentPackage {
    pub uid: String,
    pub version: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "mainClass")]
    pub main_class: Option<String>,
    #[serde(default, rename = "minecraftArguments")]
    pub minecraft_arguments: Option<String>,
    #[serde(default, rename = "+tweakers")]
    pub tweakers: Vec<String>,
    #[serde(default, rename = "+jvmArgs")]
    pub plus_jvm_args: Vec<String>,
    #[serde(default, rename = "assetIndex")]
    pub asset_index: Option<AssetIndexInfo>,
    #[serde(default, rename = "mainJar")]
    pub main_jar: Option<PrismLibrary>,
    #[serde(default)]
    pub libraries: Vec<PrismLibrary>,
    #[serde(default, rename = "mavenFiles")]
    pub maven_files: Vec<PrismLibrary>,
    #[serde(default)]
    pub requires: Vec<PrismRequirement>,
    #[serde(default, rename = "compatibleJavaMajors")]
    pub compatible_java_majors: Vec<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndexInfo {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PrismLibrary {
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub downloads: Option<LibraryDownloads>,
    #[serde(default)]
    pub rules: Vec<OsRule>,
    #[serde(default)]
    pub natives: HashMap<String, String>,
    #[serde(default, rename = "MMC-hint")]
    pub mmc_hint: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default)]
    pub artifact: Option<ArtifactInfo>,
    #[serde(default)]
    pub classifiers: HashMap<String, ArtifactInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArtifactInfo {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OsRule {
    pub action: String,
    #[serde(default)]
    pub os: Option<OsTarget>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OsTarget {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LabyModLatestManifest {
    #[serde(rename = "labyModVersion")]
    pub labymod_version: String,
    #[serde(rename = "commitReference")]
    pub commit_reference: String,
    pub sha1: String,
    pub size: u64,
    #[serde(default)]
    pub assets: HashMap<String, String>,
    #[serde(default, rename = "minecraftVersions")]
    pub minecraft_versions: Vec<LabyModMcVersion>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LabyModMcVersion {
    pub tag: String,
    pub version: String,
    #[serde(default)]
    pub r#type: String,
    #[serde(rename = "customManifestUrl")]
    pub custom_manifest_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LabyModLibrariesManifest {
    #[serde(default)]
    pub libraries: Vec<LabyModLibrary>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LabyModLibrary {
    pub name: String,
    pub url: String,
    #[serde(default, rename = "minecraftVersion")]
    pub minecraft_version: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MojangStyleVersionJson {
    pub id: String,
    #[serde(default, rename = "mainClass")]
    pub main_class: Option<String>,
    #[serde(default, rename = "assetIndex")]
    pub asset_index: Option<AssetIndexInfo>,
    #[serde(default)]
    pub downloads: Option<MojangDownloads>,
    #[serde(default)]
    pub libraries: Vec<PrismLibrary>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MojangDownloads {
    #[serde(default)]
    pub client: Option<ArtifactInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptiFineVersionEntry {
    pub mcversion: String,
    #[serde(rename = "type")]
    pub of_type: String,
    pub patch: String,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub forge: Option<String>,
}

impl OptiFineVersionEntry {
    pub fn full_version(&self) -> String {
        format!("{}_{}", self.of_type, self.patch)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModrinthSearchResponse {
    pub hits: Vec<ModrinthHit>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModrinthHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub versions: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModrinthVersion {
    pub id: String,
    pub name: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub files: Vec<ModrinthFile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModrinthFile {
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MrpackIndexJson {
    pub name: String,
    #[serde(default, rename = "versionId")]
    pub version_id: String,
    #[serde(default)]
    pub dependencies: HashMap<String, String>,
    #[serde(default)]
    pub files: Vec<MrpackFile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MrpackFile {
    pub path: String,
    #[serde(default)]
    pub downloads: Vec<String>,
}
