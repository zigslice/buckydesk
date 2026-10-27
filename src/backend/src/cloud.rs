use crate::command::Report;
use crate::desk::{restrict_to_owner, state_dir};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::{self, Read, Write};
use std::net::{IpAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const POLL_WAIT: Duration = Duration::from_secs(12);
const BROWSER_WAIT: Duration = Duration::from_secs(20);
const CONNECTED_MS: u64 = 25_000;
const QUEUE_CAP: usize = 8;
const FLUSH_MS: u64 = 1_000;

/// In-memory queue for one `cloud serve` process.
/// HTTP handlers take this gate and never `desk::with_desk`.
pub struct Entrance {
    dir: PathBuf,
    inner: Mutex<Memory>,
    cv: Condvar,
}

struct Memory {
    desktops: Vec<DeskLive>,
    next_id: u64,
    inflight: HashMap<String, Sender<Reply>>,
}

struct DeskLive {
    name: String,
    token_hash: String,
    seen_ms: u64,
    last_flush_ms: u64,
    queue: VecDeque<Queued>,
}

struct Queued {
    id: String,
    method: String,
    path: String,
    host: String,
    authorization: String,
    content_type: String,
    body: Vec<u8>,
    reply: Sender<Reply>,
}

struct Reply {
    status: u16,
    content_type: String,
    body: Vec<u8>,
}

struct Job {
    id: String,
    method: String,
    path: String,
    host: String,
    authorization: String,
    content_type: String,
    body: Vec<u8>,
}

#[derive(Clone, Serialize, Deserialize)]
struct LinkFile {
    #[serde(default)]
    at: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    token: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct DesktopFile {
    name: String,
    #[serde(rename = "tokenHash")]
    token_hash: String,
    #[serde(rename = "seenMs", default)]
    seen_ms: u64,
}

#[derive(Serialize, Deserialize)]
struct DesktopList {
    desktops: Vec<DesktopFile>,
}

pub struct Outgoing {
    pub status: u16,
    pub reason: String,
    pub content_type: String,
    pub body: Vec<u8>,
}

impl Outgoing {
    fn text(status: u16, reason: &str, body: &str) -> Self {
        Self {
            status,
            reason: reason.to_string(),
            content_type: "text/plain; charset=utf-8".to_string(),
            body: body.as_bytes().to_vec(),
        }
    }

    fn json(status: u16, reason: &str, value: Value) -> Self {
        Self {
            status,
            reason: reason.to_string(),
            content_type: "application/json; charset=utf-8".to_string(),
            body: serde_json::to_vec(&value).unwrap_or_else(|_| b"{}".to_vec()),
        }
    }
}

pub fn wipe() -> Result<(), ()> {
    let dir = state_dir();
    for name in [
        "link.json",
        "desktops.json",
        "cloud.json",
        "cert.pem",
        "key.pem",
    ] {
        remove_file(&dir.join(name))?;
    }
    Ok(())
}

fn remove_file(path: &Path) -> Result<(), ()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(()),
    }
}

pub fn link_status() -> Report {
    match load_link() {
        Ok(link) => link_report(&link, false),
        Err(report) => report,
    }
}

pub fn use_link(at: Option<&str>, name: Option<&str>) -> Report {
    if at.is_none() && name.is_none() {
        return link_status();
    }
    let mut link = match load_link() {
        Ok(link) => link,
        Err(report) => return report,
    };
    let new_at = if let Some(at) = at {
        match normalize_at(at) {
            Ok(value) => value,
            Err(text) => return Report::fail(text),
        }
    } else {
        link.at.clone()
    };
    let new_name = if let Some(name) = name {
        match normalize_name(name) {
            Ok(value) => value,
            Err(text) => return Report::fail(text),
        }
    } else {
        link.name.clone()
    };
    if new_at != link.at || new_name != link.name {
        link.token.clear();
    }
    link.at = new_at;
    link.name = new_name;
    if let Err(report) = store_link(&link) {
        return report;
    }
    link_report(&link, true)
}

pub fn host_status() -> Report {
    let desktops = match read_desktops(&state_dir()) {
        Ok(desktops) => desktops,
        Err(()) => return Report::fail("工作区暂时打不开。"),
    };
    let now = now_ms();
    let connected = desktops
        .iter()
        .filter(|desk| is_connected(desk.seen_ms, now))
        .count();
    let summary = if desktops.is_empty() {
        "还没有桌面接到这个入口。".to_string()
    } else {
        format!(
            "{} 台桌面接到了这个入口。{connected} 台正连着。",
            desktops.len()
        )
    };
    let mut data = Map::new();
    data.insert(
        "desktops".into(),
        Value::Array(
            desktops
                .into_iter()
                .map(|desk| {
                    let mut item = Map::new();
                    item.insert("name".into(), Value::String(desk.name));
                    item.insert(
                        "connected".into(),
                        Value::Bool(is_connected(desk.seen_ms, now)),
                    );
                    Value::Object(item)
                })
                .collect(),
        ),
    );
    Report::ok(summary).with_data(data)
}

pub fn open_entrance() -> Result<Arc<Entrance>, Report> {
    let dir = state_dir();
    let desktops = read_desktops(&dir).map_err(|()| Report::fail("工作区暂时打不开。"))?;
    let desktops = desktops
        .into_iter()
        .map(|desk| DeskLive {
            name: desk.name,
            token_hash: desk.token_hash,
            seen_ms: desk.seen_ms,
            last_flush_ms: desk.seen_ms,
            queue: VecDeque::new(),
        })
        .collect();
    Ok(Arc::new(Entrance {
        dir,
        inner: Mutex::new(Memory {
            desktops,
            next_id: 1,
            inflight: HashMap::new(),
        }),
        cv: Condvar::new(),
    }))
}

pub fn run_link(desk_addr: &str) -> Report {
    loop {
        match load_link() {
            Ok(link) if link.at.is_empty() || link.name.is_empty() => {
                return Report::fail(link_summary(&link));
            }
            Ok(link) => {
                if let Err(message) = poll_once_with(&link, desk_addr) {
                    eprintln!("{message}");
                    thread::sleep(Duration::from_secs(2));
                }
            }
            Err(report) => {
                eprintln!("{}", report.summary);
                thread::sleep(Duration::from_secs(2));
            }
        }
    }
}

#[cfg(test)]
pub fn poll_once(desk_addr: &str) -> Result<bool, String> {
    let link = load_link().map_err(|report| report.summary)?;
    if link.at.is_empty() || link.name.is_empty() {
        return Err(link_summary(&link));
    }
    poll_once_with(&link, desk_addr)
}

fn poll_once_with(link: &LinkFile, desk_addr: &str) -> Result<bool, String> {
    let mut link = link.clone();
    if link.token.is_empty() {
        link.token = register_remote(&link)?;
        store_link(&link).map_err(|report| report.summary)?;
    }
    let Some(job) = poll_remote(&link)? else {
        return Ok(false);
    };
    let reply = deliver(desk_addr, &job);
    reply_remote(&link, &job.id, &reply)?;
    Ok(true)
}

pub fn entrance_request(
    entrance: &Entrance,
    method: &str,
    path: &str,
    host: &str,
    authorization: &str,
    content_type: &str,
    body: &[u8],
) -> Outgoing {
    let route = path.split('?').next().unwrap_or(path);
    match (method, route) {
        ("POST", "/api/entrance/register") => register(entrance, authorization, body),
        ("POST", "/api/entrance/poll") => poll(entrance, authorization, body),
        ("POST", "/api/entrance/reply") => reply(entrance, authorization, body),
        _ => forward(
            entrance,
            method,
            path,
            host,
            authorization,
            content_type,
            body,
        ),
    }
}

fn register(entrance: &Entrance, authorization: &str, body: &[u8]) -> Outgoing {
    let value = match json_body(body) {
        Ok(value) => value,
        Err(outgoing) => return outgoing,
    };
    let name = match required_name(&value) {
        Ok(name) => name,
        Err(outgoing) => return outgoing,
    };
    let mut guard = lock(entrance);
    if let Some(index) = find_mut(&guard, &name) {
        if bearer_matches(&guard.desktops[index], authorization) {
            touch(&mut guard, index, &entrance.dir);
            return Outgoing::json(200, "OK", json!({"ok": true}));
        }
        return Outgoing::json(
            409,
            "Conflict",
            json!({"ok": false, "summary": "这台桌面已经接到入口。"}),
        );
    }
    let Ok(token) = random_token() else {
        return Outgoing::json(
            500,
            "Internal Server Error",
            json!({"ok": false, "summary": "工作区暂时打不开。"}),
        );
    };
    let now = now_ms();
    guard.desktops.push(DeskLive {
        name,
        token_hash: token_hash(&token),
        seen_ms: now,
        last_flush_ms: now,
        queue: VecDeque::new(),
    });
    if write_desktops(&entrance.dir, &guard.desktops).is_err() {
        guard.desktops.pop();
        return Outgoing::json(
            500,
            "Internal Server Error",
            json!({"ok": false, "summary": "工作区暂时打不开。"}),
        );
    }
    Outgoing::json(200, "OK", json!({"ok": true, "token": token}))
}

fn poll(entrance: &Entrance, authorization: &str, body: &[u8]) -> Outgoing {
    let value = match json_body(body) {
        Ok(value) => value,
        Err(outgoing) => return outgoing,
    };
    let name = match required_name(&value) {
        Ok(name) => name,
        Err(outgoing) => return outgoing,
    };
    let mut guard = lock(entrance);
    let Some(index) = authed_index(&guard, &name, authorization) else {
        return unknown();
    };
    touch(&mut guard, index, &entrance.dir);
    let deadline = Instant::now() + POLL_WAIT;
    loop {
        if let Some(job) = guard.desktops[index].queue.pop_front() {
            let value = job_value(&job);
            guard.inflight.insert(job.id.clone(), job.reply);
            return Outgoing::json(200, "OK", value);
        }
        let now = Instant::now();
        if now >= deadline {
            return Outgoing::json(200, "OK", json!({"ok": true}));
        }
        let wait = entrance
            .cv
            .wait_timeout(guard, deadline.saturating_duration_since(now));
        let (next, result) = match wait {
            Ok(pair) => pair,
            Err(poison) => poison.into_inner(),
        };
        guard = next;
        if result.timed_out() {
            if let Some(job) = guard.desktops[index].queue.pop_front() {
                let value = job_value(&job);
                guard.inflight.insert(job.id.clone(), job.reply);
                return Outgoing::json(200, "OK", value);
            }
            return Outgoing::json(200, "OK", json!({"ok": true}));
        }
    }
}

fn reply(entrance: &Entrance, authorization: &str, body: &[u8]) -> Outgoing {
    let value = match json_body(body) {
        Ok(value) => value,
        Err(outgoing) => return outgoing,
    };
    let name = match required_name(&value) {
        Ok(name) => name,
        Err(outgoing) => return outgoing,
    };
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let status = value.get("status").and_then(Value::as_u64).unwrap_or(502) as u16;
    let content_type = value
        .get("contentType")
        .and_then(Value::as_str)
        .unwrap_or("application/octet-stream")
        .to_string();
    let payload = match value.get("body").and_then(Value::as_str).unwrap_or("") {
        "" => Vec::new(),
        text => match base64::engine::general_purpose::STANDARD.decode(text) {
            Ok(bytes) => bytes,
            Err(_) => {
                return Outgoing::json(
                    400,
                    "Bad Request",
                    json!({"ok": false, "summary": "应答读不出来。"}),
                )
            }
        },
    };
    let mut guard = lock(entrance);
    let Some(index) = authed_index(&guard, &name, authorization) else {
        return unknown();
    };
    touch(&mut guard, index, &entrance.dir);
    if let Some(sender) = guard.inflight.remove(&id) {
        let _ = sender.send(Reply {
            status,
            content_type,
            body: payload,
        });
    }
    Outgoing::json(200, "OK", json!({"ok": true}))
}

fn forward(
    entrance: &Entrance,
    method: &str,
    path: &str,
    host: &str,
    authorization: &str,
    content_type: &str,
    body: &[u8],
) -> Outgoing {
    if !method_ok(method)
        || !path_ok(path)
        || field_breaks(host)
        || field_breaks(authorization)
        || field_breaks(content_type)
    {
        return Outgoing::text(400, "Bad Request", "请求读不出来。");
    }
    let route = path.split('?').next().unwrap_or(path);
    let (id, rx) = {
        let mut guard = lock(entrance);
        let Some(index) = guard
            .desktops
            .iter()
            .position(|desk| names_match(&desk.name, host))
        else {
            if method == "GET" && (route == "/" || route == "/index.html") {
                return Outgoing::text(200, "OK", "这是云入口。从桌面的名字打开。");
            }
            return Outgoing::text(421, "Misdirected Request", "这个地址不是接到入口的桌面。");
        };
        if !is_connected(guard.desktops[index].seen_ms, now_ms()) {
            return Outgoing::text(503, "Service Unavailable", "这台桌面还没有连上。");
        }
        if guard.desktops[index].queue.len() >= QUEUE_CAP {
            return Outgoing::text(503, "Service Unavailable", "这台桌面忙。");
        }
        let id = guard.next_id.to_string();
        guard.next_id = guard.next_id.saturating_add(1);
        let (tx, rx) = mpsc::channel();
        guard.desktops[index].queue.push_back(Queued {
            id: id.clone(),
            method: method.to_string(),
            path: path.to_string(),
            host: host.to_string(),
            authorization: authorization.to_string(),
            content_type: content_type.to_string(),
            body: body.to_vec(),
            reply: tx,
        });
        entrance.cv.notify_all();
        (id, rx)
    };
    match rx.recv_timeout(BROWSER_WAIT) {
        Ok(reply) => outgoing_from_reply(reply),
        Err(_) => {
            let mut guard = lock(entrance);
            for desk in &mut guard.desktops {
                desk.queue.retain(|job| job.id != id);
            }
            guard.inflight.remove(&id);
            Outgoing::text(504, "Gateway Timeout", "这台桌面没有应答。")
        }
    }
}

fn deliver(desk_addr: &str, job: &Job) -> Reply {
    if !is_guest(job) && !crate::desk::session_allows(&job.authorization) {
        return Reply {
            status: 401,
            content_type: "application/json; charset=utf-8".to_string(),
            body: r#"{"ok":false,"summary":"请重新进入。","data":{},"error":"failed"}"#
                .as_bytes()
                .to_vec(),
        };
    }
    match forward_tcp(desk_addr, job) {
        Ok(reply) => reply,
        Err(error) if refused(&error) => Reply {
            status: 502,
            content_type: "text/plain; charset=utf-8".to_string(),
            body: "这台桌面没有打开。".as_bytes().to_vec(),
        },
        Err(_) => Reply {
            status: 504,
            content_type: "text/plain; charset=utf-8".to_string(),
            body: "这台桌面没有应答。".as_bytes().to_vec(),
        },
    }
}

fn forward_tcp(desk_addr: &str, job: &Job) -> io::Result<Reply> {
    let mut stream = TcpStream::connect(desk_addr)?;
    stream.set_read_timeout(Some(Duration::from_secs(15)))?;
    stream.set_write_timeout(Some(Duration::from_secs(15)))?;
    let host = if job.host.is_empty() {
        desk_addr
    } else {
        job.host.as_str()
    };
    let mut head = format!(
        "{} {} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nContent-Length: {}\r\n",
        job.method,
        job.path,
        job.body.len()
    );
    if !job.authorization.is_empty() {
        head.push_str(&format!("Authorization: {}\r\n", job.authorization));
    }
    if !job.content_type.is_empty() {
        head.push_str(&format!("Content-Type: {}\r\n", job.content_type));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(&job.body)?;
    stream.flush()?;
    read_response(&mut stream)
}

fn read_response(stream: &mut TcpStream) -> io::Result<Reply> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 2048];
    let header_end = loop {
        if buf.len() > 1024 * 1024 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "too large"));
        }
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "closed"));
        }
        buf.extend_from_slice(&chunk[..read]);
        if let Some(end) = find_header_end(&buf) {
            break end;
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let status = head
        .split("\r\n")
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(502);
    let mut content_type = header_value(&head, "content-type");
    if content_type.is_empty() {
        content_type = "application/octet-stream".to_string();
    }
    let length = content_length(&head);
    let body_at = header_end + 4;
    let body = if let Some(length) = length {
        while buf.len() < body_at + length {
            let read = stream.read(&mut chunk)?;
            if read == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..read]);
        }
        buf.get(body_at..body_at + length)
            .unwrap_or_default()
            .to_vec()
    } else {
        let mut rest = buf.get(body_at..).unwrap_or_default().to_vec();
        loop {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => {
                    rest.extend_from_slice(&chunk[..read]);
                    if rest.len() > 3 * 1024 * 1024 {
                        break;
                    }
                }
                Err(error)
                    if error.kind() == io::ErrorKind::WouldBlock
                        || error.kind() == io::ErrorKind::TimedOut =>
                {
                    break;
                }
                Err(error) => return Err(error),
            }
        }
        rest
    };
    Ok(Reply {
        status,
        content_type,
        body,
    })
}

fn register_remote(link: &LinkFile) -> Result<String, String> {
    let value = post_json(
        &format!("{}/api/entrance/register", link.at),
        None,
        &json!({"name": link.name}),
    )?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(summary_of(&value));
    }
    value
        .get("token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "入口没有答上来。".to_string())
}

fn poll_remote(link: &LinkFile) -> Result<Option<Job>, String> {
    let value = post_json(
        &format!("{}/api/entrance/poll", link.at),
        Some(&link.token),
        &json!({"name": link.name}),
    )?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(summary_of(&value));
    }
    let Some(job) = value.get("job") else {
        return Ok(None);
    };
    parse_job(job).map(Some)
}

fn reply_remote(link: &LinkFile, id: &str, reply: &Reply) -> Result<(), String> {
    let value = post_json(
        &format!("{}/api/entrance/reply", link.at),
        Some(&link.token),
        &json!({
            "name": link.name,
            "id": id,
            "status": reply.status,
            "contentType": reply.content_type,
            "body": base64::engine::general_purpose::STANDARD.encode(&reply.body),
        }),
    )?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(summary_of(&value));
    }
    Ok(())
}

fn post_json(url: &str, token: Option<&str>, body: &Value) -> Result<Value, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(25))
        .build();
    let mut request = agent.post(url);
    if let Some(token) = token {
        request = request.set("Authorization", &format!("Bearer {token}"));
    }
    let response = match request.send_json(body) {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(_) => return Err("连不上入口。".to_string()),
    };
    response
        .into_json::<Value>()
        .map_err(|_| "入口没有答上来。".to_string())
}

fn parse_job(value: &Value) -> Result<Job, String> {
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if id.is_empty() {
        return Err("入口没有答上来。".to_string());
    }
    let body = match value.get("body").and_then(Value::as_str).unwrap_or("") {
        "" => Vec::new(),
        text => base64::engine::general_purpose::STANDARD
            .decode(text)
            .map_err(|_| "入口没有答上来。".to_string())?,
    };
    Ok(Job {
        id,
        method: value
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("GET")
            .to_string(),
        path: value
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("/")
            .to_string(),
        host: value
            .get("host")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        authorization: value
            .get("authorization")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        content_type: value
            .get("contentType")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        body,
    })
}

fn summary_of(value: &Value) -> String {
    value
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or("入口没有答上来。")
        .to_string()
}

fn is_guest(job: &Job) -> bool {
    if job.method == "GET" || job.method == "HEAD" {
        return true;
    }
    if job.method != "POST" {
        return false;
    }
    let path = job.path.split('?').next().unwrap_or(job.path.as_str());
    if path != "/api/exec" {
        return false;
    }
    let Ok(parsed) = serde_json::from_slice::<Value>(&job.body) else {
        return false;
    };
    let Some(items) = parsed.get("argv").and_then(Value::as_array) else {
        return false;
    };
    let mut names = Vec::with_capacity(items.len());
    for item in items {
        let Some(text) = item.as_str() else {
            return false;
        };
        names.push(text);
    }
    matches!(
        names.as_slice(),
        ["desk", "identity", "status"] | ["desk", "identity", "login", ..]
    )
}

fn json_body(body: &[u8]) -> Result<Value, Outgoing> {
    serde_json::from_slice(body).map_err(|_| {
        Outgoing::json(
            400,
            "Bad Request",
            json!({"ok": false, "summary": "请求体不是 JSON。"}),
        )
    })
}

fn required_name(value: &Value) -> Result<String, Outgoing> {
    let raw = value.get("name").and_then(Value::as_str).unwrap_or("");
    normalize_name(raw)
        .map_err(|text| Outgoing::json(400, "Bad Request", json!({"ok": false, "summary": text})))
}

fn unknown() -> Outgoing {
    Outgoing::json(
        401,
        "Unauthorized",
        json!({"ok": false, "summary": "入口不认识这台桌面。"}),
    )
}

fn job_value(job: &Queued) -> Value {
    json!({
        "ok": true,
        "job": {
            "id": job.id,
            "method": job.method,
            "path": job.path,
            "host": job.host,
            "authorization": job.authorization,
            "contentType": job.content_type,
            "body": base64::engine::general_purpose::STANDARD.encode(&job.body),
        }
    })
}

fn outgoing_from_reply(reply: Reply) -> Outgoing {
    Outgoing {
        status: reply.status,
        reason: reason_phrase(reply.status).to_string(),
        content_type: safe_type(&reply.content_type),
        body: reply.body,
    }
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        421 => "Misdirected Request",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "OK",
    }
}

fn safe_type(value: &str) -> String {
    if value.is_empty() || field_breaks(value) {
        "application/octet-stream".to_string()
    } else {
        value.to_string()
    }
}

fn method_ok(method: &str) -> bool {
    !method.is_empty() && method.bytes().all(|byte| byte.is_ascii_uppercase())
}

fn path_ok(path: &str) -> bool {
    path.starts_with('/') && !field_breaks(path) && !path.contains(' ')
}

fn field_breaks(value: &str) -> bool {
    value.contains('\r') || value.contains('\n')
}

fn find_mut(guard: &Memory, name: &str) -> Option<usize> {
    guard.desktops.iter().position(|desk| desk.name == name)
}

fn authed_index(guard: &Memory, name: &str, authorization: &str) -> Option<usize> {
    let index = find_mut(guard, name)?;
    if bearer_matches(&guard.desktops[index], authorization) {
        Some(index)
    } else {
        None
    }
}

fn bearer_matches(desk: &DeskLive, authorization: &str) -> bool {
    let Some(token) = bearer(authorization) else {
        return false;
    };
    hash_eq(&desk.token_hash, &token_hash(token))
}

fn bearer(header: &str) -> Option<&str> {
    let token = header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("bearer "))?;
    let token = token.trim();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

fn touch(guard: &mut Memory, index: usize, dir: &Path) {
    let now = now_ms();
    let desk = &mut guard.desktops[index];
    desk.seen_ms = now;
    if now.saturating_sub(desk.last_flush_ms) >= FLUSH_MS {
        desk.last_flush_ms = now;
        let _ = write_desktops(dir, &guard.desktops);
    }
}

fn is_connected(seen_ms: u64, now: u64) -> bool {
    seen_ms > 0 && now.saturating_sub(seen_ms) < CONNECTED_MS
}

fn names_match(stored: &str, host: &str) -> bool {
    let host = host_name(host);
    host == stored || idna::domain_to_ascii(&host).ok().as_deref() == Some(stored)
}

fn host_name(host: &str) -> String {
    let host = host.trim();
    let name = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or(rest)
    } else {
        host.split(':').next().unwrap_or(host)
    };
    name.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn lock(entrance: &Entrance) -> MutexGuard<'_, Memory> {
    entrance
        .inner
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

fn link_report(link: &LinkFile, saved: bool) -> Report {
    let mut summary = link_summary(link);
    if saved && !link.at.is_empty() && !link.name.is_empty() {
        summary.push_str("还要从这台桌面向外连。");
    }
    let mut data = Map::new();
    data.insert("name".into(), Value::String(link.name.clone()));
    data.insert("at".into(), Value::String(link.at.clone()));
    Report::ok(summary).with_data(data)
}

fn link_summary(link: &LinkFile) -> String {
    if link.name.is_empty() {
        "这台桌面还没有名字，也还没有云入口。".to_string()
    } else if link.at.is_empty() {
        format!("这台桌面叫 {}。公网入口在它自己身上。", link.name)
    } else {
        format!("这台桌面叫 {}。它向外连 {}。", link.name, link.at)
    }
}

fn load_link() -> Result<LinkFile, Report> {
    load_link_at(&state_dir())
}

fn load_link_at(dir: &Path) -> Result<LinkFile, Report> {
    let path = dir.join("link.json");
    if !path.exists() {
        return Ok(LinkFile {
            at: String::new(),
            name: String::new(),
            token: String::new(),
        });
    }
    let text = fs::read_to_string(path).map_err(|_| Report::fail("工作区暂时打不开。"))?;
    serde_json::from_str(&text).map_err(|_| Report::fail("工作区暂时打不开。"))
}

fn store_link(link: &LinkFile) -> Result<(), Report> {
    let dir = state_dir();
    fs::create_dir_all(&dir).map_err(|_| Report::fail("入口没有记下。"))?;
    let temporary = dir.join(".link.json.tmp");
    let text = serde_json::to_string_pretty(link).map_err(|_| Report::fail("入口没有记下。"))?;
    fs::write(&temporary, text).map_err(|_| Report::fail("入口没有记下。"))?;
    restrict_to_owner(&temporary).map_err(|_| Report::fail("入口没有记下。"))?;
    fs::rename(&temporary, dir.join("link.json")).map_err(|_| Report::fail("入口没有记下。"))?;
    Ok(())
}

fn read_desktops(dir: &Path) -> Result<Vec<DesktopFile>, ()> {
    let path = dir.join("desktops.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path).map_err(|_| ())?;
    let list: DesktopList = serde_json::from_str(&text).map_err(|_| ())?;
    Ok(list.desktops)
}

fn write_desktops(dir: &Path, desktops: &[DeskLive]) -> Result<(), ()> {
    fs::create_dir_all(dir).map_err(|_| ())?;
    let list = DesktopList {
        desktops: desktops
            .iter()
            .map(|desk| DesktopFile {
                name: desk.name.clone(),
                token_hash: desk.token_hash.clone(),
                seen_ms: desk.seen_ms,
            })
            .collect(),
    };
    let temporary = dir.join(".desktops.json.tmp");
    let text = serde_json::to_string_pretty(&list).map_err(|_| ())?;
    fs::write(&temporary, text).map_err(|_| ())?;
    restrict_to_owner(&temporary)?;
    fs::rename(&temporary, dir.join("desktops.json")).map_err(|_| ())?;
    Ok(())
}

fn normalize_name(raw: &str) -> Result<String, &'static str> {
    let raw = raw.trim().trim_end_matches('.');
    if raw.is_empty()
        || raw.contains("://")
        || raw.contains('/')
        || raw.contains(':')
        || raw.contains(char::is_whitespace)
    {
        return Err("域名格式不正确");
    }
    if raw.parse::<IpAddr>().is_ok() {
        return Err("域名格式不正确");
    }
    let ascii = idna::domain_to_ascii(raw).map_err(|_| "域名格式不正确")?;
    if !ascii.contains('.') || ascii.starts_with('.') || ascii.ends_with('.') {
        return Err("域名格式不正确");
    }
    Ok(ascii)
}

fn normalize_at(raw: &str) -> Result<String, &'static str> {
    let raw = raw.trim();
    let parsed = url::Url::parse(raw).map_err(|_| "入口地址不正确")?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err("入口地址不正确");
    }
    if parsed.host_str().is_none() || !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("入口地址不正确");
    }
    let mut text = parsed.to_string();
    while text.ends_with('/') {
        text.pop();
    }
    if text.is_empty() {
        return Err("入口地址不正确");
    }
    Ok(text)
}

fn token_hash(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(digest)
}

fn hash_eq(left: &str, right: &str) -> bool {
    left.len() == right.len()
        && left
            .bytes()
            .zip(right.bytes())
            .fold(0u8, |diff, (one, other)| diff | (one ^ other))
            == 0
}

fn random_token() -> Result<String, ()> {
    let mut bytes = [0u8; 32];
    #[cfg(unix)]
    {
        let mut file = fs::File::open("/dev/urandom").map_err(|_| ())?;
        file.read_exact(&mut bytes).map_err(|_| ())?;
    }
    #[cfg(not(unix))]
    {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ())?
            .as_nanos();
        let mixed = Sha256::digest(format!("{nanos}-{}", std::process::id()).as_bytes());
        bytes.copy_from_slice(&mixed);
    }
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn refused(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::ConnectionRefused
        || matches!(error.raw_os_error(), Some(61 | 111))
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|window| window == b"\r\n\r\n")
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
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            return value.trim().parse().ok();
        }
    }
    None
}

#[cfg(test)]
pub fn forget(entrance: &Entrance, name: &str) {
    let mut guard = lock(entrance);
    if let Some(desk) = guard.desktops.iter_mut().find(|desk| desk.name == name) {
        desk.seen_ms = 0;
        desk.last_flush_ms = 0;
    }
    let _ = write_desktops(&entrance.dir, &guard.desktops);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_desk_and_missing_session_stay_on_this_side() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let closed = listener.local_addr().unwrap();
        drop(listener);
        let missing = deliver(
            &closed.to_string(),
            &Job {
                id: "1".into(),
                method: "GET".into(),
                path: "/".into(),
                host: "home.example".into(),
                authorization: String::new(),
                content_type: String::new(),
                body: Vec::new(),
            },
        );
        assert_eq!(missing.status, 502);
        assert_eq!(String::from_utf8_lossy(&missing.body), "这台桌面没有打开。");

        let locked = deliver(
            "127.0.0.1:9",
            &Job {
                id: "2".into(),
                method: "POST".into(),
                path: "/api/exec".into(),
                host: "home.example".into(),
                authorization: String::new(),
                content_type: "application/json".into(),
                body: br#"{"argv":["desk","kernel","status"]}"#.to_vec(),
            },
        );
        assert_eq!(locked.status, 401);
        assert!(String::from_utf8_lossy(&locked.body).contains("请重新进入。"));
    }

    #[test]
    fn guest_login_reaches_the_local_page() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
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
                        if find_header_end(&buf).is_some() {
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
        let reply = deliver(
            &address.to_string(),
            &Job {
                id: "3".into(),
                method: "POST".into(),
                path: "/api/exec".into(),
                host: "home.example".into(),
                authorization: String::new(),
                content_type: "application/json".into(),
                body: br#"{"argv":["desk","identity","login","--password","x"]}"#.to_vec(),
            },
        );
        assert_eq!(reply.status, 200);
        assert_eq!(String::from_utf8_lossy(&reply.body), "你好");
    }
}
