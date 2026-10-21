use crate::command::Report;
use crate::desk::{probe_addr, restrict_to_owner, state_dir};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs;
use std::io;
use std::net::{IpAddr, TcpListener, ToSocketAddrs};

#[derive(Clone, Serialize, Deserialize)]
pub struct CloudState {
    pub domain: String,
    #[serde(default)]
    pub publish: String,
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

pub fn load() -> Result<CloudState, Report> {
    let path = cloud_path();
    if !path.exists() {
        return Ok(CloudState {
            domain: String::new(),
            publish: String::new(),
        });
    }
    let text = fs::read_to_string(path).map_err(|_| Report::fail("工作区暂时打不开。"))?;
    serde_json::from_str(&text).map_err(|_| Report::fail("工作区暂时打不开。"))
}

pub fn set_domain(raw: &str) -> Result<CloudState, Report> {
    let domain = normalize_domain(raw).map_err(Report::fail)?;
    let mut state = load()?;
    state.domain = domain;
    store(&state)?;
    Ok(state)
}

pub fn set_publish(address: &str) -> Result<(), Report> {
    let mut state = load()?;
    state.publish = address.to_string();
    store(&state)
}

fn store(state: &CloudState) -> Result<(), Report> {
    let dir = state_dir();
    fs::create_dir_all(&dir).map_err(|_| Report::fail("域名写不进去。"))?;
    let temporary = dir.join(".cloud.json.tmp");
    let text = serde_json::to_string_pretty(state).map_err(|_| Report::fail("域名写不进去。"))?;
    fs::write(&temporary, text).map_err(|_| Report::fail("域名写不进去。"))?;
    restrict_to_owner(&temporary).map_err(|_| Report::fail("域名写不进去。"))?;
    fs::rename(&temporary, cloud_path()).map_err(|_| Report::fail("域名写不进去。"))?;
    Ok(())
}

pub fn status_report(state: &CloudState) -> Report {
    let domain = state.domain.as_str();
    if domain.is_empty() {
        return Report::ok("还没有域名。记下域名之后，才能从网上打开。").with_data(status_data(
            state,
            Vec::new(),
            false,
            false,
            80,
            false,
        ));
    }
    let addresses = lookup(domain);
    let points_here = addresses.iter().any(|ip| address_is_local(*ip));
    let (listening, port) = probe_publish(&state.publish);
    let secure = probe_addr("127.0.0.1:443");
    let summary = summary_for(domain, port, points_here, listening, secure);
    Report::ok(summary).with_data(status_data(
        state,
        addresses,
        points_here,
        listening,
        port,
        secure,
    ))
}

fn summary_for(
    domain: &str,
    port: u16,
    points_here: bool,
    listening: bool,
    secure: bool,
) -> String {
    if !listening {
        return format!("已记下域名 {domain}。别人还不能从网上打开。");
    }
    if !points_here {
        return "本机已经在听。域名还没有指向这台机器。".to_string();
    }
    if secure && port == 80 {
        return format!("可以从网上打开 {domain}。");
    }
    let url = public_url(domain, port);
    format!("可以用 {url} 打开。还没有证书。")
}

pub fn public_url(domain: &str, port: u16) -> String {
    if port == 80 {
        format!("http://{domain}/")
    } else {
        format!("http://{domain}:{port}/")
    }
}

fn status_data(
    state: &CloudState,
    addresses: Vec<IpAddr>,
    points_here: bool,
    listening: bool,
    port: u16,
    secure: bool,
) -> Map<String, Value> {
    let domain = state.domain.as_str();
    let mut data = Map::new();
    data.insert("domain".into(), Value::String(domain.to_string()));
    data.insert("publish".into(), Value::String(state.publish.clone()));
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
    data.insert("listening".into(), Value::Bool(listening));
    data.insert("secure".into(), Value::Bool(secure));
    data.insert(
        "ports".into(),
        Value::Array(vec![port_value(port, listening), port_value(443, secure)]),
    );
    if listening && !domain.is_empty() {
        data.insert("url".into(), Value::String(public_url(domain, port)));
    }
    data.insert(
        "reachable".into(),
        Value::Bool(points_here && listening && secure && port == 80),
    );
    data
}

fn probe_publish(publish: &str) -> (bool, u16) {
    if publish.is_empty() {
        return (probe_addr("127.0.0.1:80"), 80);
    }
    let Some(port) = publish
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse().ok())
    else {
        return (false, 80);
    };
    let endpoint = if publish.starts_with("0.0.0.0:") || publish.starts_with("[::]:") {
        format!("127.0.0.1:{port}")
    } else {
        publish.to_string()
    };
    (probe_addr(&endpoint), port)
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
