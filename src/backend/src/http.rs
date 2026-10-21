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
    pub domain: Option<String>,
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

pub fn serve_tls(
    listener: TcpListener,
    site: Site,
    config: Arc<rustls::ServerConfig>,
) -> io::Result<()> {
    let site = Arc::new(site);
    for connection in listener.incoming() {
        match connection {
            Ok(stream) => {
                let site = Arc::clone(&site);
                let config = Arc::clone(&config);
                thread::spawn(move || {
                    if let Err(error) = handle_tls(stream, &site, config) {
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

fn handle_tls(
    mut stream: TcpStream,
    site: &Site,
    config: Arc<rustls::ServerConfig>,
) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    let mut connection = rustls::ServerConnection::new(config)
        .map_err(|error| io::Error::new(io::ErrorKind::Other, error.to_string()))?;
    let mut tls = rustls::Stream::new(&mut connection, &mut stream);
    handle_io(&mut tls, site)
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
    let (status, reason, content_type, body) = respond(&request, site);
    write_bytes(stream, status, reason, content_type, &body)
}

struct Incoming {
    method: String,
    path: String,
    host: String,
    authorization: String,
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

fn respond(request: &Incoming, site: &Site) -> (u16, &'static str, &'static str, Vec<u8>) {
    let path = request
        .path
        .split('?')
        .next()
        .unwrap_or(request.path.as_str());
    let Some(path) = percent_decode(path) else {
        return plain(400, "Bad Request", "路径编码无效。");
    };
    if let Some(domain) = &site.domain {
        if !host_allowed(&request.host, domain) {
            return plain(421, "Misdirected Request", "这个地址不是这台电脑的域名。");
        }
    }
    match (request.method.as_str(), path.as_str()) {
        ("GET", "/api/commands") => json_response(200, "OK", describe(crate::command::root())),
        ("POST", "/api/exec") => exec(&request.body, site.domain.is_some(), &request.authorization),
        ("GET", _) => static_file(&path, site),
        _ => plain(405, "Method Not Allowed", "方法不允许。"),
    }
}

pub fn host_allowed(host: &str, domain: &str) -> bool {
    let host = host
        .split(':')
        .next()
        .unwrap_or(host)
        .trim()
        .trim_end_matches('.')
        .to_ascii_lowercase();
    host == domain || host == format!("ood1.{domain}") || host == "localhost" || host == "127.0.0.1"
}

fn exec(
    body: &[u8],
    require_session: bool,
    authorization: &str,
) -> (u16, &'static str, &'static str, Vec<u8>) {
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
    if require_session && !guest_command(&argv) && !crate::desk::session_allows(authorization) {
        return json_response(401, "Unauthorized", error_body("请重新进入。", "failed"));
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

fn guest_command(argv: &[String]) -> bool {
    let names: Vec<&str> = argv.iter().map(String::as_str).collect();
    matches!(
        names.as_slice(),
        ["desk", "identity", "status"] | ["desk", "identity", "login", ..]
    )
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

fn static_file(path: &str, site: &Site) -> (u16, &'static str, &'static str, Vec<u8>) {
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
        Ok(bytes) => (200, "OK", mime(&file), bytes),
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

fn json_response(
    status: u16,
    reason: &'static str,
    value: Value,
) -> (u16, &'static str, &'static str, Vec<u8>) {
    let body = serde_json::to_vec(&value).unwrap_or_else(|_| b"{}".to_vec());
    (status, reason, "application/json; charset=utf-8", body)
}

fn plain(
    status: u16,
    reason: &'static str,
    text: &str,
) -> (u16, &'static str, &'static str, Vec<u8>) {
    (
        status,
        reason,
        "text/plain; charset=utf-8",
        text.as_bytes().to_vec(),
    )
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
            domain: None,
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

    fn start_named(domain: &str) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let mut hosted = site();
        hosted.domain = Some(domain.to_string());
        thread::spawn(move || serve(listener, hosted));
        addr
    }

    #[test]
    fn public_listener_checks_the_host_and_the_session() {
        let addr = start_named("example.invalid");
        let page = exchange(
            addr,
            "GET / HTTP/1.1\r\nHost: example.invalid\r\nConnection: close\r\n\r\n",
        );
        assert!(page.starts_with("HTTP/1.1 200"));
        let device = exchange(
            addr,
            "GET / HTTP/1.1\r\nHost: ood1.example.invalid\r\nConnection: close\r\n\r\n",
        );
        assert!(device.starts_with("HTTP/1.1 200"));
        let wrong = exchange(
            addr,
            "GET / HTTP/1.1\r\nHost: other.example\r\nConnection: close\r\n\r\n",
        );
        assert!(wrong.starts_with("HTTP/1.1 421"));
        assert!(body_of(&wrong).contains("这个地址不是这台电脑的域名。"));

        let payload = br#"{"argv":["desk","kernel","status"]}"#;
        let denied = exchange(
            addr,
            &format!(
                "POST /api/exec HTTP/1.1\r\nHost: example.invalid\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                payload.len(),
                std::str::from_utf8(payload).unwrap()
            ),
        );
        assert!(denied.starts_with("HTTP/1.1 401"));
        assert!(body_of(&denied).contains("请重新进入。"));

        let login = br#"{"argv":["desk","identity","login","--password","nope-nope"]}"#;
        let guest = exchange(
            addr,
            &format!(
                "POST /api/exec HTTP/1.1\r\nHost: example.invalid\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                login.len(),
                std::str::from_utf8(login).unwrap()
            ),
        );
        assert!(guest.starts_with("HTTP/1.1 200"));
    }

    #[test]
    fn self_signed_certificate_serves_the_same_page() {
        let dir = std::env::temp_dir().join(format!("buckydesk-tls-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let addr = crate::desk::isolate(&dir, "127.0.0.1:1", || {
            let identity = crate::cloud::issue_certificate("example.invalid").unwrap();
            assert!(identity.cert_pem.starts_with("-----BEGIN CERTIFICATE-----"));
            assert_eq!(identity.kind, "self");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(dir.join("key.pem"))
                    .unwrap()
                    .permissions()
                    .mode();
                assert_eq!(mode & 0o777, 0o600);
            }
            let config = crate::cloud::tls_server_config(&identity).unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let mut hosted = site();
            hosted.domain = Some("example.invalid".into());
            thread::spawn(move || serve_tls(listener, hosted, config));
            addr
        });

        let _ = rustls::crypto::ring::default_provider().install_default();
        let config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AcceptAny))
            .with_no_client_auth();
        let name = rustls::pki_types::ServerName::try_from("example.invalid").unwrap();
        let mut connection = rustls::ClientConnection::new(Arc::new(config), name).unwrap();
        let mut tcp = TcpStream::connect(addr).unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        tcp.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut tls = rustls::Stream::new(&mut connection, &mut tcp);
        tls.write_all(b"GET / HTTP/1.1\r\nHost: example.invalid\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut bytes = Vec::new();
        match tls.read_to_end(&mut bytes) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof && !bytes.is_empty() => {}
            Err(error) => panic!("{error}"),
        }
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("HTTP/1.1 200"), "{text}");
        assert!(text.contains("BuckyDesk"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[derive(Debug)]
    struct AcceptAny;

    impl rustls::client::danger::ServerCertVerifier for AcceptAny {
        fn verify_server_cert(
            &self,
            _: &rustls::pki_types::CertificateDer<'_>,
            _: &[rustls::pki_types::CertificateDer<'_>],
            _: &rustls::pki_types::ServerName<'_>,
            _: &[u8],
            _: rustls::pki_types::UnixTime,
        ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            message: &[u8],
            cert: &rustls::pki_types::CertificateDer<'_>,
            dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            rustls::crypto::verify_tls12_signature(
                message,
                cert,
                dss,
                &rustls::crypto::ring::default_provider().signature_verification_algorithms,
            )
        }

        fn verify_tls13_signature(
            &self,
            message: &[u8],
            cert: &rustls::pki_types::CertificateDer<'_>,
            dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            rustls::crypto::verify_tls13_signature(
                message,
                cert,
                dss,
                &rustls::crypto::ring::default_provider().signature_verification_algorithms,
            )
        }

        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            rustls::crypto::ring::default_provider()
                .signature_verification_algorithms
                .supported_schemes()
        }
    }
}
