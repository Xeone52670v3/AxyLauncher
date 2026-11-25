# Graph Report - minelauncher  (2026-11-23)

## Corpus Check
- Corpus is ~23,491 words - fits in a single context window. You may not need a graph.

## Summary
- 294 nodes · 547 edges · 12 communities (8 shown, 4 thin omitted)
- Extraction: 88% EXTRACTED · 12% INFERRED · 0% AMBIGUOUS · INFERRED: 65 edges (avg confidence: 0.63)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- MetaClient & Modpack Installers
- Config & Metadata Models
- Slint UI & App Controller
- Authentication & Skin Patchers
- Game Process & Java Launcher
- Archive Export & Migration
- Linux System Tray
- Content Asset Kinds
- Auth Agent Backends
- AppImage Packaging
- Cargo Package Manifest

## God Nodes (most connected - your core abstractions)
1. `MetaClient` - 34 edges
2. `main()` - 30 edges
3. `launch_instance()` - 24 edges
4. `LoaderType` - 20 edges
5. `LauncherConfig` - 14 edges
6. `AssetKind` - 12 edges
7. `Instance` - 12 edges
8. `Account` - 11 edges
9. `AuthAgentBackend` - 10 edges
10. `ExternalProvider` - 10 edges

## Surprising Connections (you probably didn't know these)
- `main()` --calls--> `create_offline_account()`  [INFERRED]
  src/main.rs → src/auth.rs
- `main()` --calls--> `authenticate_yggdrasil()`  [INFERRED]
  src/main.rs → src/auth.rs
- `main()` --calls--> `refresh_yggdrasil_account()`  [INFERRED]
  src/main.rs → src/auth.rs
- `launch_instance()` --calls--> `fetch_yggdrasil_prefetched_base64()`  [INFERRED]
  src/launcher.rs → src/auth.rs
- `main()` --calls--> `start_microsoft_device_flow()`  [INFERRED]
  src/main.rs → src/auth.rs

## Import Cycles
- 1-file cycle: `src/tray.rs -> src/tray.rs`

## Communities (12 total, 4 thin omitted)

### Community 0 - "MetaClient & Modpack Installers"
Cohesion: 0.05
Nodes (26): extract_generic_zip(), MetaClient, urlencoding_simple(), ExternalPackItem, ExternalProvider, AtLauncher, CurseForge, Ftb (+18 more)

### Community 1 - "Config & Metadata Models"
Cohesion: 0.07
Nodes (42): ArtifactInfo, AssetIndexInfo, BUILTIN_CF_KEY, default_assets_url(), default_cat_opacity(), default_cat_scale(), default_cf_key(), default_console_font() (+34 more)

### Community 2 - "Slint UI & App Controller"
Cohesion: 0.08
Nodes (23): generate_random_nickname(), random_icon_id(), copy_png_to_clipboard(), detect_system_javas(), append_log_to_ui(), build_filtered_instances(), current_meta(), default_repo_rows() (+15 more)

### Community 3 - "Authentication & Skin Patchers"
Cohesion: 0.09
Nodes (30): authenticate_yggdrasil(), AuthlibInjectorLatest, base64_encode(), complete_xbox_and_minecraft_auth(), create_offline_account(), DeviceCodeResponse, ensure_authlib_injector(), ensure_loki_agent() (+22 more)

### Community 4 - "Game Process & Java Launcher"
Cohesion: 0.10
Nodes (16): check_os_rules(), DownloadTask, ensure_managed_java(), extract_native_jar(), find_first_executable_in_dir(), find_java_bin_in_dir(), find_system_library(), infer_java_major_from_mc() (+8 more)

### Community 5 - "Archive Export & Migration"
Cohesion: 0.10
Nodes (7): add_dir_to_zip(), export_instance_archive(), extract_world_zip(), ExportFormat, CurseForgeZip, ModrinthMrpack, PrismZip

### Community 7 - "Content Asset Kinds"
Cohesion: 0.18
Nodes (7): AssetKind, DataPack, Mod, Modpack, ResourcePack, Shader, World

### Community 8 - "Auth Agent Backends"
Cohesion: 0.29
Nodes (4): AuthAgentBackend, AuthlibInjector, Disabled, Loki

## Knowledge Gaps
- **31 isolated node(s):** `minelauncher`, `AuthlibInjector`, `Loki`, `Disabled`, `PrismZip` (+26 more)
  These have ≤1 connection - possible missing edges. (Counts symbols only; 118 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **4 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `MetaClient` connect `MetaClient & Modpack Installers` to `Config & Metadata Models`, `Slint UI & App Controller`, `Game Process & Java Launcher`, `Archive Export & Migration`?**
  _High betweenness centrality (0.141) - this node is a cross-community bridge._
- **Are the 16 inferred relationships involving `main()` (e.g. with `authenticate_yggdrasil()` and `create_offline_account()`) actually correct?**
  _`main()` has 16 INFERRED edges - model-reasoned connections that need verification._
- **What connects `minelauncher`, `AuthlibInjector`, `Loki` to the rest of the system?**
  _31 weakly-connected nodes found - possible missing edges._
- **Should `MetaClient & Modpack Installers` be split into smaller, more focused modules?**
  _Cohesion score 0.05069124423963134 - nodes in this community are weakly interconnected._
- **Why does `LoaderType` connect `MetaClient & Modpack Installers` to `Config & Metadata Models`?**
  _High betweenness centrality (0.092) - this node is a cross-community bridge._
- **Are the 7 inferred relationships involving `launch_instance()` (e.g. with `ensure_authlib_injector()` and `ensure_loki_agent()`) actually correct?**
  _`launch_instance()` has 7 INFERRED edges - model-reasoned connections that need verification._
- **Should `Config & Metadata Models` be split into smaller, more focused modules?**
  _Cohesion score 0.06561085972850679 - nodes in this community are weakly interconnected._