use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Where a command may run. The page cannot start or stop the process that hosts it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Cli,
    Http,
    Both,
}

impl Surface {
    pub fn as_str(self) -> &'static str {
        match self {
            Surface::Cli => "cli",
            Surface::Http => "http",
            Surface::Both => "both",
        }
    }

    fn allows(self, from: Surface) -> bool {
        matches!(
            (self, from),
            (Surface::Both, _) | (Surface::Cli, Surface::Cli) | (Surface::Http, Surface::Http)
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Arg {
    pub name: &'static str,
    pub takes_value: bool,
    pub summary: &'static str,
}

#[derive(Clone, Copy)]
pub struct Command {
    pub name: &'static str,
    pub summary: &'static str,
    pub surface: Surface,
    pub args: &'static [Arg],
    pub children: &'static [Command],
    pub run: Option<fn(&Invocation) -> Result<Report, CommandError>>,
}

pub struct Invocation {
    pub values: BTreeMap<String, String>,
    pub switches: Vec<String>,
}

impl Invocation {
    pub fn flag(&self, name: &str) -> bool {
        self.switches.iter().any(|switch| switch == name)
    }

    pub fn value(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub ok: bool,
    pub summary: String,
    pub data: Map<String, Value>,
}

impl Report {
    pub fn ok(summary: impl Into<String>) -> Self {
        Self {
            ok: true,
            summary: summary.into(),
            data: Map::new(),
        }
    }

    pub fn fail(summary: impl Into<String>) -> Self {
        Self {
            ok: false,
            summary: summary.into(),
            data: Map::new(),
        }
    }

    pub fn with_data(mut self, data: Map<String, Value>) -> Self {
        self.data = data;
        self
    }
}

#[derive(Debug)]
pub enum CommandError {
    Usage(String),
    Failed(String),
}

pub fn root() -> &'static Command {
    crate::catalog::root()
}

pub fn dispatch(argv: &[String], from: Surface) -> Result<Report, CommandError> {
    if argv.is_empty() || is_help(&argv[0]) {
        return Ok(Report::ok(help(root(), &[])));
    }

    let mut node = root();
    let mut path = Vec::new();
    let mut index = 0;
    while index < argv.len() && !argv[index].starts_with('-') {
        let name = &argv[index];
        let Some(child) = node.children.iter().find(|child| child.name == name) else {
            return Err(CommandError::Usage(format!(
                "未知命令 {}。\n\n{}",
                display_path(&path, name),
                help(node, &path)
            )));
        };
        node = child;
        path.push(name.as_str());
        index += 1;
        if node.run.is_some()
            && (index >= argv.len()
                || argv[index].starts_with('-')
                || !node.children.iter().any(|child| child.name == argv[index]))
        {
            break;
        }
    }

    if argv[index..].iter().any(|token| is_help(token)) {
        return Ok(Report::ok(help(node, &path)));
    }

    let Some(run) = node.run else {
        return Ok(Report::ok(help(node, &path)));
    };

    if !node.surface.allows(from) {
        return Err(CommandError::Failed(format!(
            "`{}` 只能在命令行运行。",
            display_path(&[], path.join(" ").as_str())
        )));
    }

    let invocation = parse_args(node, &path, &argv[index..])?;
    run(&invocation)
}

pub fn describe(command: &Command) -> Value {
    describe_at(command, &[])
}

fn describe_at(command: &Command, parent: &[&str]) -> Value {
    let path: Vec<&str> = if parent.is_empty() && command.name == "buckydesk" {
        Vec::new()
    } else {
        let mut path = parent.to_vec();
        path.push(command.name);
        path
    };
    let commands: Vec<Value> = command
        .children
        .iter()
        .map(|child| describe_at(child, &path))
        .collect();
    serde_json::json!({
        "name": command.name,
        "path": path,
        "summary": command.summary,
        "surface": command.surface.as_str(),
        "args": command
            .args
            .iter()
            .map(|arg| serde_json::json!({
                "name": arg.name,
                "takesValue": arg.takes_value,
                "summary": arg.summary,
            }))
            .collect::<Vec<_>>(),
        "commands": commands,
    })
}

fn is_help(token: &str) -> bool {
    matches!(token, "help" | "--help" | "-h")
}

fn parse_args(
    command: &Command,
    path: &[&str],
    argv: &[String],
) -> Result<Invocation, CommandError> {
    let mut values = BTreeMap::new();
    let mut switches = Vec::new();
    let mut index = 0;
    while index < argv.len() {
        let token = &argv[index];
        if token == "--" {
            return Err(CommandError::Usage("命令壳不接受位置参数。".to_string()));
        }
        if !token.starts_with("--") {
            return Err(CommandError::Usage(format!(
                "多余的参数 {token}。\n\n{}",
                help(command, path)
            )));
        }
        let (name, inline) = match token[2..].split_once('=') {
            Some((name, value)) => (name, Some(value.to_string())),
            None => (&token[2..], None),
        };
        if name.is_empty() {
            return Err(CommandError::Usage("空的选项名。".to_string()));
        }
        let Some(arg) = command.args.iter().find(|arg| arg.name == name) else {
            return Err(CommandError::Usage(format!(
                "未知选项 --{name}。\n\n{}",
                help(command, path)
            )));
        };
        if arg.takes_value {
            let value = if let Some(value) = inline {
                value
            } else {
                index += 1;
                argv.get(index)
                    .cloned()
                    .ok_or_else(|| CommandError::Usage(format!("--{name} 需要一个值。")))?
            };
            values.insert(name.to_string(), value);
        } else if inline.is_some() {
            return Err(CommandError::Usage(format!("--{name} 不接受值。")));
        } else {
            switches.push(name.to_string());
        }
        index += 1;
    }
    Ok(Invocation { values, switches })
}

pub fn help(command: &Command, path: &[&str]) -> String {
    let title = if path.is_empty() {
        "buckydesk".to_string()
    } else {
        format!("buckydesk {}", path.join(" "))
    };
    let mut lines = vec![format!("{title} — {}", command.summary), String::new()];
    if command.children.is_empty() {
        lines.push(format!("用法: {title} [选项]"));
    } else {
        lines.push(format!("用法: {title} <命令> [选项]"));
        lines.push(String::new());
        lines.push("命令:".to_string());
        for child in command.children {
            lines.push(format!("  {:<16} {}", child.name, child.summary));
        }
    }
    if !command.args.is_empty() {
        lines.push(String::new());
        lines.push("选项:".to_string());
        for arg in command.args {
            let form = if arg.takes_value {
                format!("--{} <值>", arg.name)
            } else {
                format!("--{}", arg.name)
            };
            lines.push(format!("  {form:<16} {}", arg.summary));
        }
    }
    lines.join("\n")
}

fn display_path(path: &[&str], name: &str) -> String {
    let mut parts = vec!["buckydesk"];
    parts.extend(path.iter().copied());
    if !name.is_empty() {
        parts.push(name);
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn root_help_lists_every_group() {
        let report = dispatch(&[], Surface::Cli).unwrap();
        assert!(report.ok);
        assert!(report.summary.contains("server"));
        assert!(report.summary.contains("kernel"));
        assert!(report.summary.contains("identity"));
    }

    #[test]
    fn kernel_services_reports_whether_the_gateway_is_listening() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let dir = std::env::temp_dir().join(format!("buckydesk-svc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        crate::desk::isolate(&dir, &address.to_string(), || {
            let report = dispatch(&argv("kernel services"), Surface::Http).unwrap();
            assert!(report.ok);
            assert_eq!(report.summary, "网关已经在这台电脑上运行。");
            assert_eq!(
                report.data.get("attached").and_then(Value::as_bool),
                Some(true)
            );
            let services = report
                .data
                .get("services")
                .and_then(Value::as_array)
                .unwrap();
            assert_eq!(services.len(), 8);
            let gateway = services
                .iter()
                .find(|item| item.get("id").and_then(Value::as_str) == Some("gateway"))
                .unwrap();
            assert_eq!(
                gateway.get("listening").and_then(Value::as_bool),
                Some(true)
            );
            assert_eq!(gateway.get("port").and_then(Value::as_u64), Some(3180));
            let node = services
                .iter()
                .find(|item| item.get("id").and_then(Value::as_str) == Some("node"))
                .unwrap();
            assert_eq!(node.get("port").and_then(Value::as_u64), Some(3181));
        });
        drop(listener);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn kernel_status_probes_the_configured_port() {
        let dir = std::env::temp_dir().join(format!("buckydesk-cmd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        crate::desk::isolate(&dir, "127.0.0.1:1", || {
            let report = dispatch(&argv("kernel status"), Surface::Http).unwrap();
            assert!(report.ok);
            assert_eq!(
                report.data.get("attached").and_then(Value::as_bool),
                Some(false)
            );
            assert_eq!(report.summary, "内核还没有在这台电脑上运行。");
        });
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn identity_activate_is_refused_twice() {
        let dir = std::env::temp_dir().join(format!(
            "buckydesk-id-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        crate::desk::isolate(&dir, "127.0.0.1:1", || {
            let activate = [
                "identity",
                "activate",
                "--name",
                "alice",
                "--password",
                "12345678",
                "--key-folder",
                "keys",
            ]
            .map(str::to_string)
            .to_vec();
            let first = dispatch(&activate, Surface::Http).unwrap();
            assert!(first.ok);
            assert!(first.data.get("passwordHash").is_none());
            let second = dispatch(&activate, Surface::Http).unwrap();
            assert!(!second.ok);
            assert_eq!(second.summary, "上次没有完成。");
            let started = dispatch(&argv("kernel start"), Surface::Cli).unwrap();
            assert!(!started.ok);
            assert_eq!(started.summary, "这台电脑上还没有可拉起的内核。");
        });
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn server_is_cli_only() {
        let error = dispatch(&argv("server"), Surface::Http).unwrap_err();
        assert!(matches!(error, CommandError::Failed(_)));
    }

    #[test]
    fn unknown_command_is_usage() {
        let error = dispatch(&argv("kernel reboot"), Surface::Cli).unwrap_err();
        assert!(matches!(error, CommandError::Usage(_)));
    }

    #[test]
    fn server_rejects_unknown_flag() {
        let error = dispatch(&argv("server --port 1016"), Surface::Cli).unwrap_err();
        match error {
            CommandError::Usage(text) => assert!(text.contains("buckydesk server")),
            CommandError::Failed(_) => panic!("unknown flag is usage, not failure"),
        }
    }

    #[test]
    fn described_tree_matches_the_shell() {
        let tree = describe(root());
        let names: Vec<&str> = tree["commands"]
            .as_array()
            .unwrap()
            .iter()
            .map(|node| node["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            vec![
                "version",
                "commands",
                "server",
                "kernel",
                "identity",
                "workspace"
            ]
        );
        let server = &tree["commands"][2];
        assert_eq!(server["surface"], "cli");
        assert_eq!(server["path"], serde_json::json!(["server"]));
        let status = &tree["commands"][3]["commands"][0];
        assert_eq!(status["path"], serde_json::json!(["kernel", "status"]));
        assert_eq!(status["surface"], "both");
        let identity: Vec<&str> = tree["commands"][4]["commands"]
            .as_array()
            .unwrap()
            .iter()
            .map(|node| node["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            identity,
            vec!["status", "activate", "finish", "login", "password", "reset"]
        );
    }
}
