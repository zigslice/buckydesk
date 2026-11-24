#![allow(dead_code)]

use crate::root;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvisionStatus {
    NotInstalled,
    Unactivated,
    Partial,
    Configured,
    Invalid,
}

impl ProvisionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProvisionStatus::NotInstalled => "not_installed",
            ProvisionStatus::Unactivated => "unactivated",
            ProvisionStatus::Partial => "partial",
            ProvisionStatus::Configured => "configured",
            ProvisionStatus::Invalid => "invalid",
        }
    }
}

impl std::fmt::Display for ProvisionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let desc = match self {
            ProvisionStatus::NotInstalled => "未安装内核",
            ProvisionStatus::Unactivated => "待激活",
            ProvisionStatus::Partial => "激活未完成",
            ProvisionStatus::Configured => "已配置",
            ProvisionStatus::Invalid => "配置无效",
        };
        write!(f, "{desc}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeIdentity {
    pub zone_id: String,
    pub node_name: String,
    pub device_did: String,
}

pub fn check_provision_status() -> ProvisionStatus {
    let root = root::buckyos_root();
    if !root.exists() {
        return ProvisionStatus::NotInstalled;
    }

    let node_daemon = root::node_daemon_bin();
    if !node_daemon.exists() {
        return ProvisionStatus::NotInstalled;
    }

    let identity_path = root::node_identity_path();
    let zone_config_path = root::zone_config_path();

    if !identity_path.exists() && !zone_config_path.exists() {
        return ProvisionStatus::Unactivated;
    }

    if identity_path.exists() != zone_config_path.exists() {
        return ProvisionStatus::Partial;
    }

    if let Ok(content) = fs::read_to_string(&identity_path) {
        if serde_json::from_str::<Value>(&content).is_ok() {
            return ProvisionStatus::Configured;
        }
    }

    ProvisionStatus::Invalid
}

pub fn load_node_identity() -> Option<NodeIdentity> {
    let path = root::node_identity_path();
    let content = fs::read_to_string(&path).ok()?;
    let value: Value = serde_json::from_str(&content).ok()?;

    Some(NodeIdentity {
        zone_id: value.get("zone_id")?.as_str()?.to_string(),
        node_name: value.get("node_name")?.as_str()?.to_string(),
        device_did: value.get("device_did")?.as_str()?.to_string(),
    })
}

pub fn hash_password(username: &str, password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hasher.update(username.as_bytes());
    hasher.update(b".buckyos");
    BASE64.encode(hasher.finalize())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivationParams {
    pub display_name: String,
    pub password: String,
    pub zone_name: Option<String>,
}

impl ActivationParams {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.display_name.trim().is_empty() {
            return Err("显示名不能为空");
        }
        if self.password.len() < 8 {
            return Err("密码至少8位");
        }
        Ok(())
    }
}

pub fn generate_local_zone_name(display_name: &str) -> String {
    let clean_name: String = display_name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .take(32)
        .collect();
    if clean_name.is_empty() {
        "local".to_string()
    } else {
        clean_name.to_lowercase()
    }
}

pub fn create_zone_config(zone_name: &str) -> Value {
    json!({
        "zone_id": format!("did:web:{zone_name}.local"),
        "zone_name": zone_name,
        "created_at": chrono_timestamp(),
    })
}

fn chrono_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}000", duration.as_secs())
}

pub fn write_zone_config(config: &Value) -> std::io::Result<()> {
    let path = root::zone_config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(config)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(&path, content)
}

pub fn create_node_identity(zone_id: &str, node_name: &str, device_did: &str) -> Value {
    json!({
        "zone_id": zone_id,
        "node_name": node_name,
        "device_did": device_did,
        "is_ood": true,
        "support_container": false,
    })
}

pub fn write_node_identity(identity: &Value) -> std::io::Result<()> {
    let path = root::node_identity_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(identity)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(&path, content)
}
