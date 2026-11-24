#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static SEQ: AtomicU64 = AtomicU64::new(1);

const GATEWAY_ADDR: &str = "127.0.0.1:3180";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Serialize)]
struct KrpcRequest {
    method: String,
    params: Value,
    sys: Vec<u64>,
}

#[derive(Debug, Deserialize)]
struct KrpcResponse {
    result: Option<Value>,
    error: Option<Value>,
    sys: Vec<u64>,
}

#[derive(Debug)]
pub enum KrpcError {
    Connect(io::Error),
    Request(io::Error),
    Response(String),
    SeqMismatch,
    ServerError(Value),
}

impl std::fmt::Display for KrpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KrpcError::Connect(e) => write!(f, "无法连接网关: {e}"),
            KrpcError::Request(e) => write!(f, "请求失败: {e}"),
            KrpcError::Response(msg) => write!(f, "响应错误: {msg}"),
            KrpcError::SeqMismatch => write!(f, "响应序号不匹配"),
            KrpcError::ServerError(e) => write!(f, "服务端错误: {e}"),
        }
    }
}

pub fn call(service: &str, method: &str, params: Value) -> Result<Value, KrpcError> {
    let seq = SEQ.fetch_add(1, Ordering::SeqCst);
    let full_method = format!("{service}.{method}");
    let request = KrpcRequest {
        method: full_method,
        params,
        sys: vec![seq],
    };
    let body = serde_json::to_string(&request).unwrap();
    let path = format!("/kapi/{service}");

    let mut stream = TcpStream::connect_timeout(
        &GATEWAY_ADDR.parse().unwrap(),
        CONNECT_TIMEOUT,
    )
    .map_err(KrpcError::Connect)?;

    stream
        .set_read_timeout(Some(READ_TIMEOUT))
        .map_err(KrpcError::Request)?;
    stream
        .set_write_timeout(Some(CONNECT_TIMEOUT))
        .map_err(KrpcError::Request)?;

    let http_request = format!(
        "POST {path} HTTP/1.1\r\n\
         Host: {GATEWAY_ADDR}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {body}",
        body.len()
    );

    stream
        .write_all(http_request.as_bytes())
        .map_err(KrpcError::Request)?;
    stream.flush().map_err(KrpcError::Request)?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(KrpcError::Request)?;

    let body_start = response
        .find("\r\n\r\n")
        .ok_or_else(|| KrpcError::Response("无效的HTTP响应".to_string()))?
        + 4;
    let body = &response[body_start..];

    let parsed: KrpcResponse = serde_json::from_str(body)
        .map_err(|e| KrpcError::Response(format!("JSON解析失败: {e}")))?;

    if parsed.sys.first() != Some(&seq) {
        return Err(KrpcError::SeqMismatch);
    }

    if let Some(error) = parsed.error {
        return Err(KrpcError::ServerError(error));
    }

    Ok(parsed.result.unwrap_or(Value::Null))
}

pub fn is_gateway_available() -> bool {
    TcpStream::connect_timeout(&GATEWAY_ADDR.parse().unwrap(), Duration::from_secs(1)).is_ok()
}

pub fn system_config_get(key: &str) -> Result<Value, KrpcError> {
    call("system_config", "get", json!({ "key": key }))
}

pub fn verify_hub_status() -> Result<Value, KrpcError> {
    call("verify_hub", "status", json!({}))
}

pub fn node_daemon_status() -> Result<Value, KrpcError> {
    call("node_daemon", "status", json!({}))
}
