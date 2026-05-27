/// ZiqaKernel Network Stack Stub
///
/// Provides a minimal packet-oriented network abstraction with:
/// - A `NetDevice` trait for network interfaces
/// - A loopback device (lo) that echoes packets back
/// - A simple packet queue per device
/// - A global `NET` registry

use spin::Mutex;

pub const MTU: usize = 1500;
const QUEUE_CAP: usize = 16;
const MAX_DEVICES: usize = 4;

/// A raw network packet (Ethernet frame or raw IP)
#[derive(Clone, Copy)]
pub struct Packet {
    pub data: [u8; MTU],
    pub len: usize,
}

impl Packet {
    pub fn new(src: &[u8]) -> Self {
        let len = src.len().min(MTU);
        let mut data = [0u8; MTU];
        data[..len].copy_from_slice(&src[..len]);
        Self { data, len }
    }
}

/// Errors from the network layer
#[derive(Debug)]
pub enum NetError {
    QueueFull,
    QueueEmpty,
    NoSuchDevice,
    DeviceFull,
}

/// A fixed-capacity packet ring queue
struct PacketQueue {
    ring: [Option<Packet>; QUEUE_CAP],
    head: usize,
    tail: usize,
    count: usize,
}

const NONE_PKT: Option<Packet> = None;

impl PacketQueue {
    const fn new() -> Self {
        Self { ring: [NONE_PKT; QUEUE_CAP], head: 0, tail: 0, count: 0 }
    }

    fn push(&mut self, pkt: Packet) -> Result<(), NetError> {
        if self.count >= QUEUE_CAP { return Err(NetError::QueueFull); }
        self.ring[self.tail] = Some(pkt);
        self.tail = (self.tail + 1) % QUEUE_CAP;
        self.count += 1;
        Ok(())
    }

    fn pop(&mut self) -> Result<Packet, NetError> {
        if self.count == 0 { return Err(NetError::QueueEmpty); }
        let pkt = self.ring[self.head].take().unwrap();
        self.head = (self.head + 1) % QUEUE_CAP;
        self.count -= 1;
        Ok(pkt)
    }

    fn len(&self) -> usize { self.count }
}

/// A virtual network device
pub struct NetDevice {
    pub name: &'static str,
    pub mac: [u8; 6],
    pub is_loopback: bool,
    tx_queue: PacketQueue,
    rx_queue: PacketQueue,
    pub tx_packets: u64,
    pub rx_packets: u64,
    pub tx_bytes: u64,
    pub rx_bytes: u64,
}

impl NetDevice {
    pub const fn loopback() -> Self {
        Self {
            name: "lo",
            mac: [0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            is_loopback: true,
            tx_queue: PacketQueue::new(),
            rx_queue: PacketQueue::new(),
            tx_packets: 0,
            rx_packets: 0,
            tx_bytes: 0,
            rx_bytes: 0,
        }
    }

    /// Transmit a packet. For loopback, immediately enqueues to rx.
    pub fn transmit(&mut self, data: &[u8]) -> Result<(), NetError> {
        let pkt = Packet::new(data);
        self.tx_packets += 1;
        self.tx_bytes += pkt.len as u64;
        if self.is_loopback {
            // Loopback: echo straight to rx
            self.rx_queue.push(pkt)?;
            self.rx_packets += 1;
            self.rx_bytes += data.len().min(MTU) as u64;
        } else {
            self.tx_queue.push(pkt)?;
        }
        Ok(())
    }

    /// Receive the next packet from the rx queue.
    pub fn receive(&mut self) -> Result<Packet, NetError> {
        self.rx_queue.pop()
    }

    /// Number of packets waiting in rx queue
    pub fn rx_pending(&self) -> usize { self.rx_queue.len() }
}

/// Global network device registry
pub struct NetStack {
    devices: [Option<NetDevice>; MAX_DEVICES],
    count: usize,
}

impl NetStack {
    pub const fn new() -> Self {
        const NONE_DEV: Option<NetDevice> = None;
        Self { devices: [NONE_DEV; MAX_DEVICES], count: 0 }
    }

    pub fn add_device(&mut self, dev: NetDevice) -> bool {
        for slot in self.devices.iter_mut() {
            if slot.is_none() {
                *slot = Some(dev);
                self.count += 1;
                return true;
            }
        }
        false
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut NetDevice> {
        self.devices.iter_mut()
            .filter_map(|s| s.as_mut())
            .find(|d| d.name == name)
    }

    pub fn device_count(&self) -> usize { self.count }

    pub fn print_stats(&self) {
        for slot in self.devices.iter() {
            if let Some(dev) = slot {
                crate::println!("  {:4}  tx={} rx={} tx_bytes={} rx_bytes={}",
                    dev.name, dev.tx_packets, dev.rx_packets, dev.tx_bytes, dev.rx_bytes);
            }
        }
    }
}

pub static NET: Mutex<NetStack> = Mutex::new(NetStack::new());

/// Initialize the network stack (registers loopback device)
pub fn init() {
    NET.lock().add_device(NetDevice::loopback());
    crate::println!("[NET] Network stack initialized (lo registered)");
}
