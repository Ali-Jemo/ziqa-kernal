# You GUI

Ziqa OS GUI is three boring layers. Keep the split permanent: hot drawing stays Zig, OS integration stays in the client shell, and widgets stay immediate-mode.

```text
app code -> youclient -> youui -> youcanvas -> XRGB8888/ARGB8888 framebuffer
```

## Layer contract

### `youcanvas`

- No heap allocation.
- No app state.
- Raw `u32` XRGB8888/ARGB8888 pixel primitives.
- Direct drawing API: `Canvas.fill`, `Canvas.rect`, `Canvas.border`, `Canvas.line`.
- Draw-command buffer API: `DrawList` stores caller-owned commands and renders them in order.
- C ABI exports live in `youcanvas-ffi.zig` for Rust and foreign callers.

### `youui`

- Immediate-mode widgets only.
- Owns layout, focus traversal, input edge interpretation, and semantic/accessibility nodes.
- Keeps persistent `UIState`; it does not retain a widget tree.
- No compositor, syscall, SHM, or kernel IPC knowledge.

### `youclient`

- Ziqa OS client shell.
- Owns SHM allocation, compositor IPC, input polling, frame begin/end, and demo apps.
- Bridges `youui` to the compositor protocol without leaking syscall details into widgets.

## Minimal app

```zig
const youcanvas = @import("youcanvas");
const youclient = @import("youclient");
const widgets = @import("youui").widgets;

pub fn main() void {
    var client = youclient.Client.connect(320, 240, 352, 264) orelse return;

    while (true) {
        client.poll();
        var ui = client.begin_ui();
        ui.canvas.fill(youcanvas.Sumerian.bg_card);
        ui.panel(youcanvas.Rect.xywh(16, 16, 288, 96), .Column, youcanvas.Sumerian.bg_elevated);
        _ = widgets.button_next(&ui, 1, 120, 32, youcanvas.Sumerian.bg_hover, youcanvas.Sumerian.gold_glow);
        ui.pop_panel();
        ui.finish_focus();
        client.flush(youcanvas.Rect.xywh(0, 0, client.width, client.height));
    }
}
```

## Retained data allowed

Only these structures persist across frames:

- draw commands supplied by the caller
- `UIState` focus/hot/active state
- focus traversal state for the current frame
- semantic accessibility nodes supplied by the caller

Skipped: retained widgets, theme objects, complex layout solvers, GPU backends. Add them only when a real app needs them.
