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
        unsafe {
            #[allow(static_mut_refs)]
            if let Some(net) = &mut crate::drivers::virtio_net::VIRTIO_NET {
                let _ = net.transmit(&buffer[..len]);
            }
        }
        result
    }
}
