# youui

Immediate-mode widgets and layout for Ziqa OS. No compositor calls. No retained widget tree.

## Frame rules

- Call `UI.begin(canvas, input, &state)` once per frame.
- Use stable `u64` IDs for focusable widgets.
- Call widgets in the same order every frame when possible.
- Call `ui.finish_focus()` after the last focusable widget.
- Keep `UIState` between frames; `UI` itself is per-frame.

```zig
var ui_state = youui.UIState{};
var ui = youui.UI.begin(canvas, input, &ui_state);
ui.panel(Rect.xywh(16, 16, 160, 96), .Column, Sumerian.bg_elevated);
_ = youui.widgets.button_next(&ui, 1, 120, 24, Sumerian.bg_hover, Sumerian.gold_glow);
ui.pop_panel();
ui.finish_focus();
```

## Input edges

`left_pressed`, `left_released`, `key_pressed`, `focus_next`, `focus_prev`, and `activate` are edge fields supplied by `youclient` or tests. `focus_next` and `focus_prev` move through registered focusable widgets and wrap at the ends.

## Focus

`ui.focusable(id)` registers traversal. If no widget has focus, the first focusable widget gets it. `focus_next` moves to the next focusable widget and wraps in `finish_focus()`.

## Widgets

```zig
_ = youui.widgets.button(&ui, 1, Rect.xywh(8, 8, 80, 24), Sumerian.bg_hover, Sumerian.gold_glow);

var checked = false;
_ = youui.widgets.checkbox(&ui, 2, Rect.xywh(8, 40, 16, 16), &checked);

var value: u32 = 50;
_ = youui.widgets.slider_u32(&ui, 3, Rect.xywh(8, 64, 120, 16), &value, 0, 100);

youui.widgets.label(&ui, Rect.xywh(8, 88, 120, 8), "Status", Sumerian.text_hi);
```

`*_next` variants allocate a rectangle from the active panel layout.

## Semantics

Optional semantic data is pushed into a caller-owned `semantics.Tree`:

```zig
var nodes: [16]youui.semantics.Node = undefined;
var tree = youui.semantics.Tree.init(&nodes);
ui.set_semantics(&tree);
```

Widgets keep rendering if the semantic slice fills. This is the stable seam for later AccessKit/AT-SPI adapters.
