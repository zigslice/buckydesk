use crate::command::{Arg, Command, CommandError, Invocation, Report, Surface};
use crate::http::{open_browser, serve, Site};
use serde_json::{Map, Value};
use std::net::{IpAddr, TcpListener};
use std::path::PathBuf;

const BIND: Arg = Arg {
    name: "bind",
    takes_value: true,
    summary: "监听地址，默认 127.0.0.1:1016",
};
const NO_OPEN: Arg = Arg {
    name: "no-open",
    takes_value: false,
    summary: "监听后不打开浏览器",
};
const FRONTEND: Arg = Arg {
    name: "frontend",
    takes_value: true,
    summary: "页面目录，默认 src/frontend",
};
const GD5: Arg = Arg {
    name: "gd5",
    takes_value: true,
    summary: "gd5 源码目录，默认 src/frontend/vendor/gd5",
};
const NAME: Arg = Arg {
    name: "name",
    takes_value: true,
    summary: "你的名字",
};
const PASSWORD: Arg = Arg {
    name: "password",
    takes_value: true,
    summary: "管理密码",
};
const KEY_FOLDER: Arg = Arg {
    name: "key-folder",
    takes_value: true,
    summary: "钥匙所在的文件夹",
};
const CURRENT: Arg = Arg {
    name: "current",
    takes_value: true,
    summary: "现在的密码",
};
const NEXT: Arg = Arg {
    name: "next",
    takes_value: true,
    summary: "新密码",
};

pub fn root() -> &'static Command {
    &ROOT
}

static ROOT: Command = Command {
    name: "bucky",
    summary: "这台电脑上的工作区，以及用域名从公网打开它",
    surface: Surface::Both,
    args: &[],
    children: &[VERSION, COMMANDS, DESK, CLOUD],
    run: None,
};

static DESK: Command = Command {
    name: "desk",
    summary: "这台电脑上的工作区",
    surface: Surface::Both,
    args: &[],
    children: &[DESK_OPEN, KERNEL, IDENTITY, crate::workspace::WORKSPACE],
    run: None,
};

static CLOUD: Command = Command {
    name: "cloud",
    summary: "用域名从公网打开这个 Zone",
    surface: Surface::Both,
    args: &[],
    children: &[CLOUD_STATUS, CLOUD_DOMAIN, CLOUD_SERVE],
    run: None,
};

static VERSION: Command = Command {
    name: "version",
    summary: "打印本壳的版本",
    surface: Surface::Both,
    args: &[],
    children: &[],
    run: Some(version),
};

static COMMANDS: Command = Command {
    name: "commands",
    summary: "打印命令树",
    surface: Surface::Both,
    args: &[],
    children: &[],
    run: Some(commands),
};

static DESK_OPEN: Command = Command {
    name: "open",
    summary: "在 127.0.0.1:1016 提供页面，并打开浏览器",
    surface: Surface::Cli,
    args: &[BIND, NO_OPEN, FRONTEND, GD5],
    children: &[],
    run: Some(server),
};

const ZONE_DOMAIN: Arg = Arg {
    name: "name",
    takes_value: true,
    summary: "Zone 的域名",
};

static CLOUD_STATUS: Command = Command {
    name: "status",
    summary: "查看域名、DNS 和 80/443 是否对得上",
    surface: Surface::Both,
    args: &[],
    children: &[],
    run: Some(cloud_status),
};

static CLOUD_DOMAIN: Command = Command {
    name: "domain",
    summary: "查看或记下 Zone 的域名",
    surface: Surface::Both,
    args: &[ZONE_DOMAIN],
    children: &[],
    run: Some(cloud_domain),
};

const CLOUD_BIND: Arg = Arg {
    name: "bind",
    takes_value: true,
    summary: "公网监听地址，默认 0.0.0.0:80",
};

const CLOUD_TLS_BIND: Arg = Arg {
    name: "tls-bind",
    takes_value: true,
    summary: "HTTPS 监听地址。入口是 80 时，默认同一地址的 443",
};

const CLOUD_CERT: Arg = Arg {
    name: "cert",
    takes_value: true,
    summary: "证书 PEM。不带时由这台机器签发",
};

const CLOUD_KEY: Arg = Arg {
    name: "key",
    takes_value: true,
    summary: "证书私钥 PEM，和 --cert 一起给出",
};

static CLOUD_SERVE: Command = Command {
    name: "serve",
    summary: "按记下的域名在公网提供工作区",
    surface: Surface::Cli,
    args: &[CLOUD_BIND, CLOUD_TLS_BIND, CLOUD_CERT, CLOUD_KEY],
    children: &[],
    run: Some(cloud_serve),
};

static KERNEL: Command = Command {
    name: "kernel",
    summary: "无界面内核进程",
    surface: Surface::Both,
    args: &[],
    children: &[KERNEL_STATUS, KERNEL_SERVICES, KERNEL_START, KERNEL_STOP],
    run: None,
};

static KERNEL_STATUS: Command = Command {
    name: "status",
    summary: "查看内核是否已附着",
    surface: Surface::Both,
    args: &[],
    children: &[],
    run: Some(kernel_status),
};

static KERNEL_SERVICES: Command = Command {
    name: "services",
    summary: "查看内核服务是否在听",
    surface: Surface::Both,
    args: &[],
    children: &[],
    run: Some(kernel_services),
};

static KERNEL_START: Command = Command {
    name: "start",
    summary: "拉起内核进程",
    surface: Surface::Both,
    args: &[],
    children: &[],
    run: Some(kernel_start),
};

static KERNEL_STOP: Command = Command {
    name: "stop",
    summary: "停止内核进程",
    surface: Surface::Both,
    args: &[],
    children: &[],
    run: Some(kernel_stop),
};

static IDENTITY: Command = Command {
    name: "identity",
    summary: "这台电脑上的工作区身份",
    surface: Surface::Both,
    args: &[],
    children: &[
        IDENTITY_STATUS,
        IDENTITY_ACTIVATE,
        IDENTITY_FINISH,
        IDENTITY_LOGIN,
        IDENTITY_LOGOUT,
        IDENTITY_PASSWORD,
        IDENTITY_RESET,
    ],
    run: None,
};

static IDENTITY_STATUS: Command = Command {
    name: "status",
    summary: "查看工作区身份",
    surface: Surface::Both,
    args: &[],
    children: &[],
    run: Some(identity_status),
};

static IDENTITY_ACTIVATE: Command = Command {
    name: "activate",
    summary: "记下名字、密码和钥匙文件夹，开始准备",
    surface: Surface::Both,
    args: &[NAME, PASSWORD, KEY_FOLDER],
    children: &[],
    run: Some(identity_activate),
};

static IDENTITY_FINISH: Command = Command {
    name: "finish",
    summary: "把准备中的工作区标成就绪",
    surface: Surface::Both,
    args: &[],
    children: &[],
    run: Some(identity_finish),
};

static IDENTITY_LOGIN: Command = Command {
    name: "login",
    summary: "用管理密码进入工作区",
    surface: Surface::Both,
    args: &[PASSWORD],
    children: &[],
    run: Some(identity_login),
};

static IDENTITY_LOGOUT: Command = Command {
    name: "logout",
    summary: "离开这一次，并收回公网入口的登录",
    surface: Surface::Both,
    args: &[],
    children: &[],
    run: Some(identity_logout),
};

static IDENTITY_PASSWORD: Command = Command {
    name: "password",
    summary: "更换管理密码",
    surface: Surface::Both,
    args: &[CURRENT, NEXT],
    children: &[],
    run: Some(identity_password),
};

static IDENTITY_RESET: Command = Command {
    name: "reset",
    summary: "删除这台电脑上记下的工作区身份",
    surface: Surface::Both,
    args: &[],
    children: &[],
    run: Some(identity_reset),
};

fn version(_: &Invocation) -> Result<Report, CommandError> {
    let mut data = Map::new();
    data.insert(
        "version".to_string(),
        Value::String(env!("CARGO_PKG_VERSION").to_string()),
    );
    Ok(Report::ok(env!("CARGO_PKG_VERSION")).with_data(data))
}

fn commands(_: &Invocation) -> Result<Report, CommandError> {
    Ok(Report::ok(crate::command::help(root(), &[])))
}

fn server(invocation: &Invocation) -> Result<Report, CommandError> {
    let bind = invocation
        .value("bind")
        .unwrap_or("127.0.0.1:1016")
        .to_string();
    let listener = TcpListener::bind(&bind)
        .map_err(|error| CommandError::Failed(format!("无法监听 {bind}: {error}")))?;
    let addr = listener
        .local_addr()
        .map_err(|error| CommandError::Failed(format!("无法读取监听地址: {error}")))?;
    let url = format!("http://{addr}");
    println!("{url}");
    if !invocation.flag("no-open") {
        if let Err(error) = open_browser(&url) {
            eprintln!("未能打开浏览器: {error}");
            eprintln!("请手动打开 {url}");
        }
    }
    let site = Site {
        frontend: frontend_dir(invocation),
        gd5: gd5_dir(invocation),
        domain: None,
    };
    serve(listener, site).map_err(|error| CommandError::Failed(error.to_string()))?;
    Ok(Report::ok(url))
}

fn kernel_status(_: &Invocation) -> Result<Report, CommandError> {
    Ok(crate::desk::with_desk(crate::desk::kernel_status_report))
}

fn kernel_services(_: &Invocation) -> Result<Report, CommandError> {
    Ok(crate::desk::with_desk(crate::desk::services_report))
}

fn cloud_status(_: &Invocation) -> Result<Report, CommandError> {
    match crate::desk::with_desk(crate::cloud::load) {
        Ok(state) => Ok(crate::cloud::status_report(&state)),
        Err(report) => Ok(report),
    }
}

fn cloud_domain(invocation: &Invocation) -> Result<Report, CommandError> {
    let name = invocation.value("name").unwrap_or("");
    let saved = crate::desk::with_desk(|| {
        if name.is_empty() {
            crate::cloud::load()
        } else {
            crate::cloud::set_domain(name)
        }
    });
    match saved {
        Ok(state) => Ok(crate::cloud::status_report(&state)),
        Err(report) => Ok(report),
    }
}

fn cloud_serve(invocation: &Invocation) -> Result<Report, CommandError> {
    let state = match crate::desk::with_desk(crate::cloud::load) {
        Ok(state) => state,
        Err(report) => return Ok(report),
    };
    if state.domain.is_empty() {
        return Ok(Report::fail("先记下域名。"));
    }
    let cert = invocation.value("cert");
    let key = invocation.value("key");
    if cert.is_some() != key.is_some() {
        return Ok(Report::fail("证书和私钥要一起给出。"));
    }
    let bind = invocation.value("bind").unwrap_or("0.0.0.0:80").to_string();
    let listener = TcpListener::bind(&bind)
        .map_err(|error| CommandError::Failed(format!("无法监听 {bind}: {error}")))?;
    let address = listener
        .local_addr()
        .map_err(|error| CommandError::Failed(format!("无法读取监听地址: {error}")))?;
    let tls_bind = invocation
        .value("tls-bind")
        .map(str::to_string)
        .or_else(|| default_tls_bind(&address));
    let tls = match tls_bind {
        Some(tls_bind) => match prepare_tls(invocation, &state.domain, cert, key, &tls_bind) {
            Ok(opened) => Some(opened),
            Err(message) => {
                eprintln!("{message}");
                None
            }
        },
        None => None,
    };
    let (tls_address, certificate) = match &tls {
        Some(opened) => (opened.address.clone(), opened.kind.to_string()),
        None => (String::new(), String::new()),
    };
    if let Err(report) = crate::desk::with_desk(|| {
        crate::cloud::set_entrance(&address.to_string(), &tls_address, &certificate)
    }) {
        return Ok(report);
    }
    let url = crate::cloud::public_url(&state.domain, address.port());
    println!("{url}");
    if let Some(opened) = tls {
        if let Some(port) = opened
            .address
            .rsplit_once(':')
            .and_then(|(_, port)| port.parse().ok())
        {
            println!("{}", crate::cloud::https_url(&state.domain, port));
        }
        std::thread::spawn(move || {
            if let Err(error) = crate::http::serve_tls(opened.listener, opened.site, opened.config)
            {
                eprintln!("HTTPS 停了: {error}");
            }
        });
    }
    let site = crate::http::Site {
        frontend: frontend_dir(invocation),
        gd5: gd5_dir(invocation),
        domain: Some(state.domain),
    };
    crate::http::serve(listener, site).map_err(|error| CommandError::Failed(error.to_string()))?;
    Ok(Report::ok(url))
}

struct TlsDoor {
    listener: TcpListener,
    site: crate::http::Site,
    config: std::sync::Arc<rustls::ServerConfig>,
    address: String,
    kind: &'static str,
}

fn default_tls_bind(address: &std::net::SocketAddr) -> Option<String> {
    if address.port() != 80 {
        return None;
    }
    Some(match address.ip() {
        IpAddr::V4(ip) => format!("{ip}:443"),
        IpAddr::V6(ip) => format!("[{ip}]:443"),
    })
}

fn prepare_tls(
    invocation: &Invocation,
    domain: &str,
    cert: Option<&str>,
    key: Option<&str>,
    tls_bind: &str,
) -> Result<TlsDoor, String> {
    let identity = crate::desk::with_desk(|| crate::cloud::identity_for(domain, cert, key))
        .map_err(|report| report.summary)?;
    let config = crate::cloud::tls_server_config(&identity).map_err(|report| report.summary)?;
    let listener = TcpListener::bind(tls_bind)
        .map_err(|error| format!("HTTPS 暂时听不了 {tls_bind}：{error}"))?;
    let address = listener
        .local_addr()
        .map_err(|error| format!("无法读取 HTTPS 地址: {error}"))?
        .to_string();
    let site = crate::http::Site {
        frontend: frontend_dir(invocation),
        gd5: gd5_dir(invocation),
        domain: Some(domain.to_string()),
    };
    Ok(TlsDoor {
        listener,
        site,
        config,
        address,
        kind: identity.kind,
    })
}

fn kernel_start(_: &Invocation) -> Result<Report, CommandError> {
    Ok(Report::fail("这台电脑上还没有可拉起的内核。"))
}

fn kernel_stop(_: &Invocation) -> Result<Report, CommandError> {
    Ok(Report::fail("这台电脑上还没有可拉起的内核。"))
}

fn identity_status(_: &Invocation) -> Result<Report, CommandError> {
    Ok(crate::desk::with_desk(crate::desk::identity_status_report))
}

fn identity_activate(invocation: &Invocation) -> Result<Report, CommandError> {
    let name = invocation.value("name").unwrap_or("");
    let password = invocation.value("password").unwrap_or("");
    let key_folder = invocation.value("key-folder").unwrap_or("");
    Ok(crate::desk::with_desk(|| {
        crate::desk::activate_report(name, password, key_folder)
    }))
}

fn identity_finish(_: &Invocation) -> Result<Report, CommandError> {
    Ok(crate::desk::with_desk(crate::desk::finish_report))
}

fn identity_logout(_: &Invocation) -> Result<Report, CommandError> {
    Ok(crate::desk::with_desk(crate::desk::logout_report))
}

fn identity_login(invocation: &Invocation) -> Result<Report, CommandError> {
    let password = invocation.value("password").unwrap_or("").to_string();
    Ok(crate::desk::with_desk(|| {
        crate::desk::login_report(&password)
    }))
}

fn identity_password(invocation: &Invocation) -> Result<Report, CommandError> {
    let current = invocation.value("current").unwrap_or("").to_string();
    let next = invocation.value("next").unwrap_or("").to_string();
    Ok(crate::desk::with_desk(|| {
        crate::desk::password_report(&current, &next)
    }))
}

fn identity_reset(_: &Invocation) -> Result<Report, CommandError> {
    Ok(crate::desk::with_desk(crate::desk::reset_report))
}

fn frontend_dir(invocation: &Invocation) -> PathBuf {
    if let Some(path) = invocation.value("frontend") {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::var_os("BUCKYDESK_FRONTEND") {
        return PathBuf::from(path);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../frontend")
}

fn gd5_dir(invocation: &Invocation) -> PathBuf {
    if let Some(path) = invocation.value("gd5") {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::var_os("BUCKYDESK_GD5") {
        return PathBuf::from(path);
    }
    frontend_dir(invocation).join("vendor/gd5")
}
