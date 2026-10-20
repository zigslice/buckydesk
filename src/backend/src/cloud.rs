use crate::command::Report;
use crate::desk::{probe_addr, restrict_to_owner, state_dir};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs;
use std::io;
use std::net::{IpAddr, TcpListener, ToSocketAddrs};
use std::thread;

#[derive(Serialize, Deserialize)]
struct CloudFile {
    domain: String,
}

fn cloud_path() -> std::path::PathBuf {
    state_dir().join("cloud.json")
}

pub fn wipe() -> Result<(), ()> {
    match fs::remove_file(cloud_path()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(()),
    }
}

pub fn stored_domain() -> Result<String, Report> {
    let path = cloud_path();
    if !path.exists() {
        return Ok(String::new());
    }
    let text = fs::read_to_string(path).map_err(|_| Report::fail("工作区暂时打不开。"))?;
    let file: CloudFile =
        serde_json::from_str(&text).map_err(|_| Report::fail("工作区暂时打不开。"))?;
    Ok(file.domain)
}

pub fn set_domain(raw: &str) -> Result<String, Report> {
    let domain = normalize_domain(raw).map_err(Report::fail)?;
    let dir = state_dir();
    fs::create_dir_all(&dir).map_err(|_| Report::fail("域名写不进去。"))?;
    let temporary = dir.join(".cloud.json.tmp");
    let text = serde_json::to_string_pretty(&CloudFile {
        domain: domain.clone(),
    })
    .map_err(|_| Report::fail("域名写不进去。"))?;
    fs::write(&temporary, text).map_err(|_| Report::fail("域名写不进去。"))?;
    restrict_to_owner(&temporary).map_err(|_| Report::fail("域名写不进去。"))?;
    fs::rename(&temporary, cloud_path()).map_err(|_| Report::fail("域名写不进去。"))?;
    Ok(domain)
}

pub fn status_report(domain: &str) -> Report {
    if domain.is_empty() {
        return Report::ok("还没有域名。记下域名之后，才能从网上打开。").with_data(status_data(
            domain,
            Vec::new(),
            false,
            false,
            false,
        ));
    }
    let addresses = lookup(domain);
    let points_here = addresses.iter().any(|ip| address_is_local(*ip));
    let (http, https) = probe_gateway();
    let reachable = points_here && http && https;
    let summary = if reachable {
        format!("可以从网上打开 {domain}。")
    } else {
        format!("已记下域名 {domain}。别人还不能从网上打开。")
    };
    Report::ok(summary).with_data(status_data(domain, addresses, points_here, http, https))
}

fn status_data(
    domain: &str,
    addresses: Vec<IpAddr>,
    points_here: bool,
    http: bool,
    https: bool,
) -> Map<String, Value> {
    let mut data = Map::new();
    data.insert("domain".into(), Value::String(domain.to_string()));
    data.insert(
        "zoneDid".into(),
        Value::String(if domain.is_empty() {
            String::new()
        } else {
            format!("did:web:{domain}")
        }),
    );
    data.insert(
        "deviceDid".into(),
        Value::String(if domain.is_empty() {
            String::new()
        } else {
            format!("did:web:ood1.{domain}")
        }),
    );
    data.insert(
        "addresses".into(),
        Value::Array(
            addresses
                .into_iter()
                .map(|ip| Value::String(ip.to_string()))
                .collect(),
        ),
    );
    data.insert("dnsPointsHere".into(), Value::Bool(points_here));
    data.insert(
        "ports".into(),
        Value::Array(vec![port_value(80, http), port_value(443, https)]),
    );
    data.insert(
        "reachable".into(),
        Value::Bool(points_here && http && https),
    );
    data
}

fn port_value(port: u16, listening: bool) -> Value {
    let mut item = Map::new();
    item.insert("port".into(), Value::from(port));
    item.insert("listening".into(), Value::Bool(listening));
    Value::Object(item)
}

fn normalize_domain(raw: &str) -> Result<String, &'static str> {
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

fn lookup(domain: &str) -> Vec<IpAddr> {
    let Ok(found) = (domain, 443).to_socket_addrs() else {
        return Vec::new();
    };
    let mut addresses: Vec<IpAddr> = found.map(|address| address.ip()).collect();
    addresses.sort();
    addresses.dedup();
    addresses
}

fn address_is_local(ip: IpAddr) -> bool {
    TcpListener::bind((ip, 0)).is_ok()
}

fn probe_gateway() -> (bool, bool) {
    thread::scope(|scope| {
        let http = scope.spawn(|| probe_addr("127.0.0.1:80"));
        let https = scope.spawn(|| probe_addr("127.0.0.1:443"));
        (http.join().unwrap_or(false), https.join().unwrap_or(false))
    })
}
