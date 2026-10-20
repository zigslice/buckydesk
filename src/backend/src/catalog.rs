use crate::command::{Arg, Command, CommandError, Invocation, Report, Surface};
use crate::http::{open_browser, serve, Site};
use serde_json::{Map, Value};
use std::net::TcpListener;
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
    children: &[CLOUD_STATUS, CLOUD_DOMAIN],
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
    match crate::desk::with_desk(crate::cloud::stored_domain) {
        Ok(domain) => Ok(crate::cloud::status_report(&domain)),
        Err(report) => Ok(report),
    }
}

fn cloud_domain(invocation: &Invocation) -> Result<Report, CommandError> {
    let name = invocation.value("name").unwrap_or("");
    let saved = crate::desk::with_desk(|| {
        if name.is_empty() {
            crate::cloud::stored_domain()
        } else {
            crate::cloud::set_domain(name)
        }
    });
    match saved {
        Ok(domain) => Ok(crate::cloud::status_report(&domain)),
        Err(report) => Ok(report),
    }
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
