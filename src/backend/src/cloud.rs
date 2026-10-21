use crate::command::Report;
use crate::desk::{probe_addr, restrict_to_owner, state_dir};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs;
use std::io::{self, Cursor};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, TcpListener, ToSocketAddrs, UdpSocket};
use std::sync::Arc;

#[derive(Clone, Serialize, Deserialize)]
pub struct CloudState {
    pub domain: String,
    #[serde(default)]
    pub publish: String,
    #[serde(default)]
    pub tls: String,
    #[serde(default)]
    pub certificate: String,
}

pub struct Identity {
    pub cert_pem: String,
    pub key_pem: String,
    pub kind: &'static str,
}

fn cloud_path() -> std::path::PathBuf {
    state_dir().join("cloud.json")
}

pub fn wipe() -> Result<(), ()> {
    remove_file(&cloud_path())?;
    remove_file(&cert_path())?;
    remove_file(&key_path())
}

fn remove_file(path: &std::path::Path) -> Result<(), ()> {
    match fs::remove_file(path) {
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
            tls: String::new(),
            certificate: String::new(),
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

#[cfg(test)]
pub fn set_publish(address: &str) -> Result<(), Report> {
    let mut state = load()?;
    state.publish = address.to_string();
    store(&state)
}

pub fn set_entrance(publish: &str, tls: &str, certificate: &str) -> Result<(), Report> {
    let mut state = load()?;
    state.publish = publish.to_string();
    state.tls = tls.to_string();
    state.certificate = certificate.to_string();
    store(&state)
}

pub fn identity_for(
    domain: &str,
    cert_path: Option<&str>,
    key_path_arg: Option<&str>,
) -> Result<Identity, Report> {
    match (cert_path, key_path_arg) {
        (Some(cert), Some(key)) => read_identity(cert, key),
        (None, None) => issue_certificate(domain),
        _ => Err(Report::fail("证书和私钥要一起给出。")),
    }
}

pub fn issue_certificate(domain: &str) -> Result<Identity, Report> {
    let mut params =
        rcgen::CertificateParams::new(vec![domain.to_string(), format!("ood1.{domain}")]);
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, domain);
    let cert =
        rcgen::Certificate::from_params(params).map_err(|_| Report::fail("证书没有写成。"))?;
    let identity = Identity {
        cert_pem: cert
            .serialize_pem()
            .map_err(|_| Report::fail("证书没有写成。"))?,
        key_pem: cert.serialize_private_key_pem(),
        kind: "self",
    };
    write_identity(&identity)?;
    Ok(identity)
}

fn read_identity(cert: &str, key: &str) -> Result<Identity, Report> {
    let cert_pem = fs::read_to_string(cert).map_err(|_| Report::fail("证书读不出来。"))?;
    let key_pem = fs::read_to_string(key).map_err(|_| Report::fail("证书读不出来。"))?;
    let identity = Identity {
        cert_pem,
        key_pem,
        kind: "provided",
    };
    write_identity(&identity)?;
    Ok(identity)
}

fn write_identity(identity: &Identity) -> Result<(), Report> {
    let dir = state_dir();
    fs::create_dir_all(&dir).map_err(|_| Report::fail("证书没有写成。"))?;
    fs::write(cert_path(), &identity.cert_pem).map_err(|_| Report::fail("证书没有写成。"))?;
    let temporary = dir.join(".key.pem.tmp");
    fs::write(&temporary, &identity.key_pem).map_err(|_| Report::fail("证书没有写成。"))?;
    restrict_to_owner(&temporary).map_err(|_| Report::fail("证书没有写成。"))?;
    fs::rename(&temporary, key_path()).map_err(|_| Report::fail("证书没有写成。"))?;
    Ok(())
}

pub fn tls_server_config(identity: &Identity) -> Result<Arc<rustls::ServerConfig>, Report> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut certs = Cursor::new(identity.cert_pem.as_bytes());
    let certs = rustls_pemfile::certs(&mut certs)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| Report::fail("证书读不出来。"))?;
    let mut key = Cursor::new(identity.key_pem.as_bytes());
    let key = rustls_pemfile::private_key(&mut key)
        .map_err(|_| Report::fail("证书读不出来。"))?
        .ok_or_else(|| Report::fail("证书读不出来。"))?;
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|_| Report::fail("证书读不出来。"))?;
    Ok(Arc::new(config))
}

fn cert_path() -> std::path::PathBuf {
    state_dir().join("cert.pem")
}

fn key_path() -> std::path::PathBuf {
    state_dir().join("key.pem")
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
            443,
            false,
            false,
            point_to(),
        ));
    }
    let addresses = lookup(domain);
    let points_here = addresses.iter().any(|ip| address_is_local(*ip));
    let (listening, port) = probe_publish(&state.publish);
    let (secure, tls_port) = probe_tls(&state.tls);
    let self_signed = secure && state.certificate == "self" && !state.tls.is_empty();
    let point_to = point_to();
    let summary = summary_for(
        domain,
        port,
        tls_port,
        points_here,
        listening,
        secure,
        self_signed,
        point_to,
    );
    Report::ok(summary).with_data(status_data(
        state,
        addresses,
        points_here,
        listening,
        port,
        tls_port,
        secure,
        self_signed,
        point_to,
    ))
}

fn summary_for(
    domain: &str,
    port: u16,
    tls_port: u16,
    points_here: bool,
    listening: bool,
    secure: bool,
    self_signed: bool,
    point_to: Option<IpAddr>,
) -> String {
    if !listening {
        return with_point(
            format!("已记下域名 {domain}。别人还不能从网上打开。"),
            domain,
            point_to,
        );
    }
    if !points_here {
        return with_point(
            "本机已经在听。域名还没有指向这台机器。".to_string(),
            domain,
            point_to,
        );
    }
    if secure && self_signed {
        return format!(
            "可以用 {} 打开。证书是这台机器签的，浏览器会提醒。",
            https_url(domain, tls_port)
        );
    }
    if secure && port == 80 {
        return format!("可以从网上打开 {domain}。");
    }
    let url = public_url(domain, port);
    format!("可以用 {url} 打开。还没有证书。")
}

fn with_point(base: String, domain: &str, point_to: Option<IpAddr>) -> String {
    let Some(ip) = point_to else {
        return base;
    };
    if is_public(ip) {
        format!("{base}把 {domain} 和 ood1.{domain} 指到 {ip}。")
    } else {
        format!("{base}这台机器的地址是 {ip}，域名要指到能到达这里的公网地址。")
    }
}

pub fn https_url(domain: &str, port: u16) -> String {
    if port == 443 {
        format!("https://{domain}/")
    } else {
        format!("https://{domain}:{port}/")
    }
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
    tls_port: u16,
    secure: bool,
    self_signed: bool,
    point_to: Option<IpAddr>,
) -> Map<String, Value> {
    let domain = state.domain.as_str();
    let mut data = Map::new();
    data.insert("domain".into(), Value::String(domain.to_string()));
    data.insert("publish".into(), Value::String(state.publish.clone()));
    data.insert("tls".into(), Value::String(state.tls.clone()));
    data.insert(
        "certificate".into(),
        Value::String(state.certificate.clone()),
    );
    data.insert("selfSigned".into(), Value::Bool(self_signed));
    data.insert(
        "pointTo".into(),
        Value::String(point_to.map(|ip| ip.to_string()).unwrap_or_default()),
    );
    data.insert(
        "records".into(),
        Value::Array(match (domain.is_empty(), point_to) {
            (false, Some(ip)) => dns_records(domain, ip),
            _ => Vec::new(),
        }),
    );
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
    let mut ports = vec![port_value(port, listening)];
    if tls_port != port {
        ports.push(port_value(tls_port, secure));
    }
    data.insert("ports".into(), Value::Array(ports));
    if secure && !domain.is_empty() {
        data.insert("url".into(), Value::String(https_url(domain, tls_port)));
    } else if listening && !domain.is_empty() {
        data.insert("url".into(), Value::String(public_url(domain, port)));
    }
    data.insert(
        "reachable".into(),
        Value::Bool(points_here && listening && secure && !self_signed && port == 80),
    );
    data
}

fn dns_records(domain: &str, ip: IpAddr) -> Vec<Value> {
    let kind = if ip.is_ipv4() { "A" } else { "AAAA" };
    [domain.to_string(), format!("ood1.{domain}")]
        .into_iter()
        .map(|name| {
            let mut item = Map::new();
            item.insert("name".into(), Value::String(name));
            item.insert("type".into(), Value::String(kind.to_string()));
            item.insert("value".into(), Value::String(ip.to_string()));
            Value::Object(item)
        })
        .collect()
}

fn point_to() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("1.1.1.1:80").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    if routable(ip) {
        Some(ip)
    } else {
        None
    }
}

fn routable(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            !ip.is_unspecified()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_broadcast()
                && !ip.is_multicast()
        }
        IpAddr::V6(ip) => {
            let first = ip.segments()[0];
            !ip.is_unspecified()
                && !ip.is_loopback()
                && !ip.is_multicast()
                && (first & 0xffc0) != 0xfe80
                && !is_unique_local(ip)
        }
    }
}

fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => routable(IpAddr::V4(ip)) && !ip.is_private() && !is_shared_v4(ip),
        IpAddr::V6(ip) => routable(IpAddr::V6(ip)),
    }
}

fn is_shared_v4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 100 && (octets[1] & 0xc0) == 64
}

fn is_unique_local(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xfe00) == 0xfc00
}

fn probe_tls(tls: &str) -> (bool, u16) {
    if tls.is_empty() {
        return (probe_addr("127.0.0.1:443"), 443);
    }
    probe_publish(tls)
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
