#![allow(dead_code)]

use crate::root;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductProfile {
    pub edition: String,
    pub capabilities: ProductCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductCapabilities {
    pub container: bool,
    pub user_service_apps: bool,
    pub static_web_apps: bool,
    pub max_running_agents: u32,
}

impl Default for ProductProfile {
    fn default() -> Self {
        Self::lite()
    }
}

impl ProductProfile {
    pub fn lite() -> Self {
        Self {
            edition: "lite".to_string(),
            capabilities: ProductCapabilities {
                container: false,
                user_service_apps: false,
                static_web_apps: true,
                max_running_agents: 1,
            },
        }
    }

    pub fn full() -> Self {
        Self {
            edition: "full".to_string(),
            capabilities: ProductCapabilities {
                container: true,
                user_service_apps: true,
                static_web_apps: true,
                max_running_agents: 0, // 0 = unlimited
            },
        }
    }

    pub fn is_lite(&self) -> bool {
        self.edition == "lite"
    }
}

pub fn load_product_profile() -> Option<ProductProfile> {
    let path = root::product_config_path();
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn save_product_profile(profile: &ProductProfile) -> std::io::Result<()> {
    let path = root::product_config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(profile)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(&path, content)
}

pub fn ensure_product_profile() -> ProductProfile {
    if let Some(profile) = load_product_profile() {
        profile
    } else {
        let profile = ProductProfile::lite();
        let _ = save_product_profile(&profile);
        profile
    }
}
