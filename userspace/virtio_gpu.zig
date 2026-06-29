//! VirtIO GPU protocol definitions and userspace driver helpers.
//!
//! Ref: VirtIO 1.1 §5.7 (GPU Device)

const std = @import("std");

pub const VIRTIO_GPU_F_VIRGL: u32 = 0;
pub const VIRTIO_GPU_F_EDID: u32 = 1;

pub const CtrlType = enum(u32) {
    // Commands
    GET_DISPLAY_INFO = 0x0100,
    RESOURCE_CREATE_2D = 0x0101,
    RESOURCE_UNREF = 0x0102,
    SET_SCANOUT = 0x0103,
    RESOURCE_FLUSH = 0x0104,
    TRANSFER_TO_HOST_2D = 0x0105,
    RESOURCE_ATTACH_BACKING = 0x0106,
    RESOURCE_DETACH_BACKING = 0x0107,
    GET_CAPSET_INFO = 0x0108,
    GET_CAPSET = 0x0109,
    GET_EDID = 0x010c,

    // Cursor commands
    UPDATE_CURSOR = 0x0300,
    MOVE_CURSOR = 0x0301,

    // Success responses
    OK_NODATA = 0x1100,
    OK_DISPLAY_INFO = 0x1101,
    OK_CAPSET_INFO = 0x1102,
    OK_CAPSET = 0x1103,
    OK_EDID = 0x1104,

    // Error responses
    ERR_UNSPEC = 0x1200,
    ERR_OUT_OF_MEMORY = 0x1201,
    ERR_INVALID_SCANOUT = 0x1202,
    ERR_INVALID_RESOURCE = 0x1203,
    ERR_INVALID_CONTEXT = 0x1204,
    ERR_INVALID_PARAMETER = 0x1205,
};

pub const CtrlHeader = extern struct {
    type: u32,
    flags: u32,
    fence_id: u64,
    ctx_id: u32,
    padding: u32,
};

pub const Rect = extern struct {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
};

pub const GetDisplayInfo = extern struct {
    hdr: CtrlHeader,
};

pub const DisplayInfo = extern struct {
    hdr: CtrlHeader,
    pmodes: [16]extern struct {
        r: Rect,
        enabled: u32,
        flags: u32,
    },
};

pub const ResourceCreate2d = extern struct {
    hdr: CtrlHeader,
    resource_id: u32,
    format: u32,
    width: u32,
    height: u32,
};

pub const ResourceAttachBacking = extern struct {
    hdr: CtrlHeader,
    resource_id: u32,
    nr_entries: u32,
};

pub const MemEntry = extern struct {
    addr: u64,
    length: u32,
    padding: u32,
};

pub const SetScanout = extern struct {
    hdr: CtrlHeader,
    r: Rect,
    scanout_id: u32,
    resource_id: u32,
};

pub const ResourceFlush = extern struct {
    hdr: CtrlHeader,
    r: Rect,
    resource_id: u32,
    padding: u32,
};

pub const TransferToHost2d = extern struct {
    hdr: CtrlHeader,
    r: Rect,
    offset: u64,
    resource_id: u32,
    padding: u32,
};

pub const Format = enum(u32) {
    B8G8R8A8_UNORM = 1,
    B8G8R8X8_UNORM = 2,
    A8R8G8B8_UNORM = 3,
    X8R8G8B8_UNORM = 4,
    R8G8B8A8_UNORM = 67,
    X8B8G8R8_UNORM = 68,
    A8B8G8R8_UNORM = 121,
    R8G8B8X8_UNORM = 134,
};
