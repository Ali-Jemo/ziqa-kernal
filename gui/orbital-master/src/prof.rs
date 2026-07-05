#![cfg(feature = "ziqa-bga-direct")]
// PROF-TEMP: temporary performance instrumentation for the orbital lag/stutter
// diagnosis. Everything in this file is diagnostic-only and is compiled out
// unless the `ziqa-bga-direct` feature is on. To revert the instrumentation,
// delete this file plus every line tagged `// PROF-TEMP` in compositor.rs,
// scheme.rs, core/mod.rs and the `mod prof;` line in main.rs.
//
// Design: orbital is single-threaded, so a thread_local accumulator is
// contention-free. Per-frame sub-phase costs are measured with the CPU TSC
// (core::arch::x86_64::_rdtsc) because std::time::Instant on ZiqaKernel only
// advances at ~1 s granularity (sub-second .elapsed() is 0). TSC is calibrated
// once against a 100 ms sleep so the report can show measured microseconds.
// One summary line per active phase bucket is emitted per second via a SINGLE
// write_all (write_fmt/eprintln fragments under the kernel [redox fd2] logger).

use std::cell::RefCell;
use std::io::Write;
use std::time::Instant;

/// Read the timestamp counter (cycles). Safe from ring-3 on this kernel
/// (CR4.TSD is not set). Used for high-resolution phase deltas.
pub fn tsc() -> u64 {
    unsafe { core::arch::x86_64::_rdtsc() }
}

/// Cycles elapsed since `t0`.
pub fn since(t0: u64) -> u64 {
    tsc().saturating_sub(t0)
}

/// Per-frame scratch (values are TSC CYCLES), reset after each commit.
#[derive(Clone, Copy, Default)]
pub struct Sub {
    pub input_cyc: u64,
    pub pre_cyc: u64,
    pub chrome_cyc: u64,
    pub drawwin_cyc: u64,
    pub cursor_cyc: u64,
    pub sync_cyc: u64,
    pub dirty_area: u64,
    pub rect_count: u32,
    pub had_work: bool, // true if the compositor processed >= 1 dirty rect
}

/// Per-phase accumulator (idle=0, move=1, drag=2). Values are TSC CYCLES.
#[derive(Clone, Copy, Default)]
pub struct Bucket {
    pub frames: u64,
    pub ft_min_cyc: u64,
    pub ft_max_cyc: u64,
    pub ft_sum_cyc: u64,
    pub input_sum_cyc: u64,
    pub pre_sum_cyc: u64,
    pub chrome_sum_cyc: u64,
    pub drawwin_sum_cyc: u64,
    pub cursor_sum_cyc: u64,
    pub sync_sum_cyc: u64,
    pub dirty_sum: u64,   // sum of dirty_area over worked frames
    pub work_frames: u64, // frames that actually had >= 1 dirty rect
}

impl Bucket {
    fn add_frame(&mut self, total_cyc: u64, sub: &Sub) {
        self.frames += 1;
        self.ft_sum_cyc += total_cyc;
        if self.frames == 1 || total_cyc < self.ft_min_cyc {
            self.ft_min_cyc = total_cyc;
        }
        if total_cyc > self.ft_max_cyc {
            self.ft_max_cyc = total_cyc;
        }
        self.input_sum_cyc += sub.input_cyc;
        self.pre_sum_cyc += sub.pre_cyc;
        self.chrome_sum_cyc += sub.chrome_cyc;
        self.drawwin_sum_cyc += sub.drawwin_cyc;
        self.cursor_sum_cyc += sub.cursor_cyc;
        self.sync_sum_cyc += sub.sync_cyc;
        if sub.had_work {
            self.work_frames += 1;
            self.dirty_sum += sub.dirty_area;
        }
    }
}

struct Prof {
    scratch: Sub,
    buckets: [Bucket; 3], // 0=idle, 1=move, 2=drag
    sec_start: Instant,
    screen_area: u64,
    cycles_per_us: u64, // 0 until calibrated
}

thread_local!(static PROF: RefCell<Prof> = RefCell::new(Prof {
    scratch: Sub::default(),
    buckets: [Bucket::default(); 3],
    sec_start: Instant::now(),
    screen_area: 0,
    cycles_per_us: 0,
}));

pub fn set_screen_area(area: u64) {
    PROF.with(|p| p.borrow_mut().screen_area = area);
}

/// Calibrate TSC→microseconds using a 500 ms sleep (sleep rounds up to APIC timer
/// tick, so a longer baseline gives better accuracy). Returns cycles/us.
pub fn calibrate() -> u64 {
    let t0 = tsc();
    std::thread::sleep(std::time::Duration::from_millis(500));
    let t1 = tsc();
    // 500 ms = 500_000 us; guard against a zero delta.
    let cpu = (t1.saturating_sub(t0)) / 500_000;
    PROF.with(|p| p.borrow_mut().cycles_per_us = cpu.max(1));
    cpu
}

/// Return the calibrated cycles-per-microsecond factor (always ≥ 1).
pub fn cycles_per_us() -> u64 {
    PROF.with(|p| p.borrow().cycles_per_us.max(1))
}

pub fn add_pre(cyc: u64) {
    PROF.with(|p| p.borrow_mut().scratch.pre_cyc += cyc);
}
pub fn add_chrome(cyc: u64) {
    PROF.with(|p| p.borrow_mut().scratch.chrome_cyc += cyc);
}
pub fn add_drawwin(cyc: u64) {
    PROF.with(|p| p.borrow_mut().scratch.drawwin_cyc += cyc);
}
pub fn add_cursor(cyc: u64) {
    PROF.with(|p| p.borrow_mut().scratch.cursor_cyc += cyc);
}
pub fn add_sync(cyc: u64, dirty_area: u64, rect_count: u32) {
    PROF.with(|p| {
        let mut p = p.borrow_mut();
        p.scratch.sync_cyc += cyc;
        p.scratch.dirty_area += dirty_area;
        p.scratch.rect_count += rect_count;
        p.scratch.had_work = true;
    });
}

/// Fold the current frame's scratch into the given phase bucket and reset it.
/// `total_cyc` is the full frame work cost (input + redraw, excluding sleep).
pub fn commit(bucket: usize, total_cyc: u64, input_cyc: u64) {
    PROF.with(|p| {
        let mut p = p.borrow_mut();
        p.scratch.input_cyc = input_cyc;
        let b = if bucket < 3 { bucket } else { 0 };
        let sub = p.scratch;
        p.buckets[b].add_frame(total_cyc, &sub);
        p.scratch = Sub::default();
    });
}

/// Print one summary line per active bucket roughly once per second.
pub fn maybe_flush() {
    PROF.with(|p| {
        let mut p = p.borrow_mut();
        // 1-second pacing via Instant works (clock advances at ~1 s granularity).
        if p.sec_start.elapsed().as_secs() < 1 {
            return;
        }
        let screen = p.screen_area.max(1);
        let cpu = p.cycles_per_us.max(1);
        // helper: cycles -> microseconds (integer)
        let us = |c: u64| -> u64 { c / cpu };
        for (i, b) in p.buckets.iter().enumerate() {
            if b.frames == 0 {
                continue;
            }
            let name = match i {
                0 => "idle",
                1 => "move",
                2 => "drag",
                _ => "?",
            };
            let n = b.frames;
            let dirty_avg = b.dirty_sum / b.work_frames.max(1);
            let ratio_pct = dirty_avg * 100 / screen;
            let line = format!(
                "PROF hz={name} fps={n} ft_us min={mn} avg={avg} max={mx} | inp={inp} pre={pre} chr={chr} dw={dw} cur={cur} syn={syn} | dirty_avg={d} screen={s} ratio={r}% work={wf}/{n}\n",
                mn = us(b.ft_min_cyc),
                avg = us(b.ft_sum_cyc / n),
                mx = us(b.ft_max_cyc),
                inp = us(b.input_sum_cyc / n),
                pre = us(b.pre_sum_cyc / n),
                chr = us(b.chrome_sum_cyc / n),
                dw = us(b.drawwin_sum_cyc / n),
                cur = us(b.cursor_sum_cyc / n),
                syn = us(b.sync_sum_cyc / n),
                d = dirty_avg,
                s = screen,
                r = ratio_pct,
                wf = b.work_frames,
            );
            let _ = std::io::stderr().lock().write_all(line.as_bytes());
        }
        p.buckets = [Bucket::default(); 3];
        p.sec_start = Instant::now();
    });
}
