/// Minimal resolver for ZiqaKernel network commands.
///
/// This is intentionally not a fake DNS table. Until a real UDP/DNS client is
/// wired into smoltcp, commands must use numeric IPv4 literals (plus localhost).
use smoltcp::wire::Ipv4Address;

pub fn resolve(hostname: &str) -> Option<Ipv4Address> {
    match hostname {
        "localhost" => Some(Ipv4Address::new(127, 0, 0, 1)),
        _ => parse_ipv4(hostname),
    }
}

pub fn supports_hostname_lookup() -> bool {
    false
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
