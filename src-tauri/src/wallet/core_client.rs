#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the reviewed read-only Core client remains private until its later command boundary is approved"
    )
)]
#![cfg_attr(
    test,
    allow(
        dead_code,
        reason = "the production wrapper remains intentionally unregistered while private transport helpers are tested"
    )
)]

use crate::supervisor::{CoreAuthorityError, CoreConnectionAuthority, SupervisorState};
use serde::Deserialize;
use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{ERROR_INSUFFICIENT_BUFFER, NO_ERROR},
    NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCPROW_OWNER_PID, TCP_TABLE_OWNER_PID_ALL,
    },
    Networking::WinSock::AF_INET,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const OPERATION_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_HEADER_BYTES: usize = 8 * 1024;
const MAX_BODY_BYTES: usize = 64 * 1024;
const MAX_TCP_TABLE_BYTES: usize = 4 * 1024 * 1024;
const SUPPORTED_STATUS_VERSION: &str = "3";
pub(super) const SUPPORTED_WALLET_CORE_CONTRACT: &str = "vision-wallet-read-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WalletCoreClientError {
    CompatibilityUnavailable,
    CoreUnavailable,
    CoreIdentityChanged,
    PeerIdentityRejected,
    InvalidAddress,
    TransportFailed,
    ResponseRejected,
    ResponseTooLarge,
    AccountIdentityMismatch,
    AccountStateRejected,
}

pub(super) struct WalletCoreReadClient<'a> {
    authority: CoreConnectionAuthority<'a>,
}

#[cfg_attr(test, derive(Debug))]
#[derive(PartialEq, Eq)]
pub(super) struct WalletCoreAccountSnapshot {
    pub address: String,
    pub exists: bool,
    pub balance: u128,
    pub nonce: u64,
}

#[cfg_attr(test, derive(Debug))]
#[derive(PartialEq, Eq)]
pub(super) struct WalletCoreStatus {
    pub version: String,
    pub canonical_tip_height: u64,
    pub canonical_tip_hash: String,
    pub peer_count: usize,
    pub recovery_state: String,
}

pub(super) trait WalletCoreReadSource {
    fn account_snapshot(
        &self,
        address: &str,
    ) -> Result<WalletCoreAccountSnapshot, WalletCoreClientError>;
    fn status(&self) -> Result<WalletCoreStatus, WalletCoreClientError>;
    fn validated_identity_fingerprint(&self) -> Result<[u8; 32], WalletCoreClientError>;
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountBalanceWire {
    address: String,
    exists: bool,
    balance: u128,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountNonceWire {
    address: String,
    exists: bool,
    nonce: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StatusWire {
    version: String,
    canonical_tip_height: u64,
    canonical_tip_hash: String,
    cached_state_root_height: Option<u64>,
    cached_state_root: Option<String>,
    mempool_size: usize,
    peer_count: usize,
    durable_peer_count: usize,
    active_inbound_sessions: usize,
    active_outbound_sessions: usize,
    transient_peer_count: usize,
    dialable_peer_count: usize,
    mining: MiningStatusWire,
    recovery: RecoveryStatusWire,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MiningStatusWire {
    available: bool,
    active: bool,
    blocks_found: u64,
    recovery_state: String,
    paused_reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryStatusWire {
    state: String,
    peer_addr: Option<String>,
    local_height: Option<u64>,
    local_work: Option<u128>,
    local_tip_hash: Option<String>,
    remote_height: Option<u64>,
    remote_work: Option<u128>,
    remote_tip_hash: Option<String>,
    reason: Option<String>,
}

trait ReadAuthority {
    fn endpoint(&self) -> SocketAddrV4;
    fn validate_before(&self) -> Result<(), WalletCoreClientError>;
    fn validate_peer(&self, stream: &TcpStream) -> Result<(), WalletCoreClientError>;
    fn validate_after(&self) -> Result<(), WalletCoreClientError>;
}

impl ReadAuthority for CoreConnectionAuthority<'_> {
    fn endpoint(&self) -> SocketAddrV4 {
        SocketAddrV4::new(Ipv4Addr::LOCALHOST, self.api_port())
    }

    fn validate_before(&self) -> Result<(), WalletCoreClientError> {
        self.validate().map_err(map_authority_error)
    }

    fn validate_peer(&self, stream: &TcpStream) -> Result<(), WalletCoreClientError> {
        verify_connected_peer_owner(stream, self.expected_pid())
    }

    fn validate_after(&self) -> Result<(), WalletCoreClientError> {
        self.validate().map_err(map_authority_error)
    }
}

fn map_authority_error(error: CoreAuthorityError) -> WalletCoreClientError {
    match error {
        CoreAuthorityError::UnsupportedCompatibility => {
            WalletCoreClientError::CompatibilityUnavailable
        }
        CoreAuthorityError::CoreUnavailable => WalletCoreClientError::CoreUnavailable,
        CoreAuthorityError::CoreIdentityChanged => WalletCoreClientError::CoreIdentityChanged,
    }
}

impl<'a> WalletCoreReadClient<'a> {
    pub(super) fn from_supervisor(
        supervisor: &'a SupervisorState,
    ) -> Result<Self, WalletCoreClientError> {
        let authority = supervisor
            .wallet_core_connection_authority()
            .map_err(map_authority_error)?;
        Ok(Self { authority })
    }

    pub(super) fn account_snapshot(
        &self,
        address: &str,
    ) -> Result<WalletCoreAccountSnapshot, WalletCoreClientError> {
        account_snapshot_with(&self.authority, address)
    }

    pub(super) fn status(&self) -> Result<WalletCoreStatus, WalletCoreClientError> {
        status_with(&self.authority)
    }
}

impl WalletCoreReadSource for WalletCoreReadClient<'_> {
    fn account_snapshot(
        &self,
        address: &str,
    ) -> Result<WalletCoreAccountSnapshot, WalletCoreClientError> {
        WalletCoreReadClient::account_snapshot(self, address)
    }

    fn status(&self) -> Result<WalletCoreStatus, WalletCoreClientError> {
        WalletCoreReadClient::status(self)
    }

    fn validated_identity_fingerprint(&self) -> Result<[u8; 32], WalletCoreClientError> {
        self.authority.validate().map_err(map_authority_error)?;
        Ok(self.authority.wallet_identity_fingerprint())
    }
}

fn account_snapshot_with<A: ReadAuthority>(
    authority: &A,
    address: &str,
) -> Result<WalletCoreAccountSnapshot, WalletCoreClientError> {
    validate_address(address)?;
    let balance = read_balance_wire(authority, address)?;
    let nonce = read_nonce_wire(authority, address)?;
    if balance.address != address || nonce.address != address || balance.address != nonce.address {
        return Err(WalletCoreClientError::AccountIdentityMismatch);
    }
    if balance.exists != nonce.exists
        || (!balance.exists && (balance.balance != 0 || nonce.nonce != 0))
    {
        return Err(WalletCoreClientError::AccountStateRejected);
    }
    Ok(WalletCoreAccountSnapshot {
        address: balance.address,
        exists: balance.exists,
        balance: balance.balance,
        nonce: nonce.nonce,
    })
}

fn read_balance_wire<A: ReadAuthority>(
    authority: &A,
    address: &str,
) -> Result<AccountBalanceWire, WalletCoreClientError> {
    read_json(authority, &format!("/balance/{address}"))
}

fn read_nonce_wire<A: ReadAuthority>(
    authority: &A,
    address: &str,
) -> Result<AccountNonceWire, WalletCoreClientError> {
    read_json(authority, &format!("/nonce/{address}"))
}

fn status_with<A: ReadAuthority>(authority: &A) -> Result<WalletCoreStatus, WalletCoreClientError> {
    let response: StatusWire = read_json(authority, "/status")?;
    validate_status_contract(&response)?;
    Ok(WalletCoreStatus {
        version: response.version,
        canonical_tip_height: response.canonical_tip_height,
        canonical_tip_hash: response.canonical_tip_hash,
        peer_count: response.peer_count,
        recovery_state: response.recovery.state,
    })
}

fn validate_address(address: &str) -> Result<(), WalletCoreClientError> {
    if is_canonical_lower_hex_32(address) {
        Ok(())
    } else {
        Err(WalletCoreClientError::InvalidAddress)
    }
}

fn is_canonical_lower_hex_32(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_status_contract(status: &StatusWire) -> Result<(), WalletCoreClientError> {
    let valid = status.version == SUPPORTED_STATUS_VERSION
        && is_canonical_lower_hex_32(&status.canonical_tip_hash)
        && status
            .cached_state_root
            .as_ref()
            .is_none_or(|value| is_canonical_lower_hex_32(value))
        && status.mining.recovery_state.len() <= 64
        && status
            .mining
            .paused_reason
            .as_ref()
            .is_none_or(|value| value.len() <= 512)
        && status.recovery.state.len() <= 64
        && status
            .recovery
            .peer_addr
            .as_ref()
            .is_none_or(|value| value.len() <= 256)
        && status
            .recovery
            .local_tip_hash
            .as_ref()
            .is_none_or(|value| is_canonical_lower_hex_32(value))
        && status
            .recovery
            .remote_tip_hash
            .as_ref()
            .is_none_or(|value| is_canonical_lower_hex_32(value))
        && status
            .recovery
            .reason
            .as_ref()
            .is_none_or(|value| value.len() <= 512);
    if valid {
        Ok(())
    } else {
        Err(WalletCoreClientError::ResponseRejected)
    }
}

fn read_json<T: for<'de> Deserialize<'de>, A: ReadAuthority>(
    authority: &A,
    path: &str,
) -> Result<T, WalletCoreClientError> {
    let deadline = Instant::now() + OPERATION_TIMEOUT;
    authority.validate_before()?;
    let endpoint = authority.endpoint();
    if endpoint.ip() != &Ipv4Addr::LOCALHOST {
        return Err(WalletCoreClientError::CoreIdentityChanged);
    }

    let connect_timeout = remaining_timeout(deadline)?.min(CONNECT_TIMEOUT);
    let mut stream = TcpStream::connect_timeout(&SocketAddr::V4(endpoint), connect_timeout)
        .map_err(|_| WalletCoreClientError::TransportFailed)?;
    let remaining = remaining_timeout(deadline)?;
    stream
        .set_read_timeout(Some(remaining))
        .map_err(|_| WalletCoreClientError::TransportFailed)?;
    stream
        .set_write_timeout(Some(remaining))
        .map_err(|_| WalletCoreClientError::TransportFailed)?;
    authority.validate_peer(&stream)?;
    authority.validate_before()?;
    let remaining = remaining_timeout(deadline)?;
    stream
        .set_read_timeout(Some(remaining))
        .map_err(|_| WalletCoreClientError::TransportFailed)?;
    stream
        .set_write_timeout(Some(remaining))
        .map_err(|_| WalletCoreClientError::TransportFailed)?;

    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAccept: application/json\r\nConnection: close\r\n\r\n",
        endpoint.port()
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|_| WalletCoreClientError::TransportFailed)?;
    stream
        .flush()
        .map_err(|_| WalletCoreClientError::TransportFailed)?;

    remaining_timeout(deadline)?;
    let body = read_http_response(&mut stream, deadline)?;
    authority.validate_after()?;
    remaining_timeout(deadline)?;
    authority.validate_peer(&stream)?;
    remaining_timeout(deadline)?;
    serde_json::from_slice(&body).map_err(|_| WalletCoreClientError::ResponseRejected)
}

fn remaining_timeout(deadline: Instant) -> Result<Duration, WalletCoreClientError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        Err(WalletCoreClientError::TransportFailed)
    } else {
        Ok(remaining)
    }
}

fn read_http_response(
    stream: &mut TcpStream,
    deadline: Instant,
) -> Result<Vec<u8>, WalletCoreClientError> {
    let mut received = Vec::with_capacity(2048);
    let header_end = loop {
        if let Some(position) = received.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
        if received.len() >= MAX_HEADER_BYTES {
            return Err(WalletCoreClientError::ResponseTooLarge);
        }
        let mut chunk = [0_u8; 1024];
        stream
            .set_read_timeout(Some(remaining_timeout(deadline)?))
            .map_err(|_| WalletCoreClientError::TransportFailed)?;
        let read = stream
            .read(&mut chunk)
            .map_err(|_| WalletCoreClientError::TransportFailed)?;
        if read == 0 {
            return Err(WalletCoreClientError::ResponseRejected);
        }
        received.extend_from_slice(&chunk[..read]);
        if received.len() > MAX_HEADER_BYTES + MAX_BODY_BYTES {
            return Err(WalletCoreClientError::ResponseTooLarge);
        }
    };

    if header_end > MAX_HEADER_BYTES {
        return Err(WalletCoreClientError::ResponseTooLarge);
    }
    let header = std::str::from_utf8(&received[..header_end])
        .map_err(|_| WalletCoreClientError::ResponseRejected)?;
    let content_length = validate_headers(header)?;
    if content_length > MAX_BODY_BYTES {
        return Err(WalletCoreClientError::ResponseTooLarge);
    }

    let mut body = received.split_off(header_end);
    if body.len() > content_length {
        return Err(WalletCoreClientError::ResponseRejected);
    }
    while body.len() < content_length {
        let remaining = content_length - body.len();
        let mut chunk = [0_u8; 4096];
        let read_limit = remaining.min(chunk.len());
        stream
            .set_read_timeout(Some(remaining_timeout(deadline)?))
            .map_err(|_| WalletCoreClientError::TransportFailed)?;
        let read = stream
            .read(&mut chunk[..read_limit])
            .map_err(|_| WalletCoreClientError::TransportFailed)?;
        if read == 0 {
            return Err(WalletCoreClientError::ResponseRejected);
        }
        body.extend_from_slice(&chunk[..read]);
    }
    Ok(body)
}

fn validate_headers(header: &str) -> Result<usize, WalletCoreClientError> {
    let mut lines = header.split("\r\n");
    let status = lines
        .next()
        .ok_or(WalletCoreClientError::ResponseRejected)?;
    let mut status_parts = status.split_ascii_whitespace();
    if status_parts.next() != Some("HTTP/1.1") || status_parts.next() != Some("200") {
        return Err(WalletCoreClientError::ResponseRejected);
    }

    let mut content_length = None;
    let mut content_type = None;
    for line in lines.filter(|line| !line.is_empty()) {
        let (name, value) = line
            .split_once(':')
            .ok_or(WalletCoreClientError::ResponseRejected)?;
        let name = name.trim();
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            if content_length.is_some() {
                return Err(WalletCoreClientError::ResponseRejected);
            }
            content_length = Some(
                value
                    .parse::<usize>()
                    .map_err(|_| WalletCoreClientError::ResponseRejected)?,
            );
        } else if name.eq_ignore_ascii_case("content-type") {
            if content_type.is_some() {
                return Err(WalletCoreClientError::ResponseRejected);
            }
            content_type = Some(value);
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(WalletCoreClientError::ResponseRejected);
        }
    }

    let media_type = content_type
        .ok_or(WalletCoreClientError::ResponseRejected)?
        .split(';')
        .next()
        .unwrap_or_default()
        .trim();
    if !media_type.eq_ignore_ascii_case("application/json") {
        return Err(WalletCoreClientError::ResponseRejected);
    }
    content_length.ok_or(WalletCoreClientError::ResponseRejected)
}

fn verify_connected_peer_owner(
    stream: &TcpStream,
    expected_pid: u32,
) -> Result<(), WalletCoreClientError> {
    let local = match stream.local_addr() {
        Ok(SocketAddr::V4(address)) if address.ip() == &Ipv4Addr::LOCALHOST => address,
        _ => return Err(WalletCoreClientError::PeerIdentityRejected),
    };
    let peer = match stream.peer_addr() {
        Ok(SocketAddr::V4(address)) if address.ip() == &Ipv4Addr::LOCALHOST => address,
        _ => return Err(WalletCoreClientError::PeerIdentityRejected),
    };

    let rows = tcp_owner_rows()?;
    let expected_server_addr = u32::from_ne_bytes(peer.ip().octets());
    let expected_client_addr = u32::from_ne_bytes(local.ip().octets());
    let matched = rows.iter().any(|row| {
        row.dwLocalAddr == expected_server_addr
            && mib_port(row.dwLocalPort) == peer.port()
            && row.dwRemoteAddr == expected_client_addr
            && mib_port(row.dwRemotePort) == local.port()
            && row.dwOwningPid == expected_pid
    });
    if matched {
        Ok(())
    } else {
        Err(WalletCoreClientError::PeerIdentityRejected)
    }
}

fn mib_port(value: u32) -> u16 {
    u16::from_be(value as u16)
}

fn tcp_owner_rows() -> Result<Vec<MIB_TCPROW_OWNER_PID>, WalletCoreClientError> {
    let mut size = 0_u32;
    let first = unsafe {
        GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut size,
            0,
            u32::from(AF_INET),
            TCP_TABLE_OWNER_PID_ALL,
            0,
        )
    };
    if first != ERROR_INSUFFICIENT_BUFFER || size == 0 || size as usize > MAX_TCP_TABLE_BYTES {
        return Err(WalletCoreClientError::PeerIdentityRejected);
    }

    for _ in 0..2 {
        let words = (size as usize).div_ceil(std::mem::size_of::<u64>());
        let mut storage = vec![0_u64; words];
        let result = unsafe {
            GetExtendedTcpTable(
                storage.as_mut_ptr().cast(),
                &mut size,
                0,
                u32::from(AF_INET),
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        };
        if result == ERROR_INSUFFICIENT_BUFFER {
            if size as usize > MAX_TCP_TABLE_BYTES {
                return Err(WalletCoreClientError::PeerIdentityRejected);
            }
            continue;
        }
        if result != NO_ERROR {
            return Err(WalletCoreClientError::PeerIdentityRejected);
        }

        let bytes = storage.as_ptr().cast::<u8>();
        let count = unsafe { *bytes.cast::<u32>() } as usize;
        let row_bytes = count
            .checked_mul(std::mem::size_of::<MIB_TCPROW_OWNER_PID>())
            .and_then(|value| value.checked_add(std::mem::size_of::<u32>()))
            .ok_or(WalletCoreClientError::PeerIdentityRejected)?;
        if row_bytes > size as usize || row_bytes > storage.len() * std::mem::size_of::<u64>() {
            return Err(WalletCoreClientError::PeerIdentityRejected);
        }
        let row_ptr =
            unsafe { bytes.add(std::mem::size_of::<u32>()) }.cast::<MIB_TCPROW_OWNER_PID>();
        let rows = unsafe { std::slice::from_raw_parts(row_ptr, count) };
        return Ok(rows.to_vec());
    }
    Err(WalletCoreClientError::PeerIdentityRejected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        net::TcpListener,
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc,
        },
        thread,
    };

    struct TestAuthority {
        endpoint: SocketAddrV4,
        generation: Arc<AtomicU64>,
        expected_generation: u64,
        expected_pid: u32,
    }

    impl ReadAuthority for TestAuthority {
        fn endpoint(&self) -> SocketAddrV4 {
            self.endpoint
        }

        fn validate_before(&self) -> Result<(), WalletCoreClientError> {
            self.validate_after()
        }

        fn validate_peer(&self, stream: &TcpStream) -> Result<(), WalletCoreClientError> {
            verify_connected_peer_owner(stream, self.expected_pid)
        }

        fn validate_after(&self) -> Result<(), WalletCoreClientError> {
            if self.generation.load(Ordering::SeqCst) == self.expected_generation {
                Ok(())
            } else {
                Err(WalletCoreClientError::CoreIdentityChanged)
            }
        }
    }

    fn serve_once(
        body: String,
        status: &'static str,
        extra_headers: &'static str,
        after_write: Option<Arc<AtomicU64>>,
    ) -> (TestAuthority, thread::JoinHandle<String>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let endpoint = match listener.local_addr().unwrap() {
            SocketAddr::V4(address) => address,
            SocketAddr::V6(_) => unreachable!(),
        };
        let generation = Arc::new(AtomicU64::new(7));
        let authority = TestAuthority {
            endpoint,
            generation,
            expected_generation: 7,
            expected_pid: std::process::id(),
        };
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let count = stream.read(&mut request).unwrap();
            let request = String::from_utf8(request[..count].to_vec()).unwrap();
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra_headers}Connection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
            if let Some(generation) = after_write {
                generation.store(8, Ordering::SeqCst);
            }
            thread::sleep(Duration::from_millis(100));
            request
        });
        (authority, server)
    }

    fn serve_json_sequence(
        bodies: Vec<String>,
    ) -> (TestAuthority, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let endpoint = match listener.local_addr().unwrap() {
            SocketAddr::V4(address) => address,
            SocketAddr::V6(_) => unreachable!(),
        };
        let authority = TestAuthority {
            endpoint,
            generation: Arc::new(AtomicU64::new(7)),
            expected_generation: 7,
            expected_pid: std::process::id(),
        };
        let server = thread::spawn(move || {
            let mut requests = Vec::with_capacity(bodies.len());
            for body in bodies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0_u8; 4096];
                let count = stream.read(&mut request).unwrap();
                requests.push(String::from_utf8(request[..count].to_vec()).unwrap());
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).unwrap();
                thread::sleep(Duration::from_millis(100));
            }
            requests
        });
        (authority, server)
    }

    fn status_json_with(version: &str, canonical_tip_hash: &str) -> String {
        serde_json::json!({
            "version": version,
            "canonical_tip_height": 7,
            "canonical_tip_hash": canonical_tip_hash,
            "cached_state_root_height": 7,
            "cached_state_root": "2".repeat(64),
            "mempool_size": 0,
            "peer_count": 2,
            "durable_peer_count": 2,
            "active_inbound_sessions": 1,
            "active_outbound_sessions": 1,
            "transient_peer_count": 0,
            "dialable_peer_count": 2,
            "mining": {
                "available": true,
                "active": false,
                "blocks_found": 0,
                "recovery_state": "normal",
                "paused_reason": null
            },
            "recovery": {
                "state": "normal",
                "peer_addr": null,
                "local_height": null,
                "local_work": null,
                "local_tip_hash": null,
                "remote_height": null,
                "remote_work": null,
                "remote_tip_hash": null,
                "reason": null
            }
        })
        .to_string()
    }

    fn status_json() -> String {
        status_json_with(SUPPORTED_STATUS_VERSION, &"1".repeat(64))
    }

    #[test]
    fn current_manifest_cannot_construct_production_client() {
        let supervisor = SupervisorState::default();
        let result = WalletCoreReadClient::from_supervisor(&supervisor);
        assert!(matches!(
            result,
            Err(WalletCoreClientError::CompatibilityUnavailable)
        ));
    }

    #[test]
    fn reads_one_consistent_account_snapshot_over_fresh_peer_proven_connections() {
        let address = "a".repeat(64);
        let balance = format!(
            "{{\"address\":\"{address}\",\"exists\":true,\"balance\":12345678901234567890}}"
        );
        let nonce = format!("{{\"address\":\"{address}\",\"exists\":true,\"nonce\":9}}");
        let (authority, server) = serve_json_sequence(vec![balance, nonce]);
        let snapshot = account_snapshot_with(&authority, &address).unwrap();
        let requests = server.join().unwrap();
        assert!(requests[0].starts_with(&format!("GET /balance/{address} HTTP/1.1\r\n")));
        assert!(requests[1].starts_with(&format!("GET /nonce/{address} HTTP/1.1\r\n")));
        for request in requests {
            assert!(request.contains("\r\nHost: 127.0.0.1:"));
            assert!(request.contains("\r\nConnection: close\r\n"));
        }
        assert_eq!(snapshot.address, address);
        assert!(snapshot.exists);
        assert_eq!(snapshot.balance, 12_345_678_901_234_567_890);
        assert_eq!(snapshot.nonce, 9);
    }

    #[test]
    fn reads_exact_supported_status() {
        let (status_authority, status_server) = serve_once(status_json(), "200 OK", "", None);
        let status = status_with(&status_authority).unwrap();
        status_server.join().unwrap();
        assert_eq!(status.version, SUPPORTED_STATUS_VERSION);
        assert_eq!(status.canonical_tip_height, 7);
        assert_eq!(status.canonical_tip_hash, "1".repeat(64));
        assert_eq!(status.peer_count, 2);
        assert_eq!(status.recovery_state, "normal");
    }

    #[test]
    fn rejects_cross_contract_account_inconsistency() {
        let address = "b".repeat(64);
        let other_address = "c".repeat(64);
        let cases = [
            (
                format!("{{\"address\":\"{address}\",\"exists\":true,\"balance\":1}}"),
                format!("{{\"address\":\"{other_address}\",\"exists\":true,\"nonce\":1}}"),
                WalletCoreClientError::AccountIdentityMismatch,
            ),
            (
                format!("{{\"address\":\"{address}\",\"exists\":true,\"balance\":1}}"),
                format!("{{\"address\":\"{address}\",\"exists\":false,\"nonce\":0}}"),
                WalletCoreClientError::AccountStateRejected,
            ),
            (
                format!("{{\"address\":\"{address}\",\"exists\":false,\"balance\":1}}"),
                format!("{{\"address\":\"{address}\",\"exists\":false,\"nonce\":0}}"),
                WalletCoreClientError::AccountStateRejected,
            ),
            (
                format!("{{\"address\":\"{address}\",\"exists\":false,\"balance\":0}}"),
                format!("{{\"address\":\"{address}\",\"exists\":false,\"nonce\":1}}"),
                WalletCoreClientError::AccountStateRejected,
            ),
        ];
        for (balance, nonce, expected) in cases {
            let (authority, server) = serve_json_sequence(vec![balance, nonce]);
            assert_eq!(account_snapshot_with(&authority, &address), Err(expected));
            server.join().unwrap();
        }
    }

    #[test]
    fn rejects_wrong_status_version_and_noncanonical_tip_hash() {
        for body in [
            status_json_with("4", &"1".repeat(64)),
            status_json_with(SUPPORTED_STATUS_VERSION, &"A".repeat(64)),
            status_json_with(SUPPORTED_STATUS_VERSION, &"1".repeat(63)),
        ] {
            let (authority, server) = serve_once(body, "200 OK", "", None);
            assert_eq!(
                status_with(&authority),
                Err(WalletCoreClientError::ResponseRejected)
            );
            server.join().unwrap();
        }
    }

    #[test]
    fn rejects_invalid_addresses_before_connecting() {
        for address in ["", &"A".repeat(64), &"g".repeat(64), &"a".repeat(63)] {
            assert_eq!(
                validate_address(address),
                Err(WalletCoreClientError::InvalidAddress)
            );
        }
    }

    #[test]
    fn rejects_wrong_connected_peer_owner() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let endpoint = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_millis(100));
        });
        let stream = TcpStream::connect(endpoint).unwrap();
        assert_eq!(
            verify_connected_peer_owner(&stream, std::process::id().wrapping_add(1)),
            Err(WalletCoreClientError::PeerIdentityRejected)
        );
        server.join().unwrap();
    }

    #[test]
    fn rejects_generation_change_after_response() {
        let generation = Arc::new(AtomicU64::new(7));
        let (mut authority, server) =
            serve_once(status_json(), "200 OK", "", Some(Arc::clone(&generation)));
        authority.generation = generation;
        assert_eq!(
            status_with(&authority),
            Err(WalletCoreClientError::CoreIdentityChanged)
        );
        server.join().unwrap();
    }

    #[test]
    fn rejects_redirects_chunking_and_unknown_fields() {
        let address = "c".repeat(64);
        let cases = [
            (
                format!("{{\"address\":\"{address}\",\"exists\":true,\"balance\":1}}"),
                "302 Found",
                "",
                WalletCoreClientError::ResponseRejected,
            ),
            (
                format!("{{\"address\":\"{address}\",\"exists\":true,\"balance\":1}}"),
                "200 OK",
                "Transfer-Encoding: chunked\r\n",
                WalletCoreClientError::ResponseRejected,
            ),
            (
                format!("{{\"address\":\"{address}\",\"exists\":true,\"balance\":1,\"extra\":0}}"),
                "200 OK",
                "",
                WalletCoreClientError::ResponseRejected,
            ),
        ];
        for (body, status, headers, expected) in cases {
            let (authority, server) = serve_once(body, status, headers, None);
            match read_balance_wire(&authority, &address) {
                Err(error) => assert_eq!(error, expected),
                Ok(_) => panic!("invalid response was accepted"),
            }
            server.join().unwrap();
        }
    }

    #[test]
    fn whole_response_deadline_rejects_a_slow_header() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let endpoint = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_millis(100));
            let body = "{}";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        });
        let mut stream = TcpStream::connect(endpoint).unwrap();
        let result = read_http_response(&mut stream, Instant::now() + Duration::from_millis(25));
        assert_eq!(result, Err(WalletCoreClientError::TransportFailed));
        server.join().unwrap();
    }

    #[test]
    fn source_contains_no_write_transport_or_tauri_boundary() {
        let source = include_str!("core_client.rs");
        assert!(!source.contains(&["PO", "ST "].concat()));
        assert!(!source.contains(&["#[tauri", "::command]"].concat()));
        assert!(!source.contains(&["req", "west"].concat()));
        assert!(!source.contains(&["Wallet", "Seed"].concat()));
        for result_type in ["WalletCoreAccountSnapshot", "WalletCoreStatus"] {
            assert!(!source.contains(&format!(
                "#[derive(Debug, PartialEq, Eq)]\npub(super) struct {result_type}"
            )));
        }
    }
}
