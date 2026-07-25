use serde::{Deserialize, Serialize};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkDiagnostics {
    pub api_private_bind_valid: bool,
    pub seed_host: Option<String>,
    pub seed_port: Option<u16>,
    pub dns_resolved: bool,
    pub seed_reachable: Option<bool>,
    pub latency_ms: Option<f64>,
    pub public_reachability: String,
    pub nat_traversal: Vec<String>,
}

pub fn diagnose(seed: Option<String>, api_bind: String) -> NetworkDiagnostics {
    let api_private_bind_valid =
        api_bind.starts_with("127.0.0.1:") || api_bind.starts_with("[::1]:");
    let mut result = NetworkDiagnostics {
        api_private_bind_valid,
        seed_host: None,
        seed_port: None,
        dns_resolved: false,
        seed_reachable: None,
        latency_ms: None,
        public_reachability: "manual test required".to_string(),
        nat_traversal: vec![
            "UPnP: future".to_string(),
            "NAT-PMP: future".to_string(),
            "PCP: future".to_string(),
            "STUN/ICE: future".to_string(),
            "Relay: future".to_string(),
        ],
    };
    if let Some(seed) = seed {
        if let Some((host, port)) = seed.rsplit_once(':') {
            if let Ok(port) = port.parse::<u16>() {
                result.seed_host = Some(host.to_string());
                result.seed_port = Some(port);
                let addrs = (host, port).to_socket_addrs();
                if let Ok(mut addrs) = addrs {
                    result.dns_resolved = true;
                    if let Some(addr) = addrs.next() {
                        let start = Instant::now();
                        match TcpStream::connect_timeout(&addr, Duration::from_secs(3)) {
                            Ok(_) => {
                                result.seed_reachable = Some(true);
                                result.latency_ms = Some(start.elapsed().as_secs_f64() * 1000.0);
                            }
                            Err(_) => result.seed_reachable = Some(false),
                        }
                    }
                }
            }
        }
    }
    result
}
