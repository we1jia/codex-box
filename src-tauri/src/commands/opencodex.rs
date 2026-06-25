use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

const OPENCODEX_SOURCE_PATH: &str = "/Users/liuweijia/Desktop/AI/OpenCodex";
// 直接 spawn Node 跑 dev runner,绕过 `pnpm run web:dev` 触发的 `build:gateway` 步骤。
// 实际 Node 进程内的 cwd 由 run-gateway.cjs 自己 reset 到 OpenCodex 项目根。
const OPENCODEX_GATEWAY_RUNNER: &str = "gateway/dev/run-gateway.cjs";
const DEFAULT_HOST: &str = "127.0.0.1";
const LAN_HOST: &str = "0.0.0.0";
const DEFAULT_PORT: u16 = 3737;
const HEALTH_PATH: &str = "/api/health";

#[derive(Default)]
pub struct OpenCodexManager {
    child: Mutex<Option<ManagedChild>>,
    last_error: Mutex<Option<String>>,
}

struct ManagedChild {
    child: Child,
    host: String,
    port: u16,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodexStatus {
    pub source_path: String,
    pub exists: bool,
    pub package_json_path: String,
    pub config_yaml_path: String,
    pub config_exists: bool,
    pub auth_password_configured: bool,
    pub running: bool,
    pub managed: bool,
    pub pid: Option<u32>,
    pub host: String,
    pub port: u16,
    pub local_url: String,
    pub lan_urls: Vec<String>,
    pub mobile_url: Option<String>,
    pub lan_access_enabled: bool,
    pub mobile_url_reachable: bool,
    pub codex_home: String,
    pub shared_codex_home: String,
    pub runtime_dir: String,
    pub log_path: String,
    pub health_endpoint: String,
    pub health_ok: bool,
    pub health_status: Option<u16>,
    pub last_error: Option<String>,
    pub lan_requires_password: bool,
}

#[tauri::command]
pub fn opencodex_status(state: tauri::State<'_, OpenCodexManager>) -> AppResult<OpenCodexStatus> {
    build_status(&state)
}

#[tauri::command]
pub fn opencodex_start(state: tauri::State<'_, OpenCodexManager>) -> AppResult<OpenCodexStatus> {
    start_with(&state, DEFAULT_HOST.to_string(), DEFAULT_PORT)?;
    build_status(&state)
}

#[tauri::command]
pub fn opencodex_start_lan(
    state: tauri::State<'_, OpenCodexManager>,
) -> AppResult<OpenCodexStatus> {
    ensure_lan_password(&state)?;
    start_with(&state, LAN_HOST.to_string(), DEFAULT_PORT)?;
    build_status(&state)
}

#[tauri::command]
pub fn opencodex_stop(state: tauri::State<'_, OpenCodexManager>) -> AppResult<OpenCodexStatus> {
    stop_managed_child(&state)?;
    build_status(&state)
}

#[tauri::command]
pub fn opencodex_restart(state: tauri::State<'_, OpenCodexManager>) -> AppResult<OpenCodexStatus> {
    stop_managed_child(&state)?;
    start_with(&state, DEFAULT_HOST.to_string(), DEFAULT_PORT)?;
    build_status(&state)
}

#[tauri::command]
pub fn opencodex_restart_lan(
    state: tauri::State<'_, OpenCodexManager>,
) -> AppResult<OpenCodexStatus> {
    ensure_lan_password(&state)?;
    stop_managed_child(&state)?;
    start_with(&state, LAN_HOST.to_string(), DEFAULT_PORT)?;
    build_status(&state)
}

#[tauri::command]
pub fn opencodex_open_url(
    state: tauri::State<'_, OpenCodexManager>,
    kind: Option<String>,
) -> AppResult<OpenCodexStatus> {
    let status = build_status(&state)?;
    let target = match kind.as_deref() {
        Some("mobile") => {
            if !status.mobile_url_reachable {
                return Err(AppError::Command(
                    "手机访问还未启用：请先配置 OpenCodex 访问密码，并用局域网模式启动。"
                        .to_string(),
                ));
            }
            status
                .mobile_url
                .clone()
                .ok_or_else(|| AppError::Command("未检测到局域网访问地址".to_string()))?
        }
        _ => status.local_url.clone(),
    };
    open_target(&target)?;
    Ok(status)
}

#[tauri::command]
pub fn opencodex_open_logs(
    state: tauri::State<'_, OpenCodexManager>,
) -> AppResult<OpenCodexStatus> {
    let status = build_status(&state)?;
    let log_path = PathBuf::from(&status.log_path);
    if log_path.exists() {
        open_reveal(&log_path)?;
    } else if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
        open_target(&parent.to_string_lossy())?;
    }
    Ok(status)
}

fn start_with(state: &OpenCodexManager, host: String, port: u16) -> AppResult<()> {
    {
        let mut guard = state
            .child
            .lock()
            .map_err(|_| AppError::Command("OpenCodex state lock poisoned".to_string()))?;
        if let Some(managed) = guard.as_mut() {
            if managed.child.try_wait()?.is_none() {
                if managed.host == host && managed.port == port {
                    return Ok(());
                }
                return Err(AppError::Command(format!(
                    "OpenCodex 已在 {}:{} 运行，请先停止或使用重启切换模式。",
                    managed.host, managed.port
                )));
            }
            *guard = None;
        }
    }

    let source_dir = PathBuf::from(OPENCODEX_SOURCE_PATH);
    let runner_path = source_dir.join(OPENCODEX_GATEWAY_RUNNER);
    if !source_dir.join("package.json").exists() || !runner_path.exists() {
        let missing = if !runner_path.exists() {
            runner_path.clone()
        } else {
            source_dir.join("package.json")
        };
        set_last_error(
            state,
            format!("OpenCodex not found at {}", missing.display()),
        );
        return Err(AppError::ConfigNotFound(missing.display().to_string()));
    }

    let runtime_dir = runtime_dir()?;
    let log_path = log_path()?;
    fs::create_dir_all(&runtime_dir)?;
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let mut stderr = stdout.try_clone()?;
    writeln!(
        stderr,
        "\n[codex-box] starting OpenCodex host={} port={} source={} runner={} runtime={}",
        host,
        port,
        source_dir.display(),
        runner_path.display(),
        runtime_dir.display()
    )?;

    let config_path = config_yaml_path();
    let codex_home = codex_home()?;
    // 直接 spawn Node 跑 OpenCodex 自带的 dev runner,Box 不复制其源码、不污染 AGPL 边界。
    // env 注入 HOST/PORT/CODEX_HOME/CODEX_WEB_* 让 Box 完全控制运行时位置与配置入口。
    let mut cmd = Command::new("node");
    cmd.arg(&runner_path)
        .current_dir(&source_dir)
        .env("HOST", &host)
        .env("PORT", port.to_string())
        .env("CODEX_HOME", &codex_home)
        .env("CODEX_WEB_RUNTIME_DIR", &runtime_dir)
        .env("CODEX_WEB_CONFIG_PATH", &config_path)
        .env("OPENCODEX_PREFERRED_LANGUAGES", "zh-CN")
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let child = cmd.spawn().map_err(|error| {
        let msg = format!("failed to spawn OpenCodex: {}", error);
        set_last_error(state, msg.clone());
        AppError::Command(msg)
    })?;

    let mut guard = state
        .child
        .lock()
        .map_err(|_| AppError::Command("OpenCodex state lock poisoned".to_string()))?;
    *guard = Some(ManagedChild { child, host, port });
    set_last_error(state, String::new());
    Ok(())
}

fn stop_managed_child(state: &OpenCodexManager) -> AppResult<()> {
    let mut managed = {
        let mut guard = state
            .child
            .lock()
            .map_err(|_| AppError::Command("OpenCodex state lock poisoned".to_string()))?;
        guard.take()
    };

    if let Some(mut managed_child) = managed.take() {
        let pid = managed_child.child.id();
        terminate_process(pid, &mut managed_child.child)?;
        set_last_error(state, String::new());
    }
    Ok(())
}

fn terminate_process(pid: u32, child: &mut Child) -> AppResult<()> {
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .arg("-TERM")
            .arg(format!("-{}", pid))
            .status();
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }

    for _ in 0..20 {
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .arg("-KILL")
            .arg(format!("-{}", pid))
            .status();
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
    let _ = child.wait();
    Ok(())
}

fn build_status(state: &OpenCodexManager) -> AppResult<OpenCodexStatus> {
    let source_path = PathBuf::from(OPENCODEX_SOURCE_PATH);
    let package_json_path = source_path.join("package.json");
    let config_yaml_path = config_yaml_path();
    let runtime_dir = runtime_dir()?;
    let log_path = log_path()?;
    let codex_home = codex_home()?;
    let config_exists = config_yaml_path.exists();
    let auth_password_configured = config_exists && config_has_auth_password(&config_yaml_path);
    let mut host = DEFAULT_HOST.to_string();
    let mut port = DEFAULT_PORT;
    let mut running = false;
    let mut managed = false;
    let mut pid = None;

    {
        let mut guard = state
            .child
            .lock()
            .map_err(|_| AppError::Command("OpenCodex state lock poisoned".to_string()))?;
        if let Some(child) = guard.as_mut() {
            if child.child.try_wait()?.is_none() {
                host = child.host.clone();
                port = child.port;
                running = true;
                managed = true;
                pid = Some(child.child.id());
            } else {
                *guard = None;
            }
        }
    }

    let health = probe_health(port);
    if !running && health.0 {
        running = true;
    }

    let local_url = format!("http://127.0.0.1:{port}");
    let lan_urls = lan_urls(port);
    let mobile_url = lan_urls.first().cloned();
    let lan_access_enabled = is_lan_host(&host) && running;
    let mobile_url_reachable =
        lan_access_enabled && auth_password_configured && mobile_url.is_some();
    let health_endpoint = format!("{local_url}{HEALTH_PATH}");
    let last_error = state
        .last_error
        .lock()
        .ok()
        .and_then(|value| value.clone())
        .filter(|value| !value.is_empty());

    Ok(OpenCodexStatus {
        source_path: source_path.to_string_lossy().to_string(),
        exists: package_json_path.exists() && source_path.join("gateway").exists(),
        package_json_path: package_json_path.to_string_lossy().to_string(),
        config_yaml_path: config_yaml_path.to_string_lossy().to_string(),
        config_exists,
        auth_password_configured,
        running,
        managed,
        pid,
        host,
        port,
        local_url,
        lan_urls,
        mobile_url,
        lan_access_enabled,
        mobile_url_reachable,
        codex_home: codex_home.to_string_lossy().to_string(),
        shared_codex_home: codex_home.to_string_lossy().to_string(),
        runtime_dir: runtime_dir.to_string_lossy().to_string(),
        log_path: log_path.to_string_lossy().to_string(),
        health_endpoint,
        health_ok: health.0,
        health_status: health.1,
        last_error,
        lan_requires_password: true,
    })
}

fn codex_home() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))?;
    Ok(home.join(".codex"))
}

fn runtime_dir() -> AppResult<PathBuf> {
    Ok(codex_home()?
        .join("codex-box")
        .join("opencodex")
        .join("runtime"))
}

fn log_path() -> AppResult<PathBuf> {
    Ok(codex_home()?
        .join("codex-box")
        .join("logs")
        .join("opencodex-gateway.log"))
}

fn config_yaml_path() -> PathBuf {
    PathBuf::from(OPENCODEX_SOURCE_PATH).join("config.yaml")
}

fn ensure_lan_password(state: &OpenCodexManager) -> AppResult<()> {
    let config_path = config_yaml_path();
    if config_path.exists() && config_has_auth_password(&config_path) {
        return Ok(());
    }

    let message = format!(
        "局域网访问需要先在 {} 配置 auth.password",
        config_path.display()
    );
    set_last_error(state, message.clone());
    Err(AppError::Command(message))
}

fn is_lan_host(host: &str) -> bool {
    !matches!(host, "127.0.0.1" | "localhost" | "::1")
}

fn config_has_auth_password(path: &Path) -> bool {
    let Ok(raw) = fs::read_to_string(path) else {
        return false;
    };
    raw.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with("password:")
            && !trimmed.ends_with(":")
            && trimmed.len() > "password:".len()
    })
}

fn probe_health(port: u16) -> (bool, Option<u16>) {
    let addr: SocketAddr = match format!("127.0.0.1:{port}").parse() {
        Ok(addr) => addr,
        Err(_) => return (false, None),
    };
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(250)) else {
        return (false, None);
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
    let request =
        format!("GET {HEALTH_PATH} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return (true, None);
    }
    let mut buf = [0_u8; 128];
    let Ok(n) = stream.read(&mut buf) else {
        return (true, None);
    };
    let head = String::from_utf8_lossy(&buf[..n]);
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok());
    (matches!(status, Some(200)), status)
}

fn lan_urls(port: u16) -> Vec<String> {
    primary_lan_ip()
        .map(|ip| vec![format!("http://{ip}:{port}")])
        .unwrap_or_default()
}

fn primary_lan_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    let ip = addr.ip();
    if ip.is_loopback() {
        None
    } else {
        Some(ip.to_string())
    }
}

fn open_target(target: &str) -> AppResult<()> {
    let status = Command::new("open")
        .arg(target)
        .status()
        .map_err(|error| AppError::Command(format!("open {target}: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::Command(format!(
            "open {target} exited with {status}"
        )))
    }
}

fn open_reveal(path: &Path) -> AppResult<()> {
    let status = Command::new("open")
        .arg("-R")
        .arg(path)
        .status()
        .map_err(|error| AppError::Command(format!("open -R {}: {error}", path.display())))?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::Command(format!(
            "open -R {} exited with {status}",
            path.display()
        )))
    }
}

fn set_last_error(state: &OpenCodexManager, value: String) {
    if let Ok(mut guard) = state.last_error.lock() {
        *guard = if value.is_empty() { None } else { Some(value) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use tempfile::tempdir;

    #[test]
    fn status_uses_local_defaults_and_shared_codex_home() {
        let manager = OpenCodexManager::default();
        let status = build_status(&manager).expect("status");

        assert_eq!(status.source_path, OPENCODEX_SOURCE_PATH);
        assert_eq!(status.host, DEFAULT_HOST);
        assert_eq!(status.port, DEFAULT_PORT);
        assert_eq!(status.local_url, "http://127.0.0.1:3737");
        assert_eq!(status.health_endpoint, "http://127.0.0.1:3737/api/health");
        assert_eq!(status.codex_home, status.shared_codex_home);
        assert!(status.codex_home.ends_with(".codex"));
        assert!(status
            .runtime_dir
            .ends_with(".codex/codex-box/opencodex/runtime"));
        assert!(status
            .log_path
            .ends_with(".codex/codex-box/logs/opencodex-gateway.log"));
        assert!(status.lan_requires_password);
    }

    #[test]
    fn lan_host_detection_only_allows_non_loopback_hosts() {
        assert!(!is_lan_host("127.0.0.1"));
        assert!(!is_lan_host("localhost"));
        assert!(!is_lan_host("::1"));
        assert!(is_lan_host("0.0.0.0"));
        assert!(is_lan_host("192.168.1.23"));
    }

    #[test]
    fn config_password_detection_requires_non_empty_password_line() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("missing.yaml");
        let empty = dir.path().join("empty.yaml");
        let configured = dir.path().join("configured.yaml");
        let nested = dir.path().join("nested.yaml");

        std::fs::write(&empty, "auth:\n  password:\n").unwrap();
        std::fs::write(&configured, "auth:\n  password: sha256-v1:abc\n").unwrap();
        std::fs::write(&nested, "auth:\n  nested_password: abc\n").unwrap();

        assert!(!config_has_auth_password(&missing));
        assert!(!config_has_auth_password(&empty));
        assert!(config_has_auth_password(&configured));
        assert!(!config_has_auth_password(&nested));
    }

    #[test]
    #[ignore = "requires local OpenCodex checkout and starts port 3737"]
    fn start_stop_opencodex_gateway_smoke() {
        if !PathBuf::from(OPENCODEX_SOURCE_PATH)
            .join("package.json")
            .exists()
        {
            panic!("OpenCodex checkout missing at {}", OPENCODEX_SOURCE_PATH);
        }

        let manager = OpenCodexManager::default();
        start_with(&manager, DEFAULT_HOST.to_string(), DEFAULT_PORT).expect("start gateway");
        struct StopGuard<'a>(&'a OpenCodexManager);
        impl Drop for StopGuard<'_> {
            fn drop(&mut self) {
                let _ = stop_managed_child(self.0);
            }
        }
        let _guard = StopGuard(&manager);

        let deadline = Instant::now() + Duration::from_secs(30);
        let mut last = build_status(&manager).expect("initial status");
        while Instant::now() < deadline {
            last = build_status(&manager).expect("status while starting");
            if last.running && last.health_ok {
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }

        assert!(
            last.running,
            "OpenCodex did not enter running state: {last:?}"
        );
        assert!(
            last.health_ok,
            "OpenCodex health did not become ok: {last:?}"
        );
        assert_eq!(last.codex_home, last.shared_codex_home);
        assert!(last.codex_home.ends_with(".codex"));
        assert_eq!(last.local_url, "http://127.0.0.1:3737");
    }
}
