use crate::command::{describe, dispatch, CommandError, Report, Surface};
use serde_json::{json, Value};
use std::fs;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const MAX_REQUEST: usize = 3 * 1024 * 1024;

pub struct Site {
    pub frontend: PathBuf,
    pub gd5: PathBuf,
    pub entrance: Option<Arc<crate::cloud::Entrance>>,
}

pub fn open_browser(url: &str) -> io::Result<()> {
    let mut command = if cfg!(target_os = "macos") {
        Command::new("open")
    } else if cfg!(target_os = "windows") {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", ""]);
        command
    } else {
        Command::new("xdg-open")
    };
    command.arg(url).spawn()?;
    Ok(())
}

pub fn serve(listener: TcpListener, site: Site) -> io::Result<()> {
    let site = Arc::new(site);
    for connection in listener.incoming() {
        match connection {
            Ok(stream) => {
                let site = Arc::clone(&site);
                thread::spawn(move || {
                    if let Err(error) = handle_client(stream, &site) {
                        eprintln!("连接处理失败: {error}");
                    }
                });
            }
            Err(error) => eprintln!("接受连接失败: {error}"),
        }
    }
    Ok(())
}

fn handle_client(mut stream: TcpStream, site: &Site) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    handle_io(&mut stream, site)
}

fn handle_io(stream: &mut (impl Read + Write), site: &Site) -> io::Result<()> {
    let request = match read_request(stream) {
        Ok(request) => request,
        Err(error) if error.kind() == io::ErrorKind::InvalidData => {
            return write_bytes(
                stream,
                413,
                "Payload Too Large",
                "text/plain; charset=utf-8",
                "请求超过 3MB".as_bytes(),
            );
        }
        Err(error) => return Err(error),
    };
    let Some(request) = request else {
        return Ok(());
    };
    let answer = respond(&request, site);
    write_bytes(
        stream,
        answer.status,
        &answer.reason,
        &answer.content_type,
        &answer.body,
    )
}

struct Incoming {
    method: String,
    path: String,
    host: String,
    authorization: String,
    content_type: String,
    body: Vec<u8>,
}

struct Answer {
    status: u16,
    reason: String,
    content_type: String,
    body: Vec<u8>,
}

fn read_request(stream: &mut impl Read) -> io::Result<Option<Incoming>> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 2048];
    let header_end = loop {
        if buf.len() >= MAX_REQUEST {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "too large"));
        }
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Ok(None);
        }
        let room = MAX_REQUEST - buf.len();
        let take = read.min(room);
        buf.extend_from_slice(&chunk[..take]);
        if let Some(end) = find_subsequence(&buf, b"\r\n\r\n") {
            break end;
        }
        if take < read {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "too large"));
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut lines = head.split("\r\n");
    let Some(start) = lines.next() else {
        return Ok(None);
    };
    let mut parts = start.split_whitespace();
    let Some(method) = parts.next() else {
        return Ok(None);
    };
    let Some(path) = parts.next() else {
        return Ok(None);
    };
    let length = content_length(&head).unwrap_or(0);
    let body_at = header_end + 4;
    if body_at.saturating_add(length) > MAX_REQUEST {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "too large"));
    }
    while buf.len() < body_at + length {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        let room = MAX_REQUEST - buf.len();
        let take = read.min(room);
        buf.extend_from_slice(&chunk[..take]);
        if take < read {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "too large"));
        }
    }
    let body = if buf.len() >= body_at + length {
        buf[body_at..body_at + length].to_vec()
    } else {
        Vec::new()
    };
    Ok(Some(Incoming {
        method: method.to_string(),
        path: path.to_string(),
        host: header_value(&head, "host"),
        authorization: header_value(&head, "authorization"),
        content_type: header_value(&head, "content-type"),
        body,
    }))
}

fn header_value(head: &str, wanted: &str) -> String {
    for line in head.split("\r\n").skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case(wanted) {
            return value.trim().to_string();
        }
    }
    String::new()
}

fn content_length(head: &str) -> Option<usize> {
    for line in head.split("\r\n").skip(1) {
        let (name, value) = line.split_once(':')?;
        if name.eq_ignore_ascii_case("content-length") {
            return value.trim().parse().ok();
        }
    }
    None
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn respond(request: &Incoming, site: &Site) -> Answer {
    if let Some(entrance) = &site.entrance {
        let out = crate::cloud::entrance_request(
            entrance,
            &request.method,
            &request.path,
            &request.host,
            &request.authorization,
            &request.content_type,
            &request.body,
        );
        return Answer {
            status: out.status,
            reason: out.reason,
            content_type: out.content_type,
            body: out.body,
        };
    }
    let path = request
        .path
        .split('?')
        .next()
        .unwrap_or(request.path.as_str());
    let Some(path) = percent_decode(path) else {
        return plain(400, "Bad Request", "路径编码无效。");
    };
    match (request.method.as_str(), path.as_str()) {
        ("GET", "/api/commands") => json_response(200, "OK", describe(crate::command::root())),
        ("POST", "/api/exec") => exec(&request.body),
        ("GET", _) => static_file(&path, site),
        _ => plain(405, "Method Not Allowed", "方法不允许。"),
    }
}

fn exec(body: &[u8]) -> Answer {
    let parsed: Value = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(_) => {
            return json_response(400, "Bad Request", error_body("请求体不是 JSON。", "usage"))
        }
    };
    let Some(items) = parsed.get("argv").and_then(Value::as_array) else {
        return json_response(
            400,
            "Bad Request",
            error_body("argv 必须是字符串数组。", "usage"),
        );
    };
    let mut argv = Vec::with_capacity(items.len());
    for item in items {
        match item.as_str() {
            Some(text) => argv.push(text.to_string()),
            None => {
                return json_response(
                    400,
                    "Bad Request",
                    error_body("argv 必须是字符串数组。", "usage"),
                )
            }
        }
    }
    match dispatch(&argv, Surface::Http) {
        Ok(report) => json_response(200, "OK", report_value(&report)),
        Err(CommandError::Usage(text)) => {
            json_response(400, "Bad Request", error_body(&text, "usage"))
        }
        Err(CommandError::Failed(text)) => {
            json_response(403, "Forbidden", error_body(&text, "failed"))
        }
    }
}

fn report_value(report: &Report) -> Value {
    json!({
        "ok": report.ok,
        "summary": report.summary,
        "data": report.data,
    })
}

fn error_body(summary: &str, error: &str) -> Value {
    json!({
        "ok": false,
        "summary": summary,
        "data": {},
        "error": error,
    })
}

fn static_file(path: &str, site: &Site) -> Answer {
    let (root, rel) = if path == "/" || path == "/index.html" {
        (&site.frontend, "index.html")
    } else if let Some(rest) = path.strip_prefix("/app/") {
        (&site.frontend, rest)
    } else if let Some(rest) = path.strip_prefix("/gd5/") {
        (&site.gd5, rest)
    } else {
        return plain(404, "Not Found", "没有这个路径。");
    };
    let Some(rel) = safe_relative(rel) else {
        return plain(404, "Not Found", "没有这个路径。");
    };
    let Some(file) = resolve_file(root, rel) else {
        return plain(404, "Not Found", "没有这个路径。");
    };
    match fs::read(&file) {
        Ok(bytes) => Answer {
            status: 200,
            reason: "OK".to_string(),
            content_type: mime(&file).to_string(),
            body: bytes,
        },
        Err(_) => plain(404, "Not Found", "没有这个路径。"),
    }
}

fn safe_relative(path: &str) -> Option<&str> {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') || path.contains('\0') {
        return None;
    }
    if path
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return None;
    }
    Some(path)
}

fn resolve_file(root: &Path, rel: &str) -> Option<PathBuf> {
    let root = root.canonicalize().ok()?;
    let file = root.join(rel).canonicalize().ok()?;
    if file.starts_with(&root) && file.is_file() {
        Some(file)
    } else {
        None
    }
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return None;
            }
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json" | "map") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("woff2") => "font/woff2",
        Some("txt" | "md") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn json_response(status: u16, reason: &str, value: Value) -> Answer {
    Answer {
        status,
        reason: reason.to_string(),
        content_type: "application/json; charset=utf-8".to_string(),
        body: serde_json::to_vec(&value).unwrap_or_else(|_| b"{}".to_vec()),
    }
}

fn plain(status: u16, reason: &str, text: &str) -> Answer {
    Answer {
        status,
        reason: reason.to_string(),
        content_type: "text/plain; charset=utf-8".to_string(),
        body: text.as_bytes().to_vec(),
    }
}

fn write_bytes(
    stream: &mut impl Write,
    status: u16,
    reason: &str,
    content_type: &str,
    body: &[u8],
) -> io::Result<()> {
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    fn site() -> Site {
        let frontend = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../frontend");
        Site {
            gd5: frontend.join("vendor/gd5"),
            frontend,
            entrance: None,
        }
    }

    fn start() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || serve(listener, site()));
        addr
    }

    fn exchange(addr: SocketAddr, request: &str) -> String {
        let mut stream = TcpStream::connect(addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut buf = String::new();
        stream.read_to_string(&mut buf).unwrap();
        buf
    }

    fn body_of(response: &str) -> &str {
        response
            .split_once("\r\n\r\n")
            .map(|(_, body)| body)
            .unwrap_or("")
    }

    #[test]
    fn page_and_commands_share_one_tree() {
        let addr = start();
        let commands = exchange(
            addr,
            "GET /api/commands HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        );
        assert!(commands.starts_with("HTTP/1.1 200"));
        let tree: Value = serde_json::from_str(body_of(&commands)).unwrap();
        assert_eq!(tree["commands"][2]["name"], "desk");
        assert_eq!(tree["commands"][2]["commands"][0]["name"], "open");
        assert_eq!(tree["commands"][2]["commands"][0]["surface"], "cli");
        assert_eq!(tree["commands"][3]["name"], "cloud");

        let payload = br#"{"argv":["desk","kernel","status"]}"#;
        let exec = exchange(
            addr,
            &format!(
                "POST /api/exec HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                payload.len(),
                std::str::from_utf8(payload).unwrap()
            ),
        );
        assert!(exec.starts_with("HTTP/1.1 200"));
        let report: Value = serde_json::from_str(body_of(&exec)).unwrap();
        assert_eq!(report["ok"], true);
        assert!(report["data"]["attached"].is_boolean());

        let denied = br#"{"argv":["desk","open"]}"#;
        let blocked = exchange(
            addr,
            &format!(
                "POST /api/exec HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                denied.len(),
                std::str::from_utf8(denied).unwrap()
            ),
        );
        assert!(blocked.starts_with("HTTP/1.1 403"));

        let index = exchange(
            addr,
            "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        );
        assert!(index.starts_with("HTTP/1.1 200"));
        assert!(body_of(&index).contains("BuckyDesk"));
        assert!(index.contains("text/html"));

        let page = exchange(
            addr,
            "GET /app/main.js HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        );
        assert!(page.starts_with("HTTP/1.1 200"));
        assert!(body_of(&page).contains("WorkspacePage"));
        assert!(page.contains("text/javascript"));

        let engine = exchange(
            addr,
            "GET /gd5/src/engine/index.js HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        );
        assert!(engine.starts_with("HTTP/1.1 200"));
        assert!(body_of(&engine).contains("Component"));

        let escaped = exchange(
            addr,
            "GET /gd5/%2e%2e/%2e%2e/backend/Cargo.toml HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        );
        assert!(escaped.starts_with("HTTP/1.1 404"));
    }

    #[test]
    fn entrance_relays_one_desktop() {
        let dir = std::env::temp_dir().join(format!(
            "buckydesk-entrance-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        crate::desk::isolate(&dir, "127.0.0.1:1", || {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let entrance = crate::cloud::open_entrance().unwrap();
            let hosted = Site {
                frontend: PathBuf::new(),
                gd5: PathBuf::new(),
                entrance: Some(Arc::clone(&entrance)),
            };
            thread::spawn(move || serve(listener, hosted));

            let saved = dispatch(
                &split(&format!(
                    "desk entrance use --at http://{addr} --name home.example"
                )),
                Surface::Cli,
            )
            .unwrap();
            assert!(saved.ok);
            assert!(saved.summary.contains("还要从这台桌面向外连。"));

            let desk = TcpListener::bind("127.0.0.1:0").unwrap();
            let desk_addr = desk.local_addr().unwrap();
            thread::spawn(move || {
                let (mut stream, _) = desk.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut buf = Vec::new();
                let mut chunk = [0u8; 1024];
                loop {
                    match stream.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(read) => {
                            buf.extend_from_slice(&chunk[..read]);
                            if buf.windows(4).any(|window| window == b"\r\n\r\n") {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                let body = "你好".as_bytes();
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(header.as_bytes()).unwrap();
                stream.write_all(body).unwrap();
            });

            let desk_text = desk_addr.to_string();
            let polled = thread::spawn(move || crate::cloud::poll_once(&desk_text));
            let registry_path = dir.join("desktops.json");
            let started = std::time::Instant::now();
            while !registry_path.exists() && started.elapsed() < Duration::from_secs(5) {
                thread::sleep(Duration::from_millis(20));
            }
            let page = exchange(
                addr,
                "GET /hello HTTP/1.1\r\nHost: home.example\r\nConnection: close\r\n\r\n",
            );
            let handled = polled.join().unwrap().unwrap();
            assert!(handled);
            assert!(page.starts_with("HTTP/1.1 200"), "{page}");
            assert_eq!(body_of(&page), "你好");

            let listed = dispatch(&split("cloud status"), Surface::Http).unwrap();
            assert_eq!(listed.summary, "1 台桌面接到了这个入口。1 台正连着。");
            let registry = std::fs::read_to_string(dir.join("desktops.json")).unwrap();
            let link = std::fs::read_to_string(dir.join("link.json")).unwrap();
            assert!(registry.contains("tokenHash"));
            assert!(!registry.contains("\"token\""));
            let token = serde_json::from_str::<Value>(&link).unwrap()["token"]
                .as_str()
                .unwrap()
                .to_string();
            assert!(!token.is_empty());
            assert!(!registry.contains(&token));

            let again = br#"{"name":"home.example"}"#;
            let taken = exchange(
                addr,
                &format!(
                    "POST /api/entrance/register HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    again.len(),
                    std::str::from_utf8(again).unwrap()
                ),
            );
            assert!(taken.starts_with("HTTP/1.1 409"), "{taken}");
            assert!(body_of(&taken).contains("这台桌面已经接到入口。"));

            crate::cloud::forget(&entrance, "home.example");
            let quiet = exchange(
                addr,
                "GET / HTTP/1.1\r\nHost: home.example\r\nConnection: close\r\n\r\n",
            );
            assert!(quiet.starts_with("HTTP/1.1 503"), "{quiet}");
            assert!(body_of(&quiet).contains("这台桌面还没有连上。"));

            let other = exchange(
                addr,
                "GET /nope HTTP/1.1\r\nHost: other.example\r\nConnection: close\r\n\r\n",
            );
            assert!(other.starts_with("HTTP/1.1 421"), "{other}");
            assert!(body_of(&other).contains("这个地址不是接到入口的桌面。"));

            let door = exchange(
                addr,
                "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
            );
            assert!(door.starts_with("HTTP/1.1 200"), "{door}");
            assert!(body_of(&door).contains("这是云入口。从桌面的名字打开。"));
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn split(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_string).collect()
    }
}
