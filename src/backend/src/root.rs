#![allow(dead_code)]

use std::env;
use std::path::PathBuf;

pub fn buckyos_root() -> PathBuf {
    if let Some(path) = env::var_os("BUCKYOS_ROOT") {
        return PathBuf::from(path);
    }
    #[cfg(target_os = "macos")]
    {
        PathBuf::from("/opt/buckyos")
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = env::var_os("APPDATA") {
            PathBuf::from(appdata).join("buckyos")
        } else {
            PathBuf::from("C:\\buckyos")
        }
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        PathBuf::from("/opt/buckyos")
    }
}

pub fn etc_dir() -> PathBuf {
    buckyos_root().join("etc")
}

pub fn bin_dir() -> PathBuf {
    buckyos_root().join("bin")
}

pub fn logs_dir() -> PathBuf {
    buckyos_root().join("logs")
}

pub fn security_dir() -> PathBuf {
    buckyos_root().join("security")
}

pub fn node_identity_path() -> PathBuf {
    etc_dir().join("node_identity.json")
}

pub fn zone_config_path() -> PathBuf {
    etc_dir().join(".zone.json")
}

pub fn product_config_path() -> PathBuf {
    etc_dir().join("product.json")
}

pub fn node_daemon_bin() -> PathBuf {
    let bin = bin_dir().join("node-daemon");
    #[cfg(target_os = "macos")]
    {
        bin.join("BuckyOS Runtime.app/Contents/MacOS/node_daemon")
    }
    #[cfg(target_os = "windows")]
    {
        bin.join("node_daemon.exe")
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        bin.join("node_daemon")
    }
}

pub fn cyfs_gateway_bin() -> PathBuf {
    let bin = bin_dir().join("cyfs-gateway");
    #[cfg(target_os = "windows")]
    {
        bin.join("cyfs_gateway.exe")
    }
    #[cfg(not(target_os = "windows"))]
    {
        bin.join("cyfs_gateway")
    }
}
