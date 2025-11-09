use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;

/// Represents an IP lease assigned to a container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpLease {
    pub ip_address: IpAddr,
    pub container_name: String,
    pub lease_time: DateTime<Utc>,
}

/// The IPAM state that gets persisted to YAML
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IpamState {
    pub pools: HashMap<String, PoolInfo>,
    pub leases: Vec<IpLease>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolInfo {
    pub pool_id: String,
    pub subnet: String,
    pub gateway: Option<String>,
}

// Docker IPAM Plugin API Request/Response types

#[derive(Debug, Serialize, Deserialize)]
pub struct CapabilitiesResponse {
    #[serde(rename = "RequiresMACAddress")]
    pub requires_mac_address: bool,
    #[serde(rename = "RequiresRequestReplay")]
    pub requires_request_replay: bool,
}

#[derive(Debug, Deserialize)]
pub struct RequestPoolRequest {
    #[serde(rename = "Pool")]
    pub pool: Option<String>,
    #[allow(dead_code)]
    #[serde(rename = "SubPool")]
    pub sub_pool: Option<String>,
    #[allow(dead_code)]
    #[serde(rename = "Options")]
    pub options: Option<HashMap<String, String>>,
    #[allow(dead_code)]
    #[serde(rename = "V6")]
    pub v6: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RequestPoolResponse {
    #[serde(rename = "PoolID")]
    pub pool_id: String,
    #[serde(rename = "Pool")]
    pub pool: String,
    #[serde(rename = "Data")]
    pub data: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct ReleasePoolRequest {
    #[serde(rename = "PoolID")]
    pub pool_id: String,
}

#[derive(Debug, Deserialize)]
pub struct RequestAddressRequest {
    #[serde(rename = "PoolID")]
    pub pool_id: String,
    #[serde(rename = "Address")]
    pub address: Option<String>,
    #[serde(rename = "Options")]
    pub options: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RequestAddressResponse {
    #[serde(rename = "Address")]
    pub address: String,
    #[serde(rename = "Data")]
    pub data: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct ReleaseAddressRequest {
    #[serde(rename = "PoolID")]
    pub pool_id: String,
    #[serde(rename = "Address")]
    pub address: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    #[serde(rename = "Err")]
    pub err: String,
}

impl ErrorResponse {
    pub fn new(msg: impl Into<String>) -> Self {
        Self { err: msg.into() }
    }
}
