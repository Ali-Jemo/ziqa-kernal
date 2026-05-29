/// smoltcp PHY adapter for ZiqaKernel's VirtIO-net driver
use smoltcp::phy::{self, Device, DeviceCapabilities, Medium};
use smoltcp::time::Instant;

pub struct ZiqaDevice;

pub struct ZiqaRxToken {
    buffer: [u8; 1500],
    len: usize,
}

pub struct ZiqaTxToken;

impl Device for ZiqaDevice {
    type RxToken<'a>
        = ZiqaRxToken
    where
        Self: 'a;
    type TxToken<'a>
        = ZiqaTxToken
    where
        Self: 'a;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        unsafe {
            #[allow(static_mut_refs)]
            if let Some(net) = &mut crate::drivers::virtio_net::VIRTIO_NET {
                if let Some((data, len)) = net.receive() {
                    return Some((ZiqaRxToken { buffer: data, len }, ZiqaTxToken));
                }
            }
        }
        None
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        Some(ZiqaTxToken)
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.medium = Medium::Ethernet;
        caps.max_transmission_unit = 1500;
        caps
    }
}

impl phy::RxToken for ZiqaRxToken {
    fn consume<R, F>(mut self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        crate::println!("[NET] smoltcp RX {} bytes", self.len);
        f(&mut self.buffer[..self.len])
    }
}

impl phy::TxToken for ZiqaTxToken {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buffer = [0u8; 1500];
        let result = f(&mut buffer[..len]);
        // Patch IP header checksum for IPv4 frames (EtherType 0x0800).
        // Ethernet header is 14 bytes; IP header starts at offset 14.
        // IHL field (low nibble of byte 14) gives header length in 32-bit words.
        if len >= 34 && buffer[12] == 0x08 && buffer[13] == 0x00 {
            let ihl = (buffer[14] & 0x0F) as usize * 4;
            if len >= 14 + ihl {
                // Zero the existing checksum field before recomputing.
                buffer[24] = 0;
                buffer[25] = 0;
                let cksum = crate::net::inet_checksum(&buffer[14..14 + ihl]);
                buffer[24] = (cksum >> 8) as u8;
                buffer[25] = (cksum & 0xFF) as u8;
            }
        }
        crate::println!("[NET] smoltcp TX {} bytes", len);
        unsafe {
            #[allow(static_mut_refs)]
            if let Some(net) = &mut crate::drivers::virtio_net::VIRTIO_NET {
                let _ = net.transmit(&buffer[..len]);
            }
        }
        result
    }
}
