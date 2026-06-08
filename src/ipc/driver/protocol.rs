use alloc::vec::Vec;

/// Standardized message types for IPC-based driver interaction
pub enum DriverRequest {
    Read { offset: usize, size: usize },
    Write { offset: usize, data: Vec<u8> },
    Ioctl { cmd: usize, arg: usize },
    GetMousePos,
    GetMouseBtn,
}

pub enum DriverResponse {
    Data(Vec<u8>),
    Status(i32),
    MousePos(i32, i32),
    MouseBtn(u8),
}

pub struct DriverMessage {
    pub req: DriverRequest,
    pub res: Option<DriverResponse>,
}
