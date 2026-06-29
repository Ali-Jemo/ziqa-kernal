# youcanvas

No-heap framebuffer drawing for Ziqa OS.

## Types

- `Canvas { ptr, width, height }`: borrowed `[*]u32` framebuffer.
- `Rect.xywh(x, y, w, h)`: integer rectangle.
- `Color`, `Theme`, `Sumerian`: palette helpers.
- `DrawList`: caller-owned command buffer.

## Color format

Pixels are `u32`. The software path treats them as XRGB8888/ARGB8888: red in bits 16..23, green in 8..15, blue in 0..7. Alpha blend helpers write opaque alpha.

## Direct framebuffer API

```zig
var pixels = [_]u32{0} ** (320 * 240);
const canvas = youcanvas.Canvas.init(&pixels, 320, 240);
canvas.fill(youcanvas.Sumerian.bg_card);
canvas.rect(youcanvas.Rect.xywh(8, 8, 64, 24), youcanvas.Sumerian.bg_hover);
canvas.border(youcanvas.Rect.xywh(8, 8, 64, 24), youcanvas.Sumerian.gold_glow, 1);
canvas.line(8, 40, 80, 60, youcanvas.Sumerian.text_hi);
```

## Draw lists

```zig
var storage: [8]youcanvas.DrawCommand = undefined;
var list = youcanvas.DrawList.init(&storage);
_ = list.push(.{ .clear = youcanvas.Sumerian.bg_card });
_ = list.push(.{ .rect = .{ .rect = youcanvas.Rect.xywh(8, 8, 64, 24), .color = youcanvas.Sumerian.bg_hover } });
_ = list.push(.{ .text = .{ .x = 12, .y = 12, .text = "OK", .color = youcanvas.Sumerian.text_hi } });
list.render(canvas);
```

`push` returns `false` when storage is full. It never allocates and never panics.

## Clipping

Rectangles, borders, lines, and text clip to the canvas. Empty rectangles draw nothing. Negative origins are accepted.

## Text

Built-in text is boot/developer ASCII only: a fixed 5x7 bitmap with one-pixel horizontal padding. Unknown bytes draw `?`. Full text shaping should use HarfBuzz for shaping and FreeType/Skrifa for rasterization when userspace dependency support exists. Do not put those dependencies in kernel hot paths.

## C ABI

`youcanvas-ffi.zig` exports `yc_fill_rect`, `yc_fill_rect_alpha`, `yc_draw_border`, `yc_draw_rounded_rect`, `yc_draw_line`, and `yc_clear` for Rust/foreign callers.
