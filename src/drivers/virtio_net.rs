use spin::Mutex;

pub struct VirtioNet {
    pub io_base: u16,
}

impl VirtioNet {
    pub fn transmit(&mut self, _data: &[u8]) -> Result<(), ()> {
        Ok(())
    }
    pub fn receive(&mut self, _buf: &mut [u8]) -> Option<usize> {
        None
    }
    pub fn receive_to_phys(&mut self, _phys: u64, _len: usize) -> Option<usize> {
        None
    }
    pub fn transmit_from_phys(&mut self, _phys: u64, _len: usize) -> Result<(), ()> {
        Ok(())
    }
}

pub static VIRTIO_NET: Mutex<Option<VirtioNet>> = Mutex::new(None);
