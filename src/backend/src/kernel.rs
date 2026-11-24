use crate::{identity, krpc, root};
use serde::{Deserialize, Serialize};
use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KernelState {
    NotInstalled,
    Unactivated,
    Stopped,
    Starting,
    Running,
    Error,
}

impl KernelState {
    pub fn as_str(&self) -> &'static str {
        match self {
            KernelState::NotInstalled => "not_installed",
            KernelState::Unactivated => "unactivated",
            KernelState::Stopped => "stopped",
            KernelState::Starting => "starting",
            KernelState::Running => "running",
            KernelState::Error => "error",
        }
    }
}

impl std::fmt::Display for KernelState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let desc = match self {
            KernelState::NotInstalled => "未安装",
            KernelState::Unactivated => "待激活",
            KernelState::Stopped => "已停止",
            KernelState::Starting => "启动中",
            KernelState::Running => "运行中",
            KernelState::Error => "错误",
        };
        write!(f, "{desc}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelStatus {
    pub state: KernelState,
    pub provision_status: identity::ProvisionStatus,
    pub gateway_available: bool,
    pub zone_id: Option<String>,
    pub node_name: Option<String>,
    pub error: Option<String>,
}

pub fn check_kernel_status() -> KernelStatus {
    let provision_status = identity::check_provision_status();

    match provision_status {
        identity::ProvisionStatus::NotInstalled => {
            return KernelStatus {
                state: KernelState::NotInstalled,
                provision_status,
                gateway_available: false,
                zone_id: None,
                node_name: None,
                error: Some("内核未安装".to_string()),
            };
        }
        identity::ProvisionStatus::Unactivated => {
            return KernelStatus {
                state: KernelState::Unactivated,
                provision_status,
                gateway_available: false,
                zone_id: None,
                node_name: None,
                error: None,
            };
        }
        identity::ProvisionStatus::Partial | identity::ProvisionStatus::Invalid => {
            return KernelStatus {
                state: KernelState::Error,
                provision_status,
                gateway_available: false,
                zone_id: None,
                node_name: None,
                error: Some(format!("身份配置状态: {provision_status}")),
            };
        }
        identity::ProvisionStatus::Configured => {}
    }

    let gateway_available = krpc::is_gateway_available();
    let identity = identity::load_node_identity();

    if !gateway_available {
        return KernelStatus {
            state: KernelState::Stopped,
            provision_status,
            gateway_available: false,
            zone_id: identity.as_ref().map(|i| i.zone_id.clone()),
            node_name: identity.as_ref().map(|i| i.node_name.clone()),
            error: None,
        };
    }

    if is_node_daemon_running() {
        KernelStatus {
            state: KernelState::Running,
            provision_status,
            gateway_available: true,
            zone_id: identity.as_ref().map(|i| i.zone_id.clone()),
            node_name: identity.as_ref().map(|i| i.node_name.clone()),
            error: None,
        }
    } else {
        KernelStatus {
            state: KernelState::Stopped,
            provision_status,
            gateway_available,
            zone_id: identity.as_ref().map(|i| i.zone_id.clone()),
            node_name: identity.as_ref().map(|i| i.node_name.clone()),
            error: None,
        }
    }
}

fn is_node_daemon_running() -> bool {
    #[cfg(unix)]
    {
        use std::process::Command;
        Command::new("pgrep")
            .args(["-x", "node_daemon"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[cfg(windows)]
    {
        use std::process::Command;
        Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq node_daemon.exe", "/NH"])
            .output()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .to_lowercase()
                    .contains("node_daemon")
            })
            .unwrap_or(false)
    }
}

pub fn start_kernel() -> Result<KernelStatus, String> {
    let status = check_kernel_status();

    match status.state {
        KernelState::NotInstalled => {
            return Err("内核未安装，请先安装BuckyOS内核".to_string());
        }
        KernelState::Unactivated => {
            return Err("系统未激活，请先完成身份激活".to_string());
        }
        KernelState::Running => {
            return Ok(status);
        }
        KernelState::Error => {
            return Err(status.error.unwrap_or_else(|| "未知错误".to_string()));
        }
        KernelState::Stopped | KernelState::Starting => {}
    }

    let node_daemon = root::node_daemon_bin();
    if !node_daemon.exists() {
        return Err(format!("找不到 node_daemon: {}", node_daemon.display()));
    }

    let root = root::buckyos_root();
    let logs_dir = root::logs_dir();
    fs::create_dir_all(&logs_dir).map_err(|e| format!("无法创建日志目录: {e}"))?;

    #[cfg(target_os = "macos")]
    {
        start_macos_kernel(&node_daemon)?;
    }

    #[cfg(target_os = "windows")]
    {
        start_windows_kernel(&node_daemon, &root)?;
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        start_linux_kernel(&node_daemon, &root)?;
    }

    for _ in 0..30 {
        thread::sleep(Duration::from_millis(500));
        if krpc::is_gateway_available() {
            return Ok(check_kernel_status());
        }
    }

    Err("启动超时，网关未就绪".to_string())
}

#[cfg(target_os = "macos")]
fn start_macos_kernel(node_daemon: &std::path::Path) -> Result<(), String> {
    let service_name = "com.buckyos.runtime";
    let result = Command::new(node_daemon)
        .args(["service", "start"])
        .output()
        .map_err(|e| format!("无法启动服务: {e}"))?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(format!("服务启动失败: {stderr}"));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn start_windows_kernel(
    node_daemon: &std::path::Path,
    root: &std::path::Path,
) -> Result<(), String> {
    let _child = Command::new(node_daemon)
        .args(["--desktop_daemon"])
        .env("BUCKYOS_ROOT", root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("无法启动 node_daemon: {e}"))?;

    Ok(())
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn start_linux_kernel(
    node_daemon: &std::path::Path,
    root: &std::path::Path,
) -> Result<(), String> {
    let _child = Command::new(node_daemon)
        .args(["--desktop_daemon"])
        .env("BUCKYOS_ROOT", root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("无法启动 node_daemon: {e}"))?;

    Ok(())
}

pub fn stop_kernel() -> Result<KernelStatus, String> {
    let status = check_kernel_status();

    match status.state {
        KernelState::NotInstalled | KernelState::Unactivated | KernelState::Stopped => {
            return Ok(status);
        }
        _ => {}
    }

    #[cfg(target_os = "macos")]
    {
        let node_daemon = root::node_daemon_bin();
        if node_daemon.exists() {
            let _ = Command::new(&node_daemon).args(["service", "stop"]).output();
        }
    }

    #[cfg(unix)]
    {
        let _ = Command::new("pkill").args(["-x", "node_daemon"]).output();
        let _ = Command::new("pkill").args(["-x", "cyfs_gateway"]).output();
    }

    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/IM", "node_daemon.exe", "/F"])
            .output();
        let _ = Command::new("taskkill")
            .args(["/IM", "cyfs_gateway.exe", "/F"])
            .output();
    }

    for _ in 0..10 {
        thread::sleep(Duration::from_millis(500));
        if !is_node_daemon_running() {
            break;
        }
    }

    Ok(check_kernel_status())
}
