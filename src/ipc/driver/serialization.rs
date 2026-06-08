use crate::ipc::driver::protocol::{DriverRequest, DriverResponse};
use crate::ipc::Message;
use alloc::vec::Vec;

pub trait Serializable {
    fn serialize(&self) -> Vec<u8>;
    fn deserialize(data: &[u8]) -> Self;
}

// Simple manual serialization for demonstration as Ziqa prefers no heap/complex dependencies.
impl Serializable for DriverRequest {
    fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        match self {
            DriverRequest::Read { offset, size } => {
                buf.push(0);
                buf.extend_from_slice(&offset.to_le_bytes());
                buf.extend_from_slice(&size.to_le_bytes());
            }
            DriverRequest::Write { offset, data } => {
                buf.push(1);
                buf.extend_from_slice(&offset.to_le_bytes());
                buf.extend_from_slice(data);
            }
            DriverRequest::Ioctl { cmd, arg } => {
                buf.push(2);
                buf.extend_from_slice(&cmd.to_le_bytes());
                buf.extend_from_slice(&arg.to_le_bytes());
            }
            DriverRequest::GetMousePos => buf.push(3),
            DriverRequest::GetMouseBtn => buf.push(4),
        }
        buf
    }

    fn deserialize(data: &[u8]) -> Self {
        match data[0] {
            0 => DriverRequest::Read {
                offset: usize::from_le_bytes(data[1..9].try_into().unwrap()),
                size: usize::from_le_bytes(data[9..17].try_into().unwrap()),
            },
            // ... add remaining cases
            3 => DriverRequest::GetMousePos,
            4 => DriverRequest::GetMouseBtn,
            _ => panic!("Unknown request type"),
        }
    }
}
