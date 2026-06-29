# youclient

Ziqa OS GUI client shell: SHM, compositor IPC, input polling, UI frame setup, and demos.

## Public API

```zig
var client = youclient.Client.connect(320, 240, 352, 264) orelse return;
client.poll();
var ui = client.begin_ui();
ui.finish_focus();
client.flush(youclient.Rect.xywh(0, 0, client.width, client.height));
```

## IPC constants

- `ZIQA_SHM_CREATE = 1010`
- `ZIQA_SHM_ATTACH = 1011`
- `ZIQA_IPC_CREATE = 1020`
- `ZIQA_IPC_SEND = 1021`
- `ZIQA_IPC_RECV = 1022`
- `COMPOSITOR_CHAN = 3`

Opcodes match `src/ipc/gui.rs`: `Connect`, `CreateSurface`, `Flush`, `Input`, `BufferAttach`, `SetPosition`, `RegisterEventChannel`.

## Lifecycle

```text
SHM_CREATE -> SHM_ATTACH -> Connect -> CreateSurface -> BufferAttach -> IPC_CREATE -> RegisterEventChannel -> SetPosition -> poll/render/Flush
```

1. Create SHM large enough for `width * height * 4`.
2. Attach SHM and wrap it in `Canvas`.
3. Connect to compositor channel `3`.
4. Create a compositor surface.
5. Attach the SHM buffer.
6. Create an event channel.
7. Register the event channel for surface input.
8. Set the surface position.
9. Poll events, render through `youui`, and flush dirty rects.

## Input mapping

- `Input.kind == 1`: keyboard; sets `key`, `key_pressed`, and `activate` for Enter/Space.
- `Input.kind == 2`: mouse; sets position, button state, press edge, release edge.
- `Input.kind == 3`: resize; updates positive width/height.

## Demo

`demo_client.zig` is the Ziqa OS executable path. `demo_memory_only()` is only a hosted smoke check with a static framebuffer.

## Limitation

`ponytail: single client assumes first surface id=1; add CreateSurface reply when multiple external clients work`.

Later-safe extension: `CreateSurfaceReply { surface_id: u32 }` once the IPC layer supports request/reply. Do not renumber existing opcodes.
