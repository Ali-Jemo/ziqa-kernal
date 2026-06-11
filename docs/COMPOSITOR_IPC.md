# Compositor IPC Protocol

The compositor manages graphical surfaces for userspace clients over an IPC channel (well-known channel ID 3).

## Message Format

Messages are sent as raw bytes: `[opcode_byte][payload_bytes]`

## OpCodes

|OpCode|Payload struct|Description|
|---|---|---|
|1|`ConnectMsg { pid: u64 }`|Client connects to compositor.|
|2|`CreateSurfaceMsg { width: u32, height: u32 }`|Request surface creation.|
|3|`FlushMsg { surface_id: u32, x, y, width, height }`|Mark surface dirty region for repaint.|
|4|`InputMsg { kind: u8, code: u32, x: u32, y: u32 }`|Input event forwarded to client (event channel 4).|
|5|`BufferAttachMsg { surface_id: u32, shm_id: u32, width, height }`|Attach SHM buffer to surface.|
|6|`SetPositionMsg { surface_id: u32, x: i32, y: i32 }`|Set surface position on screen.|

## Input Forwarding (Channel 4)

The compositor broadcasts input events to channel ID 4. Clients can poll this channel to receive keyboard (`kind=1`) and mouse (`kind=2`) events.
