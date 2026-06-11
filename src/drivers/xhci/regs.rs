//! xHCI register offsets and bit definitions (xHCI 1.x).

pub const CAP_CAPLENGTH: u64 = 0x00;
pub const CAP_HCIVERSION: u64 = 0x02;
pub const CAP_HCSPARAMS1: u64 = 0x04;
pub const CAP_HCCPARAMS1: u64 = 0x10;
pub const CAP_DBOFF: u64 = 0x14;
pub const CAP_RTSOFF: u64 = 0x18;

pub const OP_USBCMD: u64 = 0x00;
pub const OP_USBSTS: u64 = 0x04;
pub const OP_PAGESIZE: u64 = 0x28;
pub const OP_CRCR: u64 = 0x18;
pub const OP_DCBAAP: u64 = 0x30;
pub const OP_CONFIG: u64 = 0x38;
pub const OP_PORTSC_BASE: u64 = 0x400;
pub const OP_PORTSC_STRIDE: u64 = 0x10;

pub const CMD_RS: u32 = 1 << 0;
pub const CMD_HCRST: u32 = 1 << 1;
pub const STS_HCH: u32 = 1 << 0;
pub const STS_CNR: u32 = 1 << 11;
pub const CRCR_RCS: u64 = 1;

pub const PORT_CCS: u32 = 1 << 0;
pub const PORT_PED: u32 = 1 << 1;
pub const PORT_PR: u32 = 1 << 4;
pub const PORT_PP: u32 = 1 << 9;
pub const PORT_SPEED_SHIFT: u32 = 20;
pub const PORT_SPEED_MASK: u32 = 0xF << PORT_SPEED_SHIFT;
pub const PORT_CSC: u32 = 1 << 17;
pub const PORT_PRC: u32 = 1 << 21;
pub const PORT_WPR: u32 = 1 << 31;

pub const SPEED_FULL: u32 = 1;
pub const SPEED_LOW: u32 = 2;
pub const SPEED_HIGH: u32 = 3;
pub const SPEED_SUPER: u32 = 4;

pub const RT_IR0: u64 = 0x20;
pub const IR_ERSTSZ: u64 = 0x08;
pub const IR_ERSTBA: u64 = 0x10;
pub const IR_ERDP: u64 = 0x18;
pub const ERDP_EHB: u64 = 1 << 3;

pub const TRB_NORMAL: u32 = 1;
pub const TRB_SETUP: u32 = 2;
pub const TRB_DATA: u32 = 3;
pub const TRB_STATUS: u32 = 4;
pub const TRB_ENABLE_SLOT: u32 = 9;
pub const TRB_ADDR_DEV: u32 = 11;
pub const TRB_CONFIG_EP: u32 = 12;

pub const TRB_CYCLE: u32 = 1;
pub const TRB_IOC: u32 = 1 << 5;
pub const TRB_IDT: u32 = 1 << 6;
pub const TRB_DIR_IN: u32 = 1 << 16;
pub const TRB_TYPE_SHIFT: u32 = 10;

pub const CC_SUCCESS: u32 = 1;
pub const CC_SHORT_PACKET: u32 = 13;
pub const EVT_TRANSFER: u32 = 32;
pub const EVT_CMD_COMPLETION: u32 = 33;

pub const CTX_SIZE_32: usize = 32;
pub const CTX_SIZE_64: usize = 64;

pub const USB_CLASS_HID: u8 = 0x03;
pub const USB_PROTO_KEYBOARD: u8 = 1;
pub const USB_PROTO_MOUSE: u8 = 2;

pub const REQ_GET_DESCRIPTOR: u8 = 6;
pub const REQ_SET_CONFIGURATION: u8 = 9;
pub const DESC_DEVICE: u16 = 0x01;
pub const DESC_CONFIG: u16 = 0x02;


// USB class codes
pub const USB_CLASS_MASS_STORAGE: u8 = 0x08;
pub const USB_SUBCLASS_SCSI: u8 = 0x06;
pub const USB_PROTO_BOT: u8 = 0x50;

// Standard endpoint/descriptor types
pub const DESC_ENDPOINT: u16 = 0x05;
pub const DESC_INTERFACE: u16 = 0x04;
pub const EP_DIR_IN: u8 = 0x80;
pub const EP_TYPE_BULK: u8 = 2;

// Max command ring & EVT ring wait spins
pub const MAX_COMMAND_SPINS: u32 = 10_000_000;
pub const MAX_TRANSFER_SPINS: u32 = 50_000_000;

// USB hub class
pub const USB_CLASS_HUB: u8 = 0x09;
pub const DESC_HUB: u16 = 0x29;

// Standard requests used for hub port control
pub const REQ_SET_FEATURE: u8 = 3;
pub const REQ_GET_STATUS: u8 = 0;

pub const REQ_CLEAR_FEATURE: u8 = 1;

// Hub class-specific request bmRequestType
pub const HUB_REQ_GET_STATUS: u8 = 0xA3;
pub const HUB_REQ_SET_FEATURE: u8 = 0x23;
pub const HUB_REQ_CLEAR_FEATURE: u8 = 0x23;

// Port feature selectors (SET_FEATURE / CLEAR_FEATURE)
pub const PORT_FEAT_CONNECTION: u8 = 0;
pub const PORT_FEAT_ENABLE: u8 = 1;
pub const PORT_FEAT_RESET: u8 = 4;
pub const PORT_FEAT_POWER: u8 = 8;
pub const PORT_FEAT_C_CONNECTION: u8 = 16;
pub const PORT_FEAT_C_RESET: u8 = 20;

// Port status / change bit positions (GET_PORT_STATUS response, u16)
pub const PS_CURRENT_CONNECT: u16 = 1 << 0;
pub const PS_PORT_ENABLED: u16 = 1 << 1;
pub const PS_RESET: u16 = 1 << 4;
pub const PS_PORT_POWER: u16 = 1 << 8;
pub const PS_LOW_SPEED: u16 = 1 << 9;
pub const PS_HIGH_SPEED: u16 = 1 << 10;
pub const PS_C_CONNECT: u32 = 1 << 16;
pub const PS_C_RESET: u32 = 1 << 20;
