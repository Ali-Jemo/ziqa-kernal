use alloc::format;
/// Minimal HTTP/1.1 GET client for ZiqaKernel
use alloc::vec::Vec;
use smoltcp::socket::tcp;
use smoltcp::wire::{IpAddress, IpEndpoint, Ipv4Address};

pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
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

    // Create TCP socket
    let tcp_rx_buffer = tcp::SocketBuffer::new(alloc::vec![0; 8192]);
    let tcp_tx_buffer = tcp::SocketBuffer::new(alloc::vec![0; 8192]);
    let tcp_socket = tcp::Socket::new(tcp_rx_buffer, tcp_tx_buffer);
    let handle = stack.sockets.add(tcp_socket);

    // Connect
    let endpoint = IpEndpoint::new(IpAddress::Ipv4(ip), port);
    let local_port = 49152 + (crate::timer::uptime_ms() as u16 % 16384);
    {
        let socket = stack.sockets.get_mut::<tcp::Socket>(handle);
        socket
            .connect(stack.iface.context(), endpoint, local_port)
            .map_err(|_| "TCP connect failed")?;
    }

    // Poll until connected
    for _ in 0..10000 {
        stack.poll();
        let socket = stack.sockets.get::<tcp::Socket>(handle);
        if socket.is_active() && socket.may_send() {
            break;
        }
    }

    // Send HTTP request
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        path, host
    );
    {
        let socket = stack.sockets.get_mut::<tcp::Socket>(handle);
        socket
            .send_slice(request.as_bytes())
            .map_err(|_| "HTTP send failed")?;
    }

    // Receive response
    let mut response_data = Vec::new();
    for _ in 0..100000 {
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
    }

    // Clean up
    stack.sockets.remove(handle);

    // Parse status code
    let status = parse_status(&response_data);

    // Extract body (after \r\n\r\n)
    let body = extract_body(&response_data);

    Ok(HttpResponse { status, body })
}

fn parse_status(data: &[u8]) -> u16 {
    // HTTP/1.1 200 OK
    if data.len() < 12 {
        return 0;
    }
    let status_str = core::str::from_utf8(&data[9..12]).unwrap_or("0");
    status_str.parse().unwrap_or(0)
}

fn extract_body(data: &[u8]) -> Vec<u8> {
    for i in 0..data.len().saturating_sub(3) {
        if &data[i..i + 4] == b"\r\n\r\n" {
            return data[i + 4..].to_vec();
        }
    }
    Vec::new()
}
