use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use smoltcp::iface::SocketHandle;
use smoltcp::socket::{tcp, udp};
use smoltcp::wire::{IpAddress, IpEndpoint, Ipv4Address};

/// Socket state machine for the Linux ABI socket syscalls.
///
/// Supports local (loopback) connections via socket pairs and real TCP/UDP
/// connections via smoltcp when the TCP/IP stack is available.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketState {
    Created,
    Bound,
    Listening,
    Connected,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SockDomain {
    Unix,
    Inet,
    Unknown(u32),
}

impl From<u32> for SockDomain {
    fn from(v: u32) -> Self {
        match v {
            1 => SockDomain::Unix,
            2 => SockDomain::Inet,
            _ => SockDomain::Unknown(v),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SockType {
    Stream,
    Dgram,
    Unknown(u32),
}

impl From<u32> for SockType {
    fn from(v: u32) -> Self {
        match v {
            1 => SockType::Stream,
            2 => SockType::Dgram,
            _ => SockType::Unknown(v),
        }
    }
}

/// Minimal socket address (AF_INET layout: family(2) + port(2) + addr(4) + zero(8))
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct SockAddrInet {
    pub family: u16,
    pub port: u16,
    pub addr: [u8; 4],
    pub zero: [u8; 8],
}

/// Default buffer sizes for smoltcp sockets created via the socket manager.
const SMOLTCP_TCP_BUF_SIZE: usize = 8192;
const SMOLTCP_UDP_BUF_SIZE: usize = 4096;

/// Timeout in milliseconds for TCP connect operations.
const TCP_CONNECT_TIMEOUT_MS: u64 = 5_000;

pub struct SocketEntry {
    pub domain: SockDomain,
    pub socktype: SockType,
    pub protocol: u32,
    pub state: SocketState,
    pub local_addr: Option<SockAddrInet>,
    pub remote_addr: Option<SockAddrInet>,
    pub pending: Vec<usize>,
    pub paired: Option<usize>,
    pub tx_buf: Vec<u8>,
    pub rx_buf: Vec<u8>,
    pub rx_pos: usize,
    /// smoltcp socket handle (for AF_INET sockets that are backed by the
    /// TCP/IP stack).  `None` for Unix-domain or loopback-only sockets.
    pub smoltcp_handle: Option<SocketHandle>,
}

impl SocketEntry {
    pub fn new(fd: usize, domain: u32, socktype: u32, protocol: u32) -> Self {
        let _ = fd;
        Self {
            domain: SockDomain::from(domain),
            socktype: SockType::from(socktype),
            protocol,
            state: SocketState::Created,
            local_addr: None,
            remote_addr: None,
            pending: Vec::new(),
            paired: None,
            tx_buf: Vec::new(),
            rx_buf: Vec::new(),
            rx_pos: 0,
            smoltcp_handle: None,
        }
    }
}

pub struct SocketManager {
    sockets: BTreeMap<usize, SocketEntry>,
}

impl SocketManager {
    pub const fn new() -> Self {
        Self {
            sockets: BTreeMap::new(),
        }
    }

    pub fn create(&mut self, fd: usize, domain: u32, socktype: u32, protocol: u32) {
        self.sockets
            .insert(fd, SocketEntry::new(fd, domain, socktype, protocol));
    }

    pub fn remove(&mut self, fd: usize) {
        if let Some(entry) = self.sockets.get(&fd) {
            if let Some(paired) = entry.paired {
                if let Some(p) = self.sockets.get_mut(&paired) {
                    p.state = SocketState::Closed;
                    p.paired = None;
                }
            }
        }
        self.sockets.remove(&fd);
    }

    pub fn get(&self, fd: usize) -> Option<&SocketEntry> {
        self.sockets.get(&fd)
    }

    pub fn get_mut(&mut self, fd: usize) -> Option<&mut SocketEntry> {
        self.sockets.get_mut(&fd)
    }

    pub fn exists(&self, fd: usize) -> bool {
        self.sockets.contains_key(&fd)
    }

    /// Find a listening socket matching the target address/port.
    pub fn find_listener(&self, port: u16, addr: [u8; 4]) -> Option<usize> {
        let wildcard = [0u8; 4];
        for (&fd, entry) in &self.sockets {
            if entry.state == SocketState::Listening {
                if let Some(local) = &entry.local_addr {
                    if local.port == port && (local.addr == wildcard || local.addr == addr) {
                        return Some(fd);
                    }
                }
            }
        }
        None
    }

    /// Find any listening socket (fallback when no port match).
    pub fn find_any_listener(&self) -> Option<usize> {
        for (&fd, entry) in &self.sockets {
            if entry.state == SocketState::Listening {
                return Some(fd);
            }
        }
        None
    }

    // -----------------------------------------------------------------------
    //  smoltcp-backed TCP operations
    // -----------------------------------------------------------------------

    /// Connect an AF_INET TCP socket to a remote address via smoltcp.
    ///
    /// Creates a smoltcp TCP socket, adds it to the `TcpIpStack`'s `SocketSet`,
    /// initiates the three-way handshake, and busy-waits until the connection
    /// is established or the timeout elapses.
    pub fn tcp_connect(
        &mut self,
        fd: usize,
        addr: [u8; 4],
        port: u16,
    ) -> Result<(), &'static str> {
        let entry = self.sockets.get_mut(&fd).ok_or("socket: bad fd")?;

        if entry.domain != SockDomain::Inet {
            return Err("socket: not AF_INET");
        }
        if entry.socktype != SockType::Stream {
            return Err("socket: not SOCK_STREAM");
        }
        if entry.state == SocketState::Connected {
            return Err("socket: already connected");
        }

        let remote_ip = Ipv4Address::new(addr[0], addr[1], addr[2], addr[3]);
        let endpoint = IpEndpoint::new(IpAddress::Ipv4(remote_ip), port);

        // Generate an ephemeral local port from the uptime counter to avoid
        // collisions across successive connections.
        let local_port = 49152 + (crate::timer::uptime_ms() as u16 % 16384);

        let mut stack_guard = super::stack::TCPIP.lock();
        let stack = stack_guard
            .as_mut()
            .ok_or("socket: TCP/IP stack not initialized")?;

        // Create the smoltcp TCP socket and initiate the connection.
        let handle = stack.add_tcp_socket(SMOLTCP_TCP_BUF_SIZE, SMOLTCP_TCP_BUF_SIZE);
        {
            let socket = stack.sockets.get_mut::<tcp::Socket>(handle);
            socket
                .connect(stack.iface.context(), endpoint, local_port)
                .map_err(|_| "socket: smoltcp connect failed")?;
        }

        // Busy-wait for the handshake to complete.
        let deadline = crate::timer::uptime_ms() + TCP_CONNECT_TIMEOUT_MS;
        let mut connected = false;
        while crate::timer::uptime_ms() < deadline {
            stack.poll();
            if stack.sockets.get::<tcp::Socket>(handle).may_send() {
                connected = true;
                break;
            }
            x86_64::instructions::nop();
        }

        if !connected {
            stack.remove_socket(handle);
            return Err("socket: connect timeout");
        }

        // Release the stack lock before mutating the SocketEntry so we don't
        // hold two locks simultaneously (stack_guard is dropped here).
        drop(stack_guard);

        let entry = self.sockets.get_mut(&fd).ok_or("socket: bad fd")?;
        entry.smoltcp_handle = Some(handle);
        entry.state = SocketState::Connected;
        entry.remote_addr = Some(SockAddrInet {
            family: 2, // AF_INET
            port,
            addr,
            zero: [0; 8],
        });

        crate::println!(
            "[socket] fd {} connected to {}.{}.{}.{}:{}",
            fd,
            addr[0],
            addr[1],
            addr[2],
            addr[3],
            port
        );
        Ok(())
    }

    /// Bind an AF_INET socket to a local address/port.
    ///
    /// For TCP sockets this records the bind address and transitions the
    /// socket to `Bound` state.  For UDP sockets it also creates a smoltcp
    /// UDP socket and binds it to the given port.
    pub fn tcp_bind(
        &mut self,
        fd: usize,
        addr: [u8; 4],
        port: u16,
    ) -> Result<(), &'static str> {
        let entry = self.sockets.get_mut(&fd).ok_or("socket: bad fd")?;

        if entry.domain != SockDomain::Inet {
            return Err("socket: not AF_INET");
        }
        if entry.state != SocketState::Created {
            return Err("socket: already bound");
        }

        entry.local_addr = Some(SockAddrInet {
            family: 2,
            port,
            addr,
            zero: [0; 8],
        });
        entry.state = SocketState::Bound;

        // For UDP sockets, eagerly create the smoltcp socket and bind it now
        // so that datagrams can be received immediately.
        if entry.socktype == SockType::Dgram {
            let mut stack_guard = super::stack::TCPIP.lock();
            let stack = stack_guard
                .as_mut()
                .ok_or("socket: TCP/IP stack not initialized")?;

            let handle = stack.add_udp_socket(SMOLTCP_UDP_BUF_SIZE, SMOLTCP_UDP_BUF_SIZE);
            {
                let socket = stack.sockets.get_mut::<udp::Socket>(handle);
                socket
                    .bind(port)
                    .map_err(|_| "socket: UDP bind failed")?;
            }

            drop(stack_guard);

            let entry = self.sockets.get_mut(&fd).ok_or("socket: bad fd")?;
            entry.smoltcp_handle = Some(handle);
        }

        crate::println!(
            "[socket] fd {} bound to {}.{}.{}.{}:{}",
            fd,
            addr[0],
            addr[1],
            addr[2],
            addr[3],
            port
        );
        Ok(())
    }

    /// Send data through a connected AF_INET TCP socket.
    ///
    /// Returns the number of bytes accepted by the smoltcp send buffer.
    pub fn tcp_send(&mut self, fd: usize, data: &[u8]) -> Result<usize, &'static str> {
        let entry = self.sockets.get(&fd).ok_or("socket: bad fd")?;

        if entry.state != SocketState::Connected {
            return Err("socket: not connected");
        }
        let handle = entry
            .smoltcp_handle
            .ok_or("socket: no smoltcp handle")?;

        let mut stack_guard = super::stack::TCPIP.lock();
        let stack = stack_guard
            .as_mut()
            .ok_or("socket: TCP/IP stack not initialized")?;

        let sent = stack
            .sockets
            .get_mut::<tcp::Socket>(handle)
            .send_slice(data)
            .map_err(|_| "socket: TCP send failed")?;

        // Drive the interface so the data is actually transmitted.
        stack.poll();

        Ok(sent)
    }

    /// Receive data from a connected AF_INET TCP socket.
    ///
    /// Returns the number of bytes read into `buf`.  Returns 0 when the
    /// remote end has closed the connection gracefully.
    pub fn tcp_recv(
        &mut self,
        fd: usize,
        buf: &mut [u8],
    ) -> Result<usize, &'static str> {
        let entry = self.sockets.get(&fd).ok_or("socket: bad fd")?;

        if entry.state != SocketState::Connected {
            return Err("socket: not connected");
        }
        let handle = entry
            .smoltcp_handle
            .ok_or("socket: no smoltcp handle")?;

        let mut stack_guard = super::stack::TCPIP.lock();
        let stack = stack_guard
            .as_mut()
            .ok_or("socket: TCP/IP stack not initialized")?;

        // Poll first so any pending incoming data is processed.
        stack.poll();

        let socket = stack.sockets.get_mut::<tcp::Socket>(handle);

        if !socket.is_active() && !socket.may_recv() {
            // Connection has been closed by the remote side.
            return Ok(0);
        }

        if !socket.can_recv() {
            // Nothing available right now but the connection is still open.
            return Ok(0);
        }

        let n = socket
            .recv_slice(buf)
            .map_err(|_| "socket: TCP recv failed")?;

        Ok(n)
    }

    // -----------------------------------------------------------------------
    //  smoltcp-backed UDP operations
    // -----------------------------------------------------------------------

    /// Send a UDP datagram to the specified remote address.
    ///
    /// If the socket does not yet have a smoltcp handle (i.e. it was not
    /// previously bound), an ephemeral UDP socket is created and bound
    /// automatically.
    pub fn udp_send(
        &mut self,
        fd: usize,
        addr: [u8; 4],
        port: u16,
        data: &[u8],
    ) -> Result<usize, &'static str> {
        let entry = self.sockets.get(&fd).ok_or("socket: bad fd")?;

        if entry.domain != SockDomain::Inet {
            return Err("socket: not AF_INET");
        }
        if entry.socktype != SockType::Dgram {
            return Err("socket: not SOCK_DGRAM");
        }

        let remote_ip = Ipv4Address::new(addr[0], addr[1], addr[2], addr[3]);
        let endpoint = IpEndpoint::new(IpAddress::Ipv4(remote_ip), port);

        let mut stack_guard = super::stack::TCPIP.lock();
        let stack = stack_guard
            .as_mut()
            .ok_or("socket: TCP/IP stack not initialized")?;

        // Lazily create and bind the smoltcp UDP socket if we don't have one
        // yet (the caller never called bind()).
        let handle = if let Some(h) = entry.smoltcp_handle {
            h
        } else {
            let h = stack.add_udp_socket(SMOLTCP_UDP_BUF_SIZE, SMOLTCP_UDP_BUF_SIZE);
            let ephemeral = 49152 + (crate::timer::uptime_ms() as u16 % 16384);
            stack
                .sockets
                .get_mut::<udp::Socket>(h)
                .bind(ephemeral)
                .map_err(|_| "socket: UDP auto-bind failed")?;

            // We need to store the handle back on the entry, but we're
            // borrowing `self` via `entry` already.  Drop the immutable
            // borrow and re-borrow mutably after we finish with the stack.
            drop(stack_guard);
            let entry_mut = self.sockets.get_mut(&fd).ok_or("socket: bad fd")?;
            entry_mut.smoltcp_handle = Some(h);
            entry_mut.state = SocketState::Bound;

            // Re-acquire the stack for the actual send.
            let mut sg = super::stack::TCPIP.lock();
            let st = sg.as_mut().ok_or("socket: TCP/IP stack not initialized")?;

            st.sockets
                .get_mut::<udp::Socket>(h)
                .send_slice(data, endpoint)
                .map_err(|_| "socket: UDP send failed")?;
            st.poll();
            return Ok(data.len());
        };

        stack
            .sockets
            .get_mut::<udp::Socket>(handle)
            .send_slice(data, endpoint)
            .map_err(|_| "socket: UDP send failed")?;

        stack.poll();
        Ok(data.len())
    }

    /// Receive a UDP datagram.
    ///
    /// Returns `(bytes_read, sender_ip, sender_port)`.  Returns an error if
    /// no datagram is available.
    pub fn udp_recv(
        &mut self,
        fd: usize,
        buf: &mut [u8],
    ) -> Result<(usize, [u8; 4], u16), &'static str> {
        let entry = self.sockets.get(&fd).ok_or("socket: bad fd")?;

        if entry.domain != SockDomain::Inet {
            return Err("socket: not AF_INET");
        }
        if entry.socktype != SockType::Dgram {
            return Err("socket: not SOCK_DGRAM");
        }
        let handle = entry
            .smoltcp_handle
            .ok_or("socket: UDP socket not bound")?;

        let mut stack_guard = super::stack::TCPIP.lock();
        let stack = stack_guard
            .as_mut()
            .ok_or("socket: TCP/IP stack not initialized")?;

        // Poll to pick up any waiting datagrams.
        stack.poll();

        let socket = stack.sockets.get_mut::<udp::Socket>(handle);

        if !socket.can_recv() {
            return Err("socket: no datagram available");
        }

        let (n, meta) = socket
            .recv_slice(buf)
            .map_err(|_| "socket: UDP recv failed")?;

        let sender_addr = match meta.endpoint.addr {
            IpAddress::Ipv4(ip) => ip.0,
        };
        let sender_port = meta.endpoint.port;

        Ok((n, sender_addr, sender_port))
    }

    // -----------------------------------------------------------------------
    //  Socket lifecycle
    // -----------------------------------------------------------------------

    /// Close a socket, removing its smoltcp handle from the TCP/IP stack if
    /// one is present, and cleaning up paired loopback sockets.
    ///
    /// After this call the fd is no longer valid.
    pub fn close(&mut self, fd: usize) -> Result<(), &'static str> {
        let entry = self.sockets.get_mut(&fd).ok_or("socket: bad fd")?;

        // If there is a smoltcp socket backing this entry, clean it up.
        if let Some(handle) = entry.smoltcp_handle.take() {
            // For TCP sockets, initiate a graceful close before removing.
            if entry.socktype == SockType::Stream {
                if let Some(mut stack_guard) = super::stack::TCPIP.try_lock() {
                    if let Some(stack) = stack_guard.as_mut() {
                        let socket = stack.sockets.get_mut::<tcp::Socket>(handle);
                        socket.close();
                        // Give the FIN a chance to be transmitted.
                        stack.poll();
                        stack.remove_socket(handle);
                    }
                }
            } else {
                // UDP – just remove.
                if let Some(mut stack_guard) = super::stack::TCPIP.try_lock() {
                    if let Some(stack) = stack_guard.as_mut() {
                        stack.sockets.get_mut::<udp::Socket>(handle).close();
                        stack.remove_socket(handle);
                    }
                }
            }
        }

        // Handle loopback socket pair clean-up (existing behaviour).
        if let Some(paired) = entry.paired {
            if let Some(p) = self.sockets.get_mut(&paired) {
                p.state = SocketState::Closed;
                p.paired = None;
            }
        }

        self.sockets.remove(&fd);
        Ok(())
    }
}

/// Global socket manager
pub static SOCKETS: spin::Mutex<SocketManager> = spin::Mutex::new(SocketManager::new());
