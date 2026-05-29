use super::device::ZiqaDevice;
/// TCP/IP stack powered by smoltcp
use alloc::vec;
use smoltcp::iface::{Config, Interface, SocketSet};
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, HardwareAddress, IpAddress, IpCidr, Ipv4Address};
use spin::Mutex;

/// Network configuration
const IP_ADDR: Ipv4Address = Ipv4Address::new(10, 0, 2, 15);
const GATEWAY: Ipv4Address = Ipv4Address::new(10, 0, 2, 2);
const MAC: [u8; 6] = [0x52, 0x54, 0x12, 0x34, 0x56, 0x78];

pub struct TcpIpStack {
    pub iface: Interface,
    pub device: ZiqaDevice,
    pub sockets: SocketSet<'static>,
}

impl TcpIpStack {
    pub fn new() -> Self {
        let mut device = ZiqaDevice;

        let hw_addr = HardwareAddress::Ethernet(EthernetAddress(MAC));
        let config = Config::new(hw_addr);
        // Set random/default ports and keep packet buffers simple
        let mut iface = Interface::new(config, &mut device, Self::now());

        // Set IP address
        iface.update_ip_addrs(|addrs| {
            addrs.push(IpCidr::new(IpAddress::Ipv4(IP_ADDR), 24)).ok();
        });

        // Set default gateway
        iface.routes_mut().add_default_ipv4_route(GATEWAY).ok();

        let sockets = SocketSet::new(vec![]);

        Self {
            iface,
            device,
            sockets,
        }
    }

    pub fn now() -> Instant {
        let ms = crate::timer::uptime_ms();
        Instant::from_millis(ms as i64)
    }

    pub fn poll(&mut self) {
        let timestamp = Self::now();
        self.iface
            .poll(timestamp, &mut self.device, &mut self.sockets);
    }

    pub fn ip_addr(&self) -> Ipv4Address {
        IP_ADDR
    }

    pub fn gateway(&self) -> Ipv4Address {
        GATEWAY
    }

    pub fn mac(&self) -> [u8; 6] {
        MAC
    }
}

pub static TCPIP: Mutex<Option<TcpIpStack>> = Mutex::new(None);

pub fn init() {
    let stack = TcpIpStack::new();
    *TCPIP.lock() = Some(stack);
    crate::println!("[TCP/IP] Stack initialized (IP: 10.0.2.15, GW: 10.0.2.2)");
}
