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

pub fn root() -> &'static Command {
    &ROOT
}

static ROOT: Command = Command {
    name: "buckydesk",
    summary: "本机 BuckyOS 无界面内核的命令壳",
    surface: Surface::Both,
    args: &[],
    children: &[VERSION, COMMANDS, SERVER, KERNEL, IDENTITY],
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

static SERVER: Command = Command {
    name: "server",
    summary: "在 127.0.0.1:1016 提供页面，并打开浏览器",
    surface: Surface::Cli,
    args: &[BIND, NO_OPEN, FRONTEND, GD5],
    children: &[],
    run: Some(server),
};

static KERNEL: Command = Command {
    name: "kernel",
    summary: "无界面内核进程",
    surface: Surface::Both,
    args: &[],
    children: &[KERNEL_STATUS, KERNEL_START, KERNEL_STOP],
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
    summary: "本机 Zone 身份",
    surface: Surface::Both,
    args: &[],
    children: &[IDENTITY_STATUS, IDENTITY_ACTIVATE],
    run: None,
};

static IDENTITY_STATUS: Command = Command {
    name: "status",
    summary: "查看身份文件是否已写入",
    surface: Surface::Both,
    args: &[],
    children: &[],
    run: Some(identity_status),
};

static IDENTITY_ACTIVATE: Command = Command {
    name: "activate",
    summary: "写本机身份并交给内核",
    surface: Surface::Both,
    args: &[],
    children: &[],
    run: Some(identity_activate),
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
    let mut data = Map::new();
    data.insert("attached".to_string(), Value::Bool(false));
    Ok(Report::ok("内核尚未附着。").with_data(data))
}

fn kernel_start(_: &Invocation) -> Result<Report, CommandError> {
    Ok(Report::fail("内核启动尚未实现。"))
}

fn kernel_stop(_: &Invocation) -> Result<Report, CommandError> {
    Ok(Report::fail("内核停止尚未实现。"))
}

fn identity_status(_: &Invocation) -> Result<Report, CommandError> {
    let mut data = Map::new();
    data.insert("configured".to_string(), Value::Bool(false));
    Ok(Report::ok("尚未写入本机身份。").with_data(data))
}

fn identity_activate(_: &Invocation) -> Result<Report, CommandError> {
    Ok(Report::fail("身份写入尚未实现。"))
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
