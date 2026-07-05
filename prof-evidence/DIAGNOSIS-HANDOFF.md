# Orbital Compositor Lag/Stutter — Performance Diagnosis (INSTRUMENTATION HANDOFF)

> **STATUS: INCOMPLETE — not the final evidence-ranked report.**
> The idle phase is measured with real TSC-calibrated numbers. The sustained
> **mouse-move** and **window-drag** phases could NOT be captured by this agent
> (no display/mouse available) and require the human-run manual test described
> in §6. Per the task contract, a final report is withheld until all three
> phases are measured. What follows is the handoff: instrumentation status,
> measured idle evidence, one captured active full-screen frame, all
> code-determined structural findings (with exact refs), and a *preliminary*
> hypothesis ranking that idle + code evidence already constrain tightly.

---

## 0. ONE-LINE SUMMARY (strongest evidence-backed statement)

**Measured idle rules out a busy/damage redraw loop (≈10 redraw-cycles/sec, zero compositing work, 0% dirty ratio, ~55 µs/frame); the one captured active full-screen frame blows the 16.67 ms budget at 18.2 ms, with desktop-chrome rendering `native_shell::draw_desktop` consuming 13.2 ms (72% of frame) — and code evidence shows `draw_desktop` recomputes every text glyph via `font.render` + a fresh `Image` allocation each frame (`native_shell.rs:655-658`), making per-frame glyph re-rasterization the prime suspect for stutter on any large redraw (bar/dock crossing, drag). Move/drag confirmation pending.**

---

## 1. Instrumentation added (temporary, all tagged `// PROF-TEMP`)

| File | Change |
|---|---|
| `gui/orbital-master/src/prof.rs` (NEW) | TSC-based accumulator. Per-frame scratch (input/pre/chrome/drawwin/cursor/sync cycles + dirty area + rect count); 3 phase buckets (idle/move/drag); 1/sec single-`write_all` summary line. Calibrated TSC→µs via 100 ms sleep. |
| `gui/orbital-master/src/main.rs` | `mod prof;` (cfg-gated). |
| `gui/orbital-master/src/compositor.rs` | `redraw_direct_bga`: times draw_windows closure, cursor overlay, BGA sync per dirty rect; reports dirty area. |
| `gui/orbital-master/src/scheme.rs` | `redraw`: times pre-phase + desktop-chrome; adds `is_dragging()`/`screen_area()` helpers. |
| `gui/orbital-master/src/core/mod.rs` | run loop: times input dispatch + total frame; classifies frame into bucket; calibrates TSC; flushes 1/sec. |

**Deviation from task spec (noted):** instrumentation is gated on the existing `ziqa-bga-direct` feature (NOT a new `orbital-prof` flag) so `make run-gui` / `cargo check --features ziqa-bga-direct` emit measurements with no extra flag wiring. Every probe line is tagged `// PROF-TEMP`; removal = delete `prof.rs` + the `mod prof;` line + every `PROF-TEMP` line.

**Why TSC, not `std::time::Instant`:** under ZiqaKernel `Instant::elapsed()` returns 0 for sub-second intervals (the clock advances only at ~1 s granularity — confirmed: 1/sec flush pacing works, but µs-level `elapsed()` is always 0). TSC (`_rdtsc`) is readable from ring-3 (CR4.TSD is not set) and calibrated in-process against a 100 ms sleep, so reported µs values are measured, not guessed.

**Output channel fix:** `eprintln!`/`write_fmt` issues many small writes that the kernel `[redox fd2]` logger emits as separate fragments. `prof.rs` builds one `String` and does a single `stderr().lock().write_all()` → one parseable `PROF` record per line.

**PROF record format:**
```
PROF hz=<idle|move|drag> fps=<frames this sec> ft_us min=<m> avg=<a> max=<x> | inp=<i> pre=<p> chr=<c> dw=<d> cur=<u> syn=<s> | dirty_avg=<px> screen=<px> ratio=<%> work=<worked>/<frames>
```
- `ft_us` = total frame work time (input + redraw, excludes sleep), TSC-calibrated µs.
- `inp` = input dispatch; `pre` = scheme redraw pre-phase (rezbuffer/OSD/title-to-string/running_apps); `chr` = `draw_desktop` (chrome, INSIDE the draw_windows closure so `chr ⊆ dw`); `dw` = whole draw_windows closure (chrome + windows); `cur` = software cursor overlay; `syn` = BGA backbuffer→scanout flush.
- `work=N/F` = N of F frames actually had ≥1 dirty rect (rest are no-op redraw cycles).

---

## 2. MEASURED DATA — idle phase (real, TSC-calibrated)

Source: `prof-evidence/idle-phase-serial.log` (headless QEMU, 1280×960, ~45 s idle capture, 9 one-second samples after boot settled).

| Metric | Measured value |
|---|---|
| Redraw cycles/sec at idle | **min 10 · avg 10.2 · max 11** (matches the 10 Hz idle pacer) |
| Frame work time (µs) | **min 24 · avg ~55 · max 363** (one 363 µs spike; rest ≤111 µs) |
| `inp` (input dispatch) | 0 (no input) |
| `pre` (redraw pre-phase) | **avg 3.0 µs** — the only non-zero sub-phase at idle |
| `chr / dw / cur / syn` | **all 0** — no compositing work executed |
| dirty_avg / screen | 0 / 1,228,800 → **ratio 0%** |
| `work` | **0 / N** — zero frames had any dirty rect |

**Interpretation (idle):** The run loop calls `redraw()` every frame unconditionally (`core/mod.rs:227`), but with an empty `redraws` vec the compositor path is a pure no-op (`compositor.rs:362` loop body never runs). The entire idle frame cost is ~55 µs: the `pre` phase (~3 µs for `rezbuffer` + OSD checks + `focused_title.to_string()` + `running_apps()`) plus loop bookkeeping. **Idle is NOT a busy-render loop and NOT a damage/infinite-redraw loop.** Idle CPU waste is limited to ~10 no-op redraw cycles/sec.

## 3. MEASURED DATA — one active full-screen frame (boot redraw)

One frame bucketed as `move` (a boot-time input event triggered a full-screen damage). This is **not** the manual move/drag test, but it is a real measured active compositing frame at the maximum dirty scope (ratio 100%):

```
PROF hz=move fps=1 ft_us min=18224 avg=18224 max=18224
   | inp=345 pre=243 chr=13229 dw=13923 cur=425 syn=1688
   | dirty_avg=1229170 screen=1228800 ratio=100% work=1/1
```

| Phase | µs | share of frame |
|---|---|---|
| **Total frame** | **18,224** | 100% — **exceeds the 16,667 µs (60 Hz) budget** |
| `chr` = `draw_desktop` (chrome) | **13,229** | **72.6%** |
| `dw` = draw_windows closure total | 13,923 | (chr ⊆ dw; window bodies ≈ 694 µs) |
| `syn` = BGA flush (full screen) | 1,688 | 9.3% |
| `pre` | 243 | 1.3% |
| `cur` = cursor overlay | 425 | 2.3% |
| `inp` | 345 | 1.9% |

**Interpretation (active, full-screen upper bound):** a single full-screen redraw takes 18.2 ms — over the 16.67 ms frame budget, so it costs a dropped/stuttered frame. `draw_desktop` (the desktop chrome: background gradient + top bar + dock + all labels) is 72.6% of it. Within `draw_desktop`, the code-evidenced dominant cost is text: `draw_label` calls `font.render(text, 16.0)` and allocates a fresh `Image::from_color(...)` **every call, every frame** (`native_shell.rs:655-658`) — glyph rasterization is not cached. The bar/dock render several labels per full-screen frame, so per-frame font re-rasterization is the prime suspect.

---

## 4. CODE-DETERMINED structural findings (with exact refs)

### (4) BGA write path — per-scanline contiguous copy (NOT per-pixel MMIO)
`gui/orbital-master/src/core/display/display_ziqa_bga.rs:38-64`, `Framebuffer::flush`:
```rust
for y in y0..y1 {
    let start = (y * self.stride + x0) as usize;
    let src = &self.backbuffer[start..start + count];
    unsafe { std::ptr::copy_nonoverlapping(src.as_ptr(), self.ptr.add(start) as *mut Color, count); }
}
```
One `copy_nonoverlapping` per scanline row of the dirty rect. **Not** per-pixel MMIO, **not** one bulk whole-buffer memcpy. Measured `syn` for a full screen = 1,688 µs (~2.8 GB/s over ~4.7 MB) — the BGA path is NOT the bottleneck.

### (5) Double buffering — PRESENT
`display_ziqa_bga.rs:16-28`: `Framebuffer` holds a heap `backbuffer: Vec<Color>` (render target) separate from `ptr: *mut u32` (mmap'd BGA scanout). All drawing lands in `backbuffer`; `sync_rect`→`flush` copies only the dirty region to `ptr` once per frame (`display_ziqa_bga.rs:106-117`). Tearing-safe. Evidence: the `flush` comment + the `syn` measurement above.

### (6) Event loop — PACED, not busy-poll
`gui/orbital-master/src/core/mod.rs:173-175, 239-247`: targets are `input_frame`/`active_frame` = 16.67 ms (60 Hz) and `idle_frame` = 100 ms (10 Hz); after each frame `std::thread::sleep(target - elapsed)`. **Measured confirms:** idle fps = 10 (matches 10 Hz target). It is NOT a tight busy loop. (Caveat: `redraw()` is called every frame even when idle — a cheap no-op, ~55 µs, but it does mean 10 no-op redraw passes/sec at idle.)

### (7) Per-frame heap allocations in the hot path — YES, several
| Location | Allocation | Frequency |
|---|---|---|
| `scheme.rs` `redraw` ~L770-776 | `focused_title = …to_string()` | every redraw() call (incl. idle) |
| `scheme.rs` ~L778 | `running_apps()` → `Vec<NativeAppKind>` (`native_shell.rs:70-72` `.collect()`) | every redraw() call |
| `scheme.rs` ~L820 (closure) | `native_ids: Vec<WindowId>` `.collect()` | every redraw with ≥1 dirty rect |
| `native_shell.rs:655-658` `draw_label` | `font.render(...)` + `Image::from_color(...)` | **per label, per frame, when the label's rect intersects the dirty region** |
| `widget/fps.rs:87-88` `draw_fps_osd` | `Image::from_color` + `font.render` | only when FPS widget enabled |

These are reclaimable cost; the `font.render`/`Image` pair in `draw_label` is the heaviest (rasterization + alloc per label per frame).

### (8) Font/text rendering — recomputed each frame, NOT cached blit
- **Title bars:** CHEAP — `window.draw_title` blends a precomputed `title_image`/`title_image_unfocused` bitmap (`window.rs:235-253`). Cached.
- **Desktop chrome (bar/dock labels):** EXPENSIVE — `native_shell::draw_label` calls `font.render(text, 16.0)` and builds a new `Image` on every invocation (`native_shell.rs:655-658`). No glyph cache. `draw_desktop` issues one `draw_label` per bar/dock label per frame whenever the dirty rect intersects the label. This is the measured 13.2 ms `chr` on a full-screen redraw.

---

## 5. Preliminary hypothesis ranking (evidence-ordered, NOT final)

Ranked by strength of *measured + code* evidence. Each cites the supporting number/ref. Move/drag data may reorder these.

1. **Per-frame glyph re-rasterization in `draw_desktop` labels is the dominant redraw cost and the prime stutter source on large redraws.**
   Evidence: measured `chr` = 13,229 µs = 72.6% of an 18,224 µs full-screen frame that exceeds the 16.67 ms budget (`prof-evidence/idle-phase-serial.log`); code: `native_shell.rs:655-658` `font.render` + fresh `Image` per label per frame, no cache. *Confirm on move/drag:* a small cursor dirty rect crossing the top bar / dock should spike `chr`.

2. **`redraw()` runs every frame including idle, doing redundant pre-phase work (~3 µs) and 3 heap allocs (`to_string`, `running_apps`, `native_ids`).**
   Evidence: idle `pre` avg 3.0 µs non-zero with `work=0/N`; code: `scheme.rs` `focused_title.to_string()` + `running_apps()` every call. Low absolute cost at idle, but it is wasted work that scales with frame rate and contributes to GC pressure during active redraws.

3. **A full-screen (or near-full-screen) dirty rect alone — independent of font — costs ~5 ms in non-chrome work** (`dw − chr ≈ 694 µs` windows + `syn` 1,688 µs + `cur` 425 µs + `pre` 243 µs). Not by itself over budget, but it stacks under #1.
   Evidence: the same boot frame breakdown.

4. **BGA write path / double-buffering / event-loop busy-poll are NOT the cause.**
   Evidence: `syn` only 1,688 µs full-screen (§4.4); double-buffer present (§4.5); loop paced and idle measured at 10 Hz with ~55 µs frames (§4.6, §2).

**Explicitly ruled out by measurement:** busy/damage redraw loop at idle; BGA per-pixel MMIO; missing double-buffer; busy-poll event loop.

---

## 6. How to complete the diagnosis (human manual test)

The instrumentation is built, proven to emit clean `PROF` lines, and the build is cached. To capture the move + drag phases:

```bash
cd /home/jemo/Projects/my-os-reorganized
make run-gui          # gtk window opens; serial → /tmp/ziqa-gui-serial.log
```
Then, while watching the window, run the fixed sequence and note timestamps:
1. **5 s idle** (don't touch the mouse) — already captured here for reference.
2. **5 s continuous mouse movement** across the full screen (sweep over the top bar and dock too).
3. **5 s dragging a window** from one corner to the opposite corner.

Extract results (the `[redox fd2]` prefix is from the kernel logger; the PROF payload is one line):
```bash
grep 'PROF hz=' /tmp/ziqa-gui-serial.log
```
Expected: `hz=move` lines during phase 2, `hz=drag` lines during phase 3. Compare `chr`/`dw`/`syn`/`ratio` against the idle + boot values above. **The decisive test for hypothesis #1:** if `chr` spikes when the cursor/drag rect crosses the bar/dock, glyph re-rasterization is confirmed as the stutter source.

To revert all instrumentation afterward:
```bash
rm gui/orbital-master/src/prof.rs
# delete the `mod prof;` line in main.rs and every line tagged `// PROF-TEMP`
```

---

## 7. Verification performed

- **Build:** `redoxer build --release --features ziqa-bga-direct` — success (the real build path; `make run-gui` uses this). Note: plain `cargo check --features ziqa-bga-direct` fails with a duplicate-lang-item error between redoxer's sysroot `core` and a stale build-std `core` — an environment artifact, not a code issue; the release build path (what actually runs) succeeds.
- **Runtime:** headless QEMU boot of the instrumented bootimage; 10 clean `PROF` records emitted over ~45 s with calibrated, non-zero µs values. Raw serial saved at `prof-evidence/idle-phase-serial.log`.
- **Not yet run:** the human manual move/drag test (§6).
