// src-tauri/src/proxy/mod.rs
//
// Codex Box 本地代理 runtime (BYOK 真链路核心)
//
// 对齐 AITabby/opencodex 的"127.0.0.1 本地代理 + 重写 base_url"模式,
// 完全独立实现,不 spawn 外部进程,不复制其源码/UI/长段文案。
//
// 关键边界:
//   - 仅监听 127.0.0.1,不绑 LAN
//   - secret 走 env 引用,绝不落盘/不上日志
//   - 写入 ~/.codex/config.toml 走 backup → diff → confirm → atomic → rollback
//   - 路由表存于 ~/.codex/codex-box/inject-map.json
pub mod health;
pub mod inject_map;
pub mod lifecycle;
pub mod models_endpoint;
pub mod responses_ws;
pub mod routing;
pub mod server;
pub mod state;
pub mod upstream;
pub mod vision_bridge;

pub use inject_map::{InjectMap, InjectMapEntry};
pub use lifecycle::ProxyError;
pub use routing::{resolve_route, ResolvedRoute};
pub use state::{ProxyState, ProxyStatus, ProxyStatusView};

/// Codex Box 代理默认监听端口(可改,启动时如占用则 +1 探测)
pub const DEFAULT_PROXY_PORT: u16 = 1455;

/// 探测端口最大重试次数
pub const PORT_PROBE_MAX: u8 = 5;

/// inject-map 文件路径(相对 home)
pub const INJECT_MAP_RELATIVE_PATH: &str = ".codex/codex-box/inject-map.json";

/// runtime-state 文件路径(相对 home,持久化代理运行状态供前端刷新可见)
pub const RUNTIME_STATE_RELATIVE_PATH: &str = ".codex/codex-box/runtime-state.json";

/// 备份目录(相对 home)
pub const BACKUP_DIR_RELATIVE_PATH: &str = ".codex/codex-box/backups";

/// Codex Box runtime 数据目录(相对 home)
pub const CODEX_BOX_DIR_RELATIVE_PATH: &str = ".codex/codex-box";
