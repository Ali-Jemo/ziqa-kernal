/// Minimal HTTP/1.1 GET client for ZiqaKernel
use alloc::vec::Vec;
use alloc::string::String;
use smoltcp::socket::tcp;
use smoltcp::wire::{IpAddress, IpEndpoint, Ipv4Address};

pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub content_type: Option<String>,
    pub location: Option<String>,
}

/// Perform an HTTP GET request
pub fn get(
    host: &str,
    path: &str,
    ip: Ipv4Address,
    port: u16,
) -> Result<HttpResponse, &'static str> {
    let mut stack_guard = super::stack::TCPIP.lock();
    let stack = stack_guard.as_mut().ok_or("TCP/IP stack not initialized")?;

    let tcp_rx_buffer = tcp::SocketBuffer::new(alloc::vec![0; 8192]);
    let tcp_tx_buffer = tcp::SocketBuffer::new(alloc::vec![0; 8192]);
    let handle = stack.sockets.add(tcp::Socket::new(tcp_rx_buffer, tcp_tx_buffer));

    let endpoint = IpEndpoint::new(IpAddress::Ipv4(ip), port);
    let local_port = 49152 + (crate::timer::uptime_ms() as u16 % 16384);
    {
        let socket = stack.sockets.get_mut::<tcp::Socket>(handle);
        socket.connect(stack.iface.context(), endpoint, local_port)
            .map_err(|_| "TCP connect failed")?;
    }

    // Wait for connection
    let connect_start = crate::timer::uptime_ms();
    let mut connected = false;
    while crate::timer::uptime_ms() - connect_start < 5000 {
        stack.poll();
        if stack.sockets.get::<tcp::Socket>(handle).may_send() {
            connected = true;
            break;
        }
        x86_64::instructions::nop();
    }
    if !connected {
        stack.sockets.remove(handle);
        return Err("TCP connect timeout");
    }

    // Send request
    let request = alloc::format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: ZiqaKernel/1.0\r\nAccept: */*\r\nConnection: close\r\n\r\n",
        path, host
    );
    stack.sockets.get_mut::<tcp::Socket>(handle)
        .send_slice(request.as_bytes())
        .map_err(|_| "HTTP send failed")?;

    // Receive response
    let mut response_data: Vec<u8> = Vec::new();
    let recv_start = crate::timer::uptime_ms();
    while crate::timer::uptime_ms() - recv_start < 10000 {
        stack.poll();
        let socket = stack.sockets.get_mut::<tcp::Socket>(handle);
        if socket.can_recv() {
            let _ = socket.recv(|data| {
                response_data.extend_from_slice(data);
                (data.len(), ())
            });
        }
        if !socket.is_active() || (!socket.may_recv() && !response_data.is_empty()) {
            break;
        }
        x86_64::instructions::nop();
    }
    stack.sockets.remove(handle);

    let status = parse_status(&response_data);
    let (headers_raw, body) = split_headers_body(&response_data);
    let content_type = find_header(headers_raw, "content-type");
    let location = find_header(headers_raw, "location");

    Ok(HttpResponse { status, body, content_type, location })
}

fn parse_status(data: &[u8]) -> u16 {
    // "HTTP/1.1 200 ..."
    if data.len() < 12 { return 0; }
    core::str::from_utf8(&data[9..12]).unwrap_or("0").parse().unwrap_or(0)
}

fn split_headers_body(data: &[u8]) -> (&[u8], Vec<u8>) {
    for i in 0..data.len().saturating_sub(3) {
        if &data[i..i+4] == b"\r\n\r\n" {
            return (&data[..i], data[i+4..].to_vec());
        }
    }
    (data, Vec::new())
}

/// Case-insensitive header value lookup.
fn find_header(headers: &[u8], name: &str) -> Option<String> {
    let text = core::str::from_utf8(headers).ok()?;
    for line in text.lines().skip(1) {
        if let Some(colon) = line.find(':') {
            let key = line[..colon].trim();
            if key.eq_ignore_ascii_case(name) {
                return Some(String::from(line[colon+1..].trim()));
            }
        }
    }
    None
}
