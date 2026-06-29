use super::device::ZiqaDevice;
/// TCP/IP stack powered by smoltcp
use alloc::vec;
use alloc::vec::Vec;
use alloc::boxed::Box;
use smoltcp::iface::{Config, Interface, SocketHandle, SocketSet};
use smoltcp::socket::{dhcpv4, tcp, udp};
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, HardwareAddress, IpAddress, IpCidr, Ipv4Address, Ipv4Cidr};
use spin::Mutex;

/// Network configuration
const IP_ADDR: Ipv4Address = Ipv4Address::new(10, 0, 2, 15);
const GATEWAY: Ipv4Address = Ipv4Address::new(10, 0, 2, 2);
const MAC: [u8; 6] = [0x52, 0x54, 0x12, 0x34, 0x56, 0x78];

pub struct TcpIpStack {
    pub iface: Box<Interface>,
    pub device: ZiqaDevice,
    pub sockets: Box<SocketSet<'static>>,
    pub dhcp_handle: SocketHandle,
    pub dhcp_configured: bool,
    pub dns_servers: Vec<Ipv4Address>,
}

impl TcpIpStack {
    pub fn new() -> Self {
        let mut device = ZiqaDevice;

        let hw_addr = HardwareAddress::Ethernet(EthernetAddress(MAC));
        let config = Config::new(hw_addr);
        // Set random/default ports and keep packet buffers simple
        let mut iface = Box::new(Interface::new(config, &mut device, Self::now()));

        // Set IP address
        iface.update_ip_addrs(|addrs| {
            addrs.push(IpCidr::new(IpAddress::Ipv4(IP_ADDR), 24)).ok();
        });

        // Set default gateway
        iface.routes_mut().add_default_ipv4_route(GATEWAY).ok();

        let mut sockets = Box::new(SocketSet::new(vec![]));
        let dhcp_handle = sockets.add(dhcpv4::Socket::new());

        Self {
            iface,
            device,
            sockets,
            dhcp_handle,
            dhcp_configured: false,
            dns_servers: Vec::new(),
        }
    }

    pub fn now() -> Instant {
        let ms = crate::timer::uptime_ms();
        Instant::from_millis(ms as i64)
    }

    pub fn poll(&mut self) -> bool {
        let timestamp = Self::now();
        let changed = self.iface
            .poll(timestamp, &mut self.device, &mut *self.sockets);
        self.poll_dhcp();
        changed
    }

    fn poll_dhcp(&mut self) {
        let event = {
            let socket = self.sockets.get_mut::<dhcpv4::Socket>(self.dhcp_handle);
            match socket.poll() {
                Some(dhcpv4::Event::Configured(config)) => {
                    let mut dns_servers = Vec::new();
                    for server in config.dns_servers.iter() {
                        dns_servers.push(*server);
                    }
                    Some((true, config.address, config.router, dns_servers))
                }
                Some(dhcpv4::Event::Deconfigured) => {
                    Some((false, Ipv4Cidr::new(Ipv4Address::UNSPECIFIED, 0), None, Vec::new()))
                }
                None => None,
            }
        };

        match event {
            Some((true, address, router, dns_servers)) => {
                self.dhcp_configured = true;
                self.set_ipv4_addr(address);

                if let Some(router) = router {
                    let _ = self.iface.routes_mut().add_default_ipv4_route(router);
                }

                self.dns_servers = dns_servers;

                crate::println!(
                    "[DHCP] lease acquired ip={} dns={}",
                    address,
                    self.dns_servers.len()
                );
            }
            Some((false, address, _, _)) if self.dhcp_configured => {
                self.dhcp_configured = false;
                self.set_ipv4_addr(address);
                self.iface.routes_mut().remove_default_ipv4_route();
                self.dns_servers.clear();
                crate::println!("[DHCP] lease lost");
            }
            _ => {}
        }
    }

    fn set_ipv4_addr(&mut self, cidr: Ipv4Cidr) {
        self.iface.update_ip_addrs(|addrs| {
            if let Some(dest) = addrs.iter_mut().next() {
                *dest = IpCidr::Ipv4(cidr);
            } else {
                let _ = addrs.push(IpCidr::Ipv4(cidr));
            }
        });
    }

    pub fn ip_addr(&self) -> Ipv4Address {
        match self.iface.ipv4_addr().map(|cidr| cidr.into_address()) {
            Some(IpAddress::Ipv4(addr)) => addr,
            _ => IP_ADDR,
        }
    }

    pub fn gateway(&self) -> Ipv4Address {
        GATEWAY
    }

    pub fn mac(&self) -> [u8; 6] {
        MAC
    }

    /// Add a TCP socket to the stack and return its handle.
    ///
    /// `rx_size` and `tx_size` are the receive and transmit buffer sizes in
    /// bytes.  The caller owns the returned `SocketHandle` and must call
    /// `remove_socket` when the socket is no longer needed.
    pub fn add_tcp_socket(&mut self, rx_size: usize, tx_size: usize) -> SocketHandle {
        let rx_buf = tcp::SocketBuffer::new(alloc::vec![0; rx_size]);
        let tx_buf = tcp::SocketBuffer::new(alloc::vec![0; tx_size]);
        self.sockets.add(tcp::Socket::new(rx_buf, tx_buf))
    }

    /// Add a UDP socket to the stack and return its handle.
    ///
    /// `rx_size` and `tx_size` are the payload buffer sizes.  Each direction
    /// gets 8 metadata slots for queuing datagrams.
    pub fn add_udp_socket(&mut self, rx_size: usize, tx_size: usize) -> SocketHandle {
        let rx_buf = udp::PacketBuffer::new(
            alloc::vec![udp::PacketMetadata::EMPTY; 8],
            alloc::vec![0; rx_size],
        );
        let tx_buf = udp::PacketBuffer::new(
            alloc::vec![udp::PacketMetadata::EMPTY; 8],
            alloc::vec![0; tx_size],
        );
        self.sockets.add(udp::Socket::new(rx_buf, tx_buf))
    }

    /// Remove a socket by handle, releasing all its buffers.
    pub fn remove_socket(&mut self, handle: SocketHandle) {
        self.sockets.remove(handle);
    }
}

pub static TCPIP: Mutex<Option<TcpIpStack>> = Mutex::new(None);

pub fn init() {
    let stack = TcpIpStack::new();
    *TCPIP.lock() = Some(stack);
    crate::println!("[TCP/IP] Stack initialized (IP: 10.0.2.15, GW: 10.0.2.2)");
}

/// Poll the network stack (safe to call from shell loop, no scheduler lock needed)
pub fn poll_network() {
    if let Some(mut stack) = TCPIP.try_lock() {
        if let Some(s) = stack.as_mut() {
            s.poll();
        }
    }
}
