/// DNS resolver for ZiqaKernel network commands.
///
/// Uses smoltcp's DNS socket when the TCP/IP stack is available. Numeric IPv4
/// literals and localhost are resolved without network I/O.
use alloc::vec;
use alloc::vec::Vec;
use smoltcp::socket::dns::{self, GetQueryResultError};
use smoltcp::wire::{DnsQueryType, IpAddress, Ipv4Address};

const DNS_TIMEOUT_MS: u64 = 5_000;

pub fn resolve(hostname: &str) -> Option<Ipv4Address> {
    match hostname {
        "localhost" => return Some(Ipv4Address::new(127, 0, 0, 1)),
        _ => {}
    }

    if let Some(ip) = parse_ipv4(hostname) {
        return Some(ip);
    }

    resolve_via_smoltcp(hostname)
}

pub fn supports_hostname_lookup() -> bool {
    super::stack::TCPIP.lock().is_some()
}

fn resolve_via_smoltcp(hostname: &str) -> Option<Ipv4Address> {
    let mut stack_guard = super::stack::TCPIP.lock();
    let stack = stack_guard.as_mut()?;

    let mut servers: Vec<IpAddress> = Vec::new();
    for server in stack.dns_servers.iter() {
        servers.push(IpAddress::Ipv4(*server));
    }

    // QEMU user networking exposes DNS at 10.0.2.3. Keep public DNS as a
    // fallback for bridged/tap networks.
    if servers.is_empty() {
        servers.push(IpAddress::Ipv4(Ipv4Address::new(10, 0, 2, 3)));
        servers.push(IpAddress::Ipv4(Ipv4Address::new(8, 8, 8, 8)));
    }

    let dns_socket = dns::Socket::new(&servers, vec![]);
    let handle = stack.sockets.add(dns_socket);

    let query = {
        let iface = &mut stack.iface;
        let sockets = &mut stack.sockets;
        sockets
            .get_mut::<dns::Socket>(handle)
            .start_query(iface.context(), hostname, DnsQueryType::A)
            .ok()?
    };

    let deadline = crate::timer::uptime_ms() + DNS_TIMEOUT_MS;
    let mut answer = None;

    while crate::timer::uptime_ms() < deadline {
        stack.poll();

        match stack
            .sockets
            .get_mut::<dns::Socket>(handle)
            .get_query_result(query)
        {
            Ok(addrs) => {
                if let Some(IpAddress::Ipv4(ip)) = addrs.iter().next() {
                    answer = Some(*ip);
                }
                break;
            }
            Err(GetQueryResultError::Pending) => {
                x86_64::instructions::nop();
            }
            Err(GetQueryResultError::Failed) => break,
        }
    }

    stack.sockets.remove(handle);
    answer
}

fn parse_ipv4(s: &str) -> Option<Ipv4Address> {
    let parts: alloc::vec::Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let a = parts[0].parse::<u8>().ok()?;
    let b = parts[1].parse::<u8>().ok()?;
    let c = parts[2].parse::<u8>().ok()?;
    let d = parts[3].parse::<u8>().ok()?;
    Some(Ipv4Address::new(a, b, c, d))
}
