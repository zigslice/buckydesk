use crate::command::Report;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::cell::Cell;
use std::fs;
use std::io;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

const HASH_SUFFIX: &[u8] = b".buckyos";
const PROBE_TIMEOUT: Duration = Duration::from_millis(200);

static GATE: Mutex<()> = Mutex::new(());
thread_local! {
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Serializes identity files and the kernel-address environment.
/// Re-entering on the same thread is allowed so a test can hold the gate
/// while a command reads the same variables.
pub fn with_desk<T>(body: impl FnOnce() -> T) -> T {
    struct Leave;
    impl Drop for Leave {
        fn drop(&mut self) {
            DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
        }
    }

    let outer = DEPTH.with(|depth| {
        let outer = depth.get() == 0;
        depth.set(depth.get() + 1);
        outer
    });
    let _leave = Leave;
    let _gate = if outer {
        Some(GATE.lock().unwrap_or_else(|poison| poison.into_inner()))
    } else {
        None
    };
    body()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct DeskFile {
    phase: String,
    name: String,
    #[serde(rename = "passwordHash")]
    password_hash: String,
    #[serde(rename = "keyFolder")]
    key_folder: String,
}

pub fn hash_password(name: &str, password: &str) -> String {
    let digest = Sha256::new()
        .chain_update(password.as_bytes())
        .chain_update(name.as_bytes())
        .chain_update(HASH_SUFFIX)
        .finalize();
    base64::engine::general_purpose::STANDARD.encode(digest)
}

pub fn state_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("BUCKYDESK_STATE") {
        return PathBuf::from(path);
    }
    home_dir().join(".buckydesk")
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn desk_path() -> PathBuf {
    state_dir().join("desk.json")
}

fn load_file() -> Result<Option<DeskFile>, ()> {
    let path = desk_path();
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path).map_err(|_| ())?;
    serde_json::from_str(&text).map(Some).map_err(|_| ())
}

fn store_file(desk: &DeskFile) -> Result<(), ()> {
    let dir = state_dir();
    fs::create_dir_all(&dir).map_err(|_| ())?;
    let path = dir.join("desk.json");
    let temporary = dir.join(".desk.json.tmp");
    let text = serde_json::to_string_pretty(desk).map_err(|_| ())?;
    fs::write(&temporary, text).map_err(|_| ())?;
    restrict_to_owner(&temporary)?;
    fs::rename(&temporary, &path).map_err(|_| ())?;
    Ok(())
}

fn restrict_to_owner(path: &Path) -> Result<(), ()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|_| ())?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

fn remove_file() -> Result<(), ()> {
    match fs::remove_file(desk_path()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(()),
    }
}

pub fn kernel_endpoint() -> String {
    std::env::var("BUCKYDESK_KERNEL").unwrap_or_else(|_| "127.0.0.1:3180".to_string())
}

pub fn kernel_attached() -> bool {
    let endpoint = kernel_endpoint();
    let Ok(mut addresses) = endpoint.to_socket_addrs() else {
        return false;
    };
    let Some(address) = addresses.next() else {
        return false;
    };
    TcpStream::connect_timeout(&address, PROBE_TIMEOUT).is_ok()
}

fn public_data(desk: &DeskFile) -> Map<String, Value> {
    let mut data = Map::new();
    data.insert("configured".to_string(), Value::Bool(true));
    data.insert("phase".to_string(), Value::String(desk.phase.clone()));
    data.insert("name".to_string(), Value::String(desk.name.clone()));
    data.insert(
        "keyFolder".to_string(),
        Value::String(desk.key_folder.clone()),
    );
    data
}

fn empty_data() -> Map<String, Value> {
    let mut data = Map::new();
    data.insert("configured".to_string(), Value::Bool(false));
    data
}

pub fn identity_status_report() -> Report {
    match load_file() {
        Ok(Some(desk)) => {
            let summary = if desk.phase == "ready" {
                "工作区已就绪。"
            } else {
                "上次没有完成。"
            };
            Report::ok(summary).with_data(public_data(&desk))
        }
        Ok(None) => Report::ok("尚未写入本机身份。").with_data(empty_data()),
        Err(()) => Report::fail("工作区暂时打不开。"),
    }
}

pub fn activate_report(name: &str, password: &str, key_folder: &str) -> Report {
    let name = name.trim();
    let key_folder = key_folder.trim();
    if name.is_empty() {
        return Report::fail("先写下你的名字。");
    }
    if password.chars().count() < 8 {
        return Report::fail("密码长度不能小于 8 位");
    }
    if key_folder.is_empty() {
        return Report::fail("先选定钥匙要放的位置。");
    }
    match load_file() {
        Ok(Some(desk)) if desk.phase == "ready" => Report::fail("已经有工作区了。"),
        Ok(Some(_)) => Report::fail("上次没有完成。"),
        Ok(None) => {
            let desk = DeskFile {
                phase: "partial".to_string(),
                name: name.to_string(),
                password_hash: hash_password(name, password),
                key_folder: key_folder.to_string(),
            };
            match store_file(&desk) {
                Ok(()) => Report::ok("正在准备你的工作区。").with_data(public_data(&desk)),
                Err(()) => Report::fail("身份写不进去。"),
            }
        }
        Err(()) => Report::fail("工作区暂时打不开。"),
    }
}

pub fn finish_report() -> Report {
    match load_file() {
        Ok(Some(mut desk)) => {
            desk.phase = "ready".to_string();
            match store_file(&desk) {
                Ok(()) => Report::ok("工作区已就绪。").with_data(public_data(&desk)),
                Err(()) => Report::fail("身份写不进去。"),
            }
        }
        Ok(None) => Report::fail("还没有开始准备。"),
        Err(()) => Report::fail("工作区暂时打不开。"),
    }
}

pub fn login_report(password: &str) -> Report {
    match load_file() {
        Ok(Some(desk)) if desk.phase != "ready" => Report::fail("上次没有完成。"),
        Ok(Some(desk)) => {
            if hash_password(&desk.name, password) != desk.password_hash {
                return Report::fail("密码不正确。");
            }
            Report::ok("已进入工作区。").with_data(public_data(&desk))
        }
        Ok(None) => Report::fail("尚未写入本机身份。"),
        Err(()) => Report::fail("工作区暂时打不开。"),
    }
}

pub fn password_report(current: &str, next: &str) -> Report {
    if next.chars().count() < 8 {
        return Report::fail("密码长度不能小于 8 位");
    }
    match load_file() {
        Ok(Some(desk)) if desk.phase != "ready" => Report::fail("上次没有完成。"),
        Ok(Some(mut desk)) => {
            if hash_password(&desk.name, current) != desk.password_hash {
                return Report::fail("现在的密码不正确。");
            }
            desk.password_hash = hash_password(&desk.name, next);
            match store_file(&desk) {
                Ok(()) => Report::ok("密码已更新。").with_data(public_data(&desk)),
                Err(()) => Report::fail("身份写不进去。"),
            }
        }
        Ok(None) => Report::fail("尚未写入本机身份。"),
        Err(()) => Report::fail("工作区暂时打不开。"),
    }
}

pub fn reset_report() -> Report {
    match remove_file() {
        Ok(()) => Report::ok("工作区已重置。").with_data(empty_data()),
        Err(()) => Report::fail("身份写不进去。"),
    }
}

pub fn kernel_status_report() -> Report {
    let attached = kernel_attached();
    let mut data = Map::new();
    data.insert("attached".to_string(), Value::Bool(attached));
    let summary = if attached {
        "内核已经在这台电脑上运行。"
    } else {
        "内核还没有在这台电脑上运行。"
    };
    Report::ok(summary).with_data(data)
}

/// Holds the desk gate and points identity plus the kernel probe at a scratch directory.
#[cfg(test)]
pub fn isolate<T>(dir: &Path, kernel: &str, body: impl FnOnce() -> T) -> T {
    with_desk(|| {
        let previous_state = std::env::var_os("BUCKYDESK_STATE");
        let previous_kernel = std::env::var_os("BUCKYDESK_KERNEL");
        std::env::set_var("BUCKYDESK_STATE", dir);
        std::env::set_var("BUCKYDESK_KERNEL", kernel);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
        restore_var("BUCKYDESK_STATE", previous_state);
        restore_var("BUCKYDESK_KERNEL", previous_kernel);
        match result {
            Ok(value) => value,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    })
}

#[cfg(test)]
fn restore_var(name: &str, previous: Option<std::ffi::OsString>) {
    match previous {
        Some(value) => std::env::set_var(name, value),
        None => std::env::remove_var(name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("buckydesk-{}-{}", std::process::id(), nanos));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn password_hash_matches_the_upstream_formula() {
        assert_eq!(
            hash_password("alice", "12345678"),
            "rHzkpnm67kJXknD6pjOtKUr2tlMDdUw3z2TIZfo1uxs="
        );
    }

    #[test]
    fn activate_then_finish_login_and_reset() {
        let dir = scratch();
        isolate(&dir, "127.0.0.1:1", || {
            let short = activate_report("alice", "short", "keys");
            assert!(!short.ok);
            assert!(short.summary.contains("8"));

            let partial = activate_report("alice", "12345678", "文档/buckydesk-key");
            assert!(partial.ok);
            assert_eq!(
                partial.data.get("phase").and_then(Value::as_str),
                Some("partial")
            );
            assert!(partial.data.get("passwordHash").is_none());
            let saved = fs::read_to_string(dir.join("desk.json")).unwrap();
            assert!(!saved.contains("12345678"));
            assert!(saved.contains("rHzkpnm67kJXknD6pjOtKUr2tlMDdUw3z2TIZfo1uxs="));
            assert!(!dir.join("文档").exists());

            let again = activate_report("bob", "12345678", "other");
            assert!(!again.ok);
            assert_eq!(again.summary, "上次没有完成。");

            let ready = finish_report();
            assert!(ready.ok);
            assert_eq!(
                ready.data.get("phase").and_then(Value::as_str),
                Some("ready")
            );
            let again_ready = finish_report();
            assert!(again_ready.ok);

            let occupied = activate_report("bob", "12345678", "other");
            assert_eq!(occupied.summary, "已经有工作区了。");

            let wrong = login_report("nope-nope");
            assert_eq!(wrong.summary, "密码不正确。");
            let right = login_report("12345678");
            assert!(right.ok);
            assert_eq!(
                right.data.get("name").and_then(Value::as_str),
                Some("alice")
            );

            let changed = password_report("12345678", "87654321");
            assert!(changed.ok);
            assert!(!login_report("12345678").ok);
            assert!(login_report("87654321").ok);

            let cleared = reset_report();
            assert!(cleared.ok);
            assert!(!dir.join("desk.json").exists());
            let status = identity_status_report();
            assert_eq!(
                status.data.get("configured").and_then(Value::as_bool),
                Some(false)
            );
        });
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn kernel_probe_sees_an_open_port_and_a_closed_one() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let dir = scratch();
        isolate(&dir, &address.to_string(), || {
            let report = kernel_status_report();
            assert_eq!(
                report.data.get("attached").and_then(Value::as_bool),
                Some(true)
            );
            assert_eq!(report.summary, "内核已经在这台电脑上运行。");
        });
        drop(listener);
        isolate(&dir, "127.0.0.1:1", || {
            let report = kernel_status_report();
            assert_eq!(
                report.data.get("attached").and_then(Value::as_bool),
                Some(false)
            );
            assert_eq!(report.summary, "内核还没有在这台电脑上运行。");
        });
        let _ = fs::remove_dir_all(dir);
    }
}
