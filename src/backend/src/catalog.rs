use crate::command::{Arg, Command, CommandError, Invocation, Report, Surface};
use crate::http::{open_browser, serve, Site};
use crate::{identity, kernel, product, root};
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
const DISPLAY_NAME: Arg = Arg {
    name: "name",
    takes_value: true,
    summary: "显示名称",
};
const PASSWORD: Arg = Arg {
    name: "password",
    takes_value: true,
    summary: "管理员密码（至少8位）",
};
const ZONE_NAME: Arg = Arg {
    name: "zone",
    takes_value: true,
    summary: "Zone名称（可选，默认从显示名生成）",
};

pub fn root() -> &'static Command {
    &ROOT
}

static ROOT: Command = Command {
    name: "buckydesk",
    summary: "本机 BuckyOS 无界面内核的命令壳",
    surface: Surface::Both,
    args: &[],
    children: &[VERSION, COMMANDS, SERVER, KERNEL, IDENTITY, PRODUCT],
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
    summary: "查看内核运行状态",
    surface: Surface::Both,
    args: &[],
    children: &[],
    run: Some(kernel_status_cmd),
};

static KERNEL_START: Command = Command {
    name: "start",
    summary: "拉起内核进程",
    surface: Surface::Cli,
    args: &[],
    children: &[],
    run: Some(kernel_start_cmd),
};

static KERNEL_STOP: Command = Command {
    name: "stop",
    summary: "停止内核进程",
    surface: Surface::Cli,
    args: &[],
    children: &[],
    run: Some(kernel_stop_cmd),
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
    summary: "查看身份文件状态",
    surface: Surface::Both,
    args: &[],
    children: &[],
    run: Some(identity_status_cmd),
};

static IDENTITY_ACTIVATE: Command = Command {
    name: "activate",
    summary: "激活本机身份",
    surface: Surface::Cli,
    args: &[DISPLAY_NAME, PASSWORD, ZONE_NAME],
    children: &[],
    run: Some(identity_activate_cmd),
};

static PRODUCT: Command = Command {
    name: "product",
    summary: "产品配置",
    surface: Surface::Both,
    args: &[],
    children: &[PRODUCT_INFO],
    run: None,
};

static PRODUCT_INFO: Command = Command {
    name: "info",
    summary: "查看产品配置",
    surface: Surface::Both,
    args: &[],
    children: &[],
    run: Some(product_info_cmd),
};

fn version(_: &Invocation) -> Result<Report, CommandError> {
    let mut data = Map::new();
    data.insert(
        "version".to_string(),
        Value::String(env!("CARGO_PKG_VERSION").to_string()),
    );
    data.insert(
        "buckyos_root".to_string(),
        Value::String(root::buckyos_root().display().to_string()),
    );
    Ok(Report::ok(format!(
        "BuckyDesk {} (BUCKYOS_ROOT={})",
        env!("CARGO_PKG_VERSION"),
        root::buckyos_root().display()
    ))
    .with_data(data))
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

fn kernel_status_cmd(_: &Invocation) -> Result<Report, CommandError> {
    let status = kernel::check_kernel_status();
    let mut data = Map::new();

    data.insert(
        "state".to_string(),
        Value::String(status.state.as_str().to_string()),
    );
    data.insert(
        "provision_status".to_string(),
        Value::String(status.provision_status.as_str().to_string()),
    );
    data.insert(
        "gateway_available".to_string(),
        Value::Bool(status.gateway_available),
    );

    if let Some(zone_id) = &status.zone_id {
        data.insert("zone_id".to_string(), Value::String(zone_id.clone()));
    }
    if let Some(node_name) = &status.node_name {
        data.insert("node_name".to_string(), Value::String(node_name.clone()));
    }
    if let Some(error) = &status.error {
        data.insert("error".to_string(), Value::String(error.clone()));
    }

    let summary = match status.state {
        kernel::KernelState::Running => {
            format!(
                "内核运行中。Zone: {}",
                status.zone_id.as_deref().unwrap_or("未知")
            )
        }
        kernel::KernelState::Stopped => "内核已停止。".to_string(),
        kernel::KernelState::Unactivated => "系统待激活。请运行 identity activate 完成激活。".to_string(),
        kernel::KernelState::NotInstalled => "内核未安装。".to_string(),
        kernel::KernelState::Starting => "内核启动中...".to_string(),
        kernel::KernelState::Error => {
            format!(
                "内核错误: {}",
                status.error.as_deref().unwrap_or("未知错误")
            )
        }
    };

    Ok(Report::ok(summary).with_data(data))
}

fn kernel_start_cmd(_: &Invocation) -> Result<Report, CommandError> {
    match kernel::start_kernel() {
        Ok(status) => {
            let mut data = Map::new();
            data.insert(
                "state".to_string(),
                Value::String(status.state.as_str().to_string()),
            );
            data.insert(
                "gateway_available".to_string(),
                Value::Bool(status.gateway_available),
            );
            if let Some(zone_id) = &status.zone_id {
                data.insert("zone_id".to_string(), Value::String(zone_id.clone()));
            }

            if status.state == kernel::KernelState::Running {
                Ok(Report::ok(format!(
                    "内核已启动。Zone: {}",
                    status.zone_id.as_deref().unwrap_or("未知")
                ))
                .with_data(data))
            } else {
                Ok(Report::fail(format!("内核状态: {}", status.state)).with_data(data))
            }
        }
        Err(e) => Err(CommandError::Failed(e)),
    }
}

fn kernel_stop_cmd(_: &Invocation) -> Result<Report, CommandError> {
    match kernel::stop_kernel() {
        Ok(status) => {
            let mut data = Map::new();
            data.insert(
                "state".to_string(),
                Value::String(status.state.as_str().to_string()),
            );

            Ok(Report::ok("内核已停止。").with_data(data))
        }
        Err(e) => Err(CommandError::Failed(e)),
    }
}

fn identity_status_cmd(_: &Invocation) -> Result<Report, CommandError> {
    let status = identity::check_provision_status();
    let identity = identity::load_node_identity();

    let mut data = Map::new();
    data.insert(
        "status".to_string(),
        Value::String(status.as_str().to_string()),
    );
    data.insert(
        "configured".to_string(),
        Value::Bool(status == identity::ProvisionStatus::Configured),
    );

    if let Some(id) = &identity {
        data.insert("zone_id".to_string(), Value::String(id.zone_id.clone()));
        data.insert("node_name".to_string(), Value::String(id.node_name.clone()));
        data.insert(
            "device_did".to_string(),
            Value::String(id.device_did.clone()),
        );
    }

    let summary = match status {
        identity::ProvisionStatus::Configured => {
            format!(
                "身份已配置。Zone: {}",
                identity.map(|i| i.zone_id).unwrap_or_else(|| "未知".to_string())
            )
        }
        identity::ProvisionStatus::Unactivated => "待激活。请运行 identity activate 完成激活。".to_string(),
        identity::ProvisionStatus::Partial => "身份配置不完整。".to_string(),
        identity::ProvisionStatus::NotInstalled => "内核未安装。".to_string(),
        identity::ProvisionStatus::Invalid => "身份配置无效。".to_string(),
    };

    Ok(Report::ok(summary).with_data(data))
}

fn identity_activate_cmd(invocation: &Invocation) -> Result<Report, CommandError> {
    let status = identity::check_provision_status();
    if status == identity::ProvisionStatus::Configured {
        return Err(CommandError::Failed(
            "系统已激活。如需重新激活，请先清除 $BUCKYOS_ROOT/etc 下的身份文件。".to_string(),
        ));
    }
    if status == identity::ProvisionStatus::NotInstalled {
        return Err(CommandError::Failed("内核未安装。".to_string()));
    }

    let display_name = invocation
        .value("name")
        .ok_or_else(|| CommandError::Usage("需要 --name <显示名称>".to_string()))?;
    let password = invocation
        .value("password")
        .ok_or_else(|| CommandError::Usage("需要 --password <密码>".to_string()))?;

    if password.len() < 8 {
        return Err(CommandError::Failed("密码至少需要8位。".to_string()));
    }

    let zone_name = invocation
        .value("zone")
        .map(|s| s.to_string())
        .unwrap_or_else(|| identity::generate_local_zone_name(display_name));

    let zone_id = format!("did:web:{zone_name}.local");
    let node_name = format!("{zone_name}-ood1");
    let device_did = format!("did:dev:{zone_name}.{node_name}");

    let zone_config = identity::create_zone_config(&zone_name);
    identity::write_zone_config(&zone_config)
        .map_err(|e| CommandError::Failed(format!("无法写入 zone 配置: {e}")))?;

    let node_identity = identity::create_node_identity(&zone_id, &node_name, &device_did);
    identity::write_node_identity(&node_identity)
        .map_err(|e| CommandError::Failed(format!("无法写入节点身份: {e}")))?;

    let profile = product::ensure_product_profile();
    if profile.is_lite() {
        println!("产品模式: Lite（无容器，单Agent）");
    }

    let mut data = Map::new();
    data.insert("zone_id".to_string(), Value::String(zone_id.clone()));
    data.insert("node_name".to_string(), Value::String(node_name.clone()));
    data.insert("device_did".to_string(), Value::String(device_did));
    data.insert("edition".to_string(), Value::String(profile.edition));

    Ok(Report::ok(format!(
        "激活成功。Zone: {zone_id}，节点: {node_name}。请运行 kernel start 启动内核。"
    ))
    .with_data(data))
}

fn product_info_cmd(_: &Invocation) -> Result<Report, CommandError> {
    let profile = product::ensure_product_profile();
    let mut data = Map::new();

    data.insert("edition".to_string(), Value::String(profile.edition.clone()));
    data.insert(
        "container".to_string(),
        Value::Bool(profile.capabilities.container),
    );
    data.insert(
        "user_service_apps".to_string(),
        Value::Bool(profile.capabilities.user_service_apps),
    );
    data.insert(
        "static_web_apps".to_string(),
        Value::Bool(profile.capabilities.static_web_apps),
    );
    data.insert(
        "max_running_agents".to_string(),
        Value::Number(profile.capabilities.max_running_agents.into()),
    );

    let summary = if profile.is_lite() {
        "产品模式: Lite（无容器，单Agent，可安装静态网页应用）"
    } else {
        "产品模式: Full（支持容器，多Agent）"
    };

    Ok(Report::ok(summary).with_data(data))
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
