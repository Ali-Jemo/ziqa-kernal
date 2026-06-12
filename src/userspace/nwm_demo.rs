/// ZiqaOS Desktop — floating window manager, 80×25 VGA text mode.
///
/// Mouse: left-click title bar → focus/drag; [X] → close; [_] → minimize
///        right-click desktop → context menu
/// Keys:  Space=menu  Tab=focus  m=min  x=close  Arrows=move  +/-=resize
///        1-6=launch  h=help  q/ESC=quit
///        Terminal: type+Enter; Editor: type, s=save, arrows=cursor

use core::hint::spin_loop;
use libm;

const W: usize = 80;
const H: usize = 25;
// Usable area: rows 1..H-1 (row 0 = menubar, row H-1 = taskbar)
const DESK_TOP: usize = 1;
const DESK_BOT: usize = H - 2; // last usable row (inclusive)

// ── Colours ───────────────────────────────────────────────────────────────────
const C_DESK:    u8 = 0x17; // white on blue  (desktop bg)
const C_MBAR:    u8 = 0x70; // black on lgray (menu bar)
const C_TBAR:    u8 = 0x70; // taskbar
const C_TITLE:   u8 = 0x1F; // bright-white on blue
const C_TITLEHI: u8 = 0x4F; // bright-white on red  (active)
const C_BORDER:  u8 = 0x17;
const C_BORDEHI: u8 = 0x4E; // yellow on blue (active)
const C_CONTENT: u8 = 0x07;
const C_CLOSE:   u8 = 0x4C;
const C_MENU:    u8 = 0x70;
const C_MENUSEL: u8 = 0x17;

// ── Mouse ─────────────────────────────────────────────────────────────────────

struct Mouse {
    cx: usize,          // VGA column  0..W
    cy: usize,          // VGA row     0..H
    fx: f32,            // smoothed high-precision X
    fy: f32,            // smoothed high-precision Y
    btn: bool,          // left button currently held
    prev_btn: bool,
    rbtn: bool,         // right button
    prev_rbtn: bool,
    drag_slot: Option<usize>,
    drag_ox: usize,     // grab offset within window
    drag_oy: usize,
    resize_slot: Option<usize>,
    select_start: Option<(usize, usize)>,
}

impl Mouse {
    const fn new() -> Self {
        Self { cx:40, cy:12, fx:40.0, fy:12.0, btn:false, prev_btn:false,
               rbtn:false, prev_rbtn:false,
               drag_slot:None, drag_ox:0, drag_oy:0,
               resize_slot:None, select_start:None }
    }

    fn poll(&mut self) {
        let (mx, my) = crate::drivers::ps2_mouse::get_mouse_pos();
        let raw = crate::drivers::ps2_mouse::get_mouse_btn();

        // Target coordinates (80x25)
        let tx = (mx as f32 / 1920.0).clamp(0.0, 1.0) * (W - 1) as f32;
        let ty = (my as f32 / 1080.0).clamp(0.0, 1.0) * (H - 1) as f32;
        
        // EMA Smoothing for "liquid" movement
        let alpha = 0.5; 
        self.fx = self.fx * (1.0 - alpha) + tx * alpha;
        self.fy = self.fy * (1.0 - alpha) + ty * alpha;

        self.cx = self.fx as usize;
        self.cy = self.fy as usize;

        self.prev_btn  = self.btn;
        self.prev_rbtn = self.rbtn;
        self.btn  = raw & 1 != 0;
        self.rbtn = raw & 2 != 0;
    }

    fn just_pressed(&self)  -> bool { self.btn  && !self.prev_btn  }
    fn just_released(&self) -> bool { !self.btn && self.prev_btn   }
    fn rjust_pressed(&self) -> bool { self.rbtn && !self.prev_rbtn }
}

// ── Screen (double-buffer + dirty flush) ──────────────────────────────────────
struct Screen {
    vga:    *mut u16,
    shadow: [[u16; W]; H],
    prev:   [[u16; W]; H],
}
impl Screen {
    fn new(off: u64) -> Self {
        Self { vga:(off+0xb8000)as*mut u16, shadow:[[0;W];H], prev:[[0xFFFF;W];H] }
    }
    #[inline] fn put(&mut self, x:usize, y:usize, ch:u8, at:u8) {
        if x<W && y<H { self.shadow[y][x]=(ch as u16)|((at as u16)<<8); }
    }
    fn flush(&mut self) {
        let mut fb_dirty = false;
        for y in 0..H { for x in 0..W {
            let c=self.shadow[y][x];
            if c!=self.prev[y][x] {
                unsafe{core::ptr::write_volatile(self.vga.add(y*W+x),c);}
                crate::drivers::fb_console::draw_cell(x, y, c as u8, (c >> 8) as u8);
                self.prev[y][x]=c;
                fb_dirty = true;
            }
        }}
        if fb_dirty {
            crate::drivers::fb_console::flush();
        }
    }
    fn fill(&mut self, x:usize, y:usize, w:usize, h:usize, ch:u8, at:u8) {
        for r in y..y+h { for c in x..x+w { self.put(c,r,ch,at); } }
    }
    fn hline(&mut self, y:usize, ch:u8, at:u8) { for x in 0..W{self.put(x,y,ch,at);} }
    fn print(&mut self, x:usize, y:usize, s:&str, at:u8) {
        for (i,b) in s.bytes().enumerate() { if x+i>=W{break;} self.put(x+i,y,b,at); }
    }
    fn print_n(&mut self, x:usize, y:usize, mut n:u32, w:usize, at:u8) {
        let mut buf=[b' ';10]; let mut i=9usize;
        if n==0{buf[i]=b'0';}
        else{while n>0{buf[i]=b'0'+(n%10)as u8;n/=10;if i>0{i-=1;}else{break;}}i+=1;}
        let d=&buf[i..]; let pad=w.saturating_sub(d.len());
        for j in 0..pad{self.put(x+j,y,b' ',at);}
        for (j,&b) in d.iter().enumerate(){self.put(x+pad+j,y,b,at);}
    }
}

fn box_draw(s:&mut Screen, x:usize, y:usize, w:usize, h:usize, at:u8) {
    if w<2||h<2{return;}
    s.put(x,y,0xC9,at); s.put(x+w-1,y,0xBB,at);
    s.put(x,y+h-1,0xC8,at); s.put(x+w-1,y+h-1,0xBC,at);
    for c in x+1..x+w-1{s.put(c,y,0xCD,at);s.put(c,y+h-1,0xCD,at);}
    for r in y+1..y+h-1{s.put(x,r,0xBA,at);s.put(x+w-1,r,0xBA,at);}
}

// ── App kinds ─────────────────────────────────────────────────────────────────
#[derive(Clone,Copy,PartialEq)]
enum App { Terminal, SysMon, Files, Network, Editor, About, Cube3D, Snake }

impl App {
    fn title(self) -> &'static str { match self {
        App::Terminal => "Terminal", App::SysMon => "System Monitor",
        App::Files    => "File Manager", App::Network => "Network",
        App::Editor   => "Text Editor", App::About   => "About ZiqaOS",
        App::Cube3D   => "3D Cube", App::Snake => "Snake Game",
    }}
    fn default_size(self) -> (usize,usize) { match self {
        App::Terminal => (44,16),
        App::SysMon   => (42,16),
        App::Files    => (50,14),
        App::Network  => (44,14),
        App::Editor   => (50,16),
        App::About    => (36,16),
        App::Cube3D   => (40,18),
        App::Snake    => (34,16),
    }}
}

// ── Input buffer ──────────────────────────────────────────────────────────────
struct IBuf { b:[u8;36], n:usize }
impl IBuf {
    const fn new()->Self{Self{b:[0;36],n:0}}
    fn push(&mut self,c:u8){if self.n<36{self.b[self.n]=c;self.n+=1;}}
    fn pop(&mut self){if self.n>0{self.n-=1;}}
    fn clear(&mut self){self.n=0;}
    fn as_str(&self)->&str{core::str::from_utf8(&self.b[..self.n]).unwrap_or("")}
}

// ── Terminal scrollback ───────────────────────────────────────────────────────
struct TOut { lines:[[u8;48];40], attrs:[u8;40], head:usize, count:usize }
impl TOut {
    const fn new()->Self{Self{lines:[[b' ';48];40],attrs:[0x07;40],head:0,count:0}}
    fn push(&mut self, text:&str, at:u8){
        let idx=(self.head+self.count)%40;
        let b=text.as_bytes(); let n=b.len().min(48);
        self.lines[idx][..n].copy_from_slice(&b[..n]);
        for v in &mut self.lines[idx][n..]{*v=b' ';}
        self.attrs[idx]=at;
        if self.count<40{self.count+=1;}else{self.head=(self.head+1)%40;}
    }
    fn get(&self, from_end:usize)->Option<(&[u8;48],u8)>{
        if from_end>=self.count{return None;}
        let idx=(self.head+self.count-1-from_end)%40;
        Some((&self.lines[idx],self.attrs[idx]))
    }
}

// ── Editor buffer ─────────────────────────────────────────────────────────────
struct EdBuf { lines:[[u8;48];20], lens:[usize;20], nlines:usize, cy:usize, cx:usize, dirty:bool }
impl EdBuf {
    fn new()->Self{
        let mut e=Self{lines:[[b' ';48];20],lens:[0;20],nlines:1,cy:0,cx:0,dirty:false};
        let welcome=b"# ZiqaOS Notes";
        e.lines[0][..welcome.len()].copy_from_slice(welcome);
        e.lens[0]=welcome.len();
        e
    }
    fn insert(&mut self, ch:u8){
        let r=self.cy; let c=self.cx;
        if c<47 && r<20 {
            let l=self.lens[r];
            if c<l { for i in (c..l).rev(){self.lines[r][i+1]=self.lines[r][i];} }
            self.lines[r][c]=ch; self.lens[r]=(l+1).min(48); self.cx+=1; self.dirty=true;
        }
    }
    fn backspace(&mut self){
        let r=self.cy; let c=self.cx;
        if c>0 {
            let l=self.lens[r];
            for i in c-1..l-1{self.lines[r][i]=self.lines[r][i+1];}
            self.lens[r]=l.saturating_sub(1); self.cx-=1; self.dirty=true;
        }
    }
    fn newline(&mut self){
        if self.nlines<20{self.nlines+=1; self.cy=(self.cy+1).min(19); self.cx=0; self.dirty=true;}
    }
    fn move_up(&mut self){if self.cy>0{self.cy-=1; self.cx=self.cx.min(self.lens[self.cy]);}}
    fn move_down(&mut self){if self.cy+1<self.nlines{self.cy+=1; self.cx=self.cx.min(self.lens[self.cy]);}}
    fn move_left(&mut self){if self.cx>0{self.cx-=1;}}
    fn move_right(&mut self){if self.cx<self.lens[self.cy]{self.cx+=1;}}
}

// ── Window ────────────────────────────────────────────────────────────────────
struct Win {
    app:  App,
    x: usize, y: usize,
    w: usize, h: usize,
    minimized: bool,
    maximized: bool,
    saved_geom: Option<(usize, usize, usize, usize)>,
    scroll: usize,
}
impl Win {
    fn new(app:App, x:usize, y:usize)->Self{
        let (w,h)=app.default_size();
        Self{app,x,y,w,h,minimized:false,maximized:false,saved_geom:None,scroll:0}
    }
    fn toggle_maximize(&mut self) {
        if self.maximized {
            if let Some((sx, sy, sw, sh)) = self.saved_geom {
                self.x = sx;
                self.y = sy;
                self.w = sw;
                self.h = sh;
            }
            self.maximized = false;
        } else {
            self.saved_geom = Some((self.x, self.y, self.w, self.h));
            self.x = 0;
            self.y = DESK_TOP;
            self.w = W;
            self.h = DESK_BOT - DESK_TOP + 1;
            self.maximized = true;
        }
    }
    fn clamp(&mut self){
        if self.maximized { return; }
        if self.x+self.w>W{self.x=W.saturating_sub(self.w);}
        if self.y<DESK_TOP{self.y=DESK_TOP;}
        if self.y+self.h>DESK_BOT+1{self.y=DESK_BOT+1-self.h.min(DESK_BOT);}
    }
}

// ── App renderers ─────────────────────────────────────────────────────────────

fn cpu_v(t:u32)->u32{15+((t/4)%65+(t/11+30)%35)/2}
fn mem_v(t:u32)->u32{38+t/120%10}
fn rx_v(t:u32)->u32{10+(t/3)%80}
fn tx_v(t:u32)->u32{5+(t/5)%40}

fn spark(s:&mut Screen, x:usize, y:usize, w:usize, t:u32, f:fn(u32)->u32, col:u8){
    for i in 0..w {
        let v=(f(t.saturating_sub((w-1-i)as u32))*8/100).min(7)as usize;
        s.put(x+i,y,[b' ',0xF9,0xF8,0xFA,0xB0,0xB1,0xB2,0xDB][v],col);
    }
}
fn hbar(s:&mut Screen, x:usize, y:usize, len:usize, fill:usize, col:u8){
    for i in 0..len{s.put(x+i,y,if i<fill{0xDB}else{0xB0},if i<fill{col}else{0x08});}
}

fn render_terminal(s:&mut Screen, x:usize, y:usize, w:usize, h:usize,
                   t:u32, scroll:usize, out:&TOut, inp:&IBuf){
    let rows=h.saturating_sub(1);
    for i in 0..rows {
        let fe=rows-1-i+scroll;
        if let Some((line,at))=out.get(fe){
            for j in 0..w.min(48){s.put(x+j,y+i,line[j],at);}
        }
    }
    s.print(x,y+h-1,"[ziqa ~]# ",0x0B);
    let is=inp.as_str(); let il=is.len().min(w.saturating_sub(11));
    s.print(x+10,y+h-1,&is[..il],0x0F);
    let cx=x+10+il;
    if (t/3)%2==0&&cx<x+w{s.put(cx,y+h-1,0xDB,0x0A);}
    if scroll>0{s.print(x+w.saturating_sub(5),y,"[^^^]",0x70);}
}

fn render_sysmon(s:&mut Screen, x:usize, y:usize, w:usize, h:usize, t:u32){
    let bw=w.saturating_sub(11).min(24);
    let cpu=cpu_v(t); let mem=mem_v(t);
    let net=rx_v(t)/2; let dsk=2+t/20%12;
    let (comp_count, comp_orig, comp_size) = crate::memory::compression::PAGE_STORE.get_stats();
    let savings_pct = if comp_orig > 0 { ((comp_orig.saturating_sub(comp_size)) * 100 / comp_orig) as u32 } else { 0 };
    let meters = [
        ("CPU", cpu, 0x0Au8),
        ("MEM", mem, 0x0E),
        ("NET", net, 0x0B),
        ("DSK", dsk, 0x0D),
        ("CMP", savings_pct, 0x0D),
    ];
    for (r, &(lbl, pct, col)) in meters.iter().enumerate() {
        if r >= h { break; }
        let c = if pct > 80 { 0x0C } else { col };
        let f = (bw as u32 * pct / 100) as usize;
        s.print(x, y + r, lbl, 0x0F);
        s.print_n(x + 4, y + r, pct, 3, c);
        s.print(x + 7, y + r, "% [", 0x0F);
        hbar(s, x + 10, y + r, bw, f, c);
        s.put(x + 10 + bw, y + r, b']', 0x0F);
    }
    if h>6{
        s.print(x,y+5,"System Performance",0x08);
        let chart_w = w.saturating_sub(2).min(34);
        for i in 0..chart_w {
            let vt = t.saturating_sub((chart_w - 1 - i) as u32);
            let val = cpu_v(vt);
            let h_idx = (val * 4 / 100).min(3) as usize;
            let ch = [0xB0, 0xB1, 0xB2, 0xDB][h_idx];
            s.put(x+i, y+6, ch, 0x0A);
        }
    }
    if h>10{
        s.print(x,y+8," PID  NAME        CPU  MEM  ST",0x70);
        let ps:&[(u32,&str,u32,u32,u8)]=&[
            (1,"init",0,2,b'S'),(2,"shell",1,4,b'S'),(3,"nwm",cpu%5+1,8,b'R'),
            (4,"pagecache",0,6,b'S'),(5,"ebpf_vm",0,3,b'S'),(6,"virtio",1,2,b'S'),
        ];
        for (i,&(pid,name,c,m,st)) in ps.iter().enumerate(){
            if 9+i>=h{break;}
            let a=if i%2==0{0x07}else{0x08};
            s.print_n(x,y+9+i,pid,4,a); s.print(x+5,y+9+i,name,a);
            s.print_n(x+17,y+9+i,c,3,0x0A); s.print_n(x+21,y+9+i,m,3,0x0E);
            s.put(x+26,y+9+i,st,if st==b'R'{0x0A}else{0x08});
        }
    }
    if h>0{
        let r=y+h-1; let sec=t;
        s.print(x,r,"Uptime: ",0x08);
        s.print_n(x+8,r,sec/3600,2,0x0B); s.put(x+10,r,b'h',0x08);
        s.print_n(x+11,r,(sec/60)%60,2,0x0B); s.put(x+13,r,b'm',0x08);
        s.print_n(x+14,r,sec%60,2,0x0B); s.put(x+16,r,b's',0x08);
        
        s.print(x + w.saturating_sub(16), r, "ZRAM:", 0x08);
        s.print_n(x + w.saturating_sub(11), r, comp_count as u32, 4, 0x0A);
        s.print(x + w.saturating_sub(7), r, "pgs", 0x08);
    }
}
// ── 3D Rotating Cube ──────────────────────────────────────────────────────────

fn render_cube3d(s: &mut Screen, cx: usize, cy: usize, w: usize, h: usize, t: u32) {
    let hw = w / 2;
    let hh = h / 2;
    let bw = w.min(40);
    let bh = h.min(20);
    let yaw   = t as f32 * 0.07;
    let pitch = libm::sinf(t as f32 * 0.05) * 0.6;
    let cy_f = libm::cosf(yaw);
    let sy_f = libm::sinf(yaw);
    let cpf = libm::cosf(pitch);
    let spf = libm::sinf(pitch);

    const CAM: f32 = 3.5;
    const V: [(f32, f32, f32); 8] = [
        (-1.0, -1.0, -1.0),( 1.0, -1.0, -1.0),( 1.0,  1.0, -1.0),(-1.0,  1.0, -1.0),
        (-1.0, -1.0,  1.0),( 1.0, -1.0,  1.0),( 1.0,  1.0,  1.0),(-1.0,  1.0,  1.0),
    ];
    const E: [(u8, u8); 12] = [
        (0,1),(1,2),(2,3),(3,0),
        (4,5),(5,6),(6,7),(7,4),
        (0,4),(1,5),(2,6),(3,7),
    ];

    let mut pv = [(0i32, 0i32); 8];
    for i in 0..8 {
        let (vx, vy, vz) = V[i];
        let x1 = vx * cy_f - vz * sy_f;
        let z1 = vx * sy_f + vz * cy_f;
        let y1 = vy * cpf - z1 * spf;
        let z2 = vy * spf + z1 * cpf + CAM;
        if z2 <= 0.1 { pv[i] = (-999, -999); continue; }
        let sc = 2.2 / z2;
        pv[i] = ((x1 * sc * hw as f32) as i32 + hw as i32,
                 (y1 * sc * hh as f32) as i32 + hh as i32);
    }

    for (ei, &(a, b)) in E.iter().enumerate() {
        let (x0, y0) = pv[a as usize];
        let (x1, y1) = pv[b as usize];
        if x0 < -500 || x1 < -500 { continue; }
        let col = 0x0A + ((ei as u32 + t / 6) % 6) as u8;
        line(s, cx, cy, bw, bh, x0, y0, x1, y1, b'*', col);
    }
}

fn line(s: &mut Screen, ox: usize, oy: usize, bw: usize, bh: usize,
        mut x0: i32, mut y0: i32, x1: i32, y1: i32, ch: u8, at: u8) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        if x0 >= 0 && x0 < bw as i32 && y0 >= 0 && y0 < bh as i32 {
            s.put(ox + x0 as usize, oy + y0 as usize, ch, at);
        }
        if x0 == x1 && y0 == y1 { break; }
        let e2 = 2 * err;
        if e2 >= dy { err += dy; x0 += sx; }
        if e2 <= dx { err += dx; y0 += sy; }
    }
}

struct Snake {
    body: [(usize, usize); 64],
    len: usize,
    dir: (i32, i32), // (dx, dy)
    food: (usize, usize),
    score: u32,
    game_over: bool,
    last_tick: u32,
}

impl Snake {
    fn new() -> Self {
        let mut body = [(0, 0); 64];
        body[0] = (10, 5);
        body[1] = (9, 5);
        body[2] = (8, 5);
        Self {
            body,
            len: 3,
            dir: (1, 0), // Moving right
            food: (15, 7),
            score: 0,
            game_over: false,
            last_tick: 0,
        }
    }

    fn reset(&mut self) {
        *self = Self::new();
    }

    fn step(&mut self, w: usize, h: usize, tick: u32) {
        if self.game_over {
            return;
        }
        if tick.wrapping_sub(self.last_tick) < 6 {
            return;
        }
        self.last_tick = tick;

        let head = self.body[0];
        let next_x = (head.0 as i32 + self.dir.0) as usize;
        let next_y = (head.1 as i32 + self.dir.1) as usize;

        if next_x == 0 || next_x >= w - 1 || next_y == 0 || next_y >= h - 1 {
            self.game_over = true;
            return;
        }

        for i in 0..self.len {
            if self.body[i] == (next_x, next_y) {
                self.game_over = true;
                return;
            }
        }

        for i in (1..self.len).rev() {
            self.body[i] = self.body[i - 1];
        }
        self.body[0] = (next_x, next_y);

        if (next_x, next_y) == self.food {
            if self.len < 64 {
                self.body[self.len] = self.body[self.len - 1];
                self.len += 1;
            }
            self.score += 10;
            let mut fx = (tick as usize % (w - 2)) + 1;
            let mut fy = (tick as usize % (h - 2)) + 1;
            if fx == 0 { fx = 1; }
            if fy == 0 { fy = 1; }
            self.food = (fx, fy);
        }
    }
}

fn render_snake(s: &mut Screen, x: usize, y: usize, w: usize, h: usize, snake: &mut Snake, tick: u32) {
    snake.step(w, h, tick);

    for c in 0..w {
        s.put(x + c, y, 0xCD, 0x07);
        s.put(x + c, y + h - 1, 0xCD, 0x07);
    }
    for r in 1..h - 1 {
        s.put(x, y + r, 0xBA, 0x07);
        s.put(x + w - 1, y + r, 0xBA, 0x07);
    }

    if snake.game_over {
        s.print(x + (w.saturating_sub(10)) / 2, y + h / 2, "GAME OVER", 0x0C);
        s.print(x + (w.saturating_sub(18)) / 2, y + h / 2 + 1, "Press 'r' to restart", 0x0E);
    } else {
        s.put(x + snake.food.0, y + snake.food.1, 0x0F, 0x0E);
        for i in 1..snake.len {
            s.put(x + snake.body[i].0, y + snake.body[i].1, 0xFE, 0x0A);
        }
        s.put(x + snake.body[0].0, y + snake.body[0].1, 0x01, 0x0F);
    }

    s.print(x + 2, y + h - 1, "Score:", 0x70);
    s.print_n(x + 8, y + h - 1, snake.score, 4, 0x70);
}

const FM:&[(&str,&str,u8,&str)]=&[
    ("bin/","System binaries",0x0E,"dir  4096"),
    ("dev/","Device files",0x0E,"dir   512"),
    ("etc/","Configuration",0x0E,"dir  1024"),
    ("proc/","Process info",0x0E,"dir     0"),
    ("tmp/","Temporary",0x0E,"dir  2048"),
    ("kernel.elf","ELF kernel",0x0F,"elf  1.2M"),
    ("init","Init process",0x0F,"elf    4K"),
    (".config","Kernel config",0x08,"txt    1K"),
    ("README.md","Documentation",0x0B,"txt    8K"),
    ("build.rs","Build script",0x0B,"rs     2K"),
    ("Makefile","Build rules",0x07,"mk     1K"),
];

fn render_files(s:&mut Screen, x:usize, y:usize, w:usize, h:usize, t:u32){
    let lw=(w*2/5).max(12); let rx=x+lw+1; let rw=w.saturating_sub(lw+1);
    for r in y..y+h{s.put(x+lw,r,0xB3,0x08);}
    s.print(x,y," Name",0x70); for c in x+5..x+lw{s.put(c,y,b' ',0x70);}
    s.print(rx,y," Type/Size ",0x70); for c in rx+10..rx+rw{s.put(c,y,b' ',0x70);}
    let sel=(t/5)as usize%FM.len();
    for (i,&(name,_,at,meta)) in FM.iter().enumerate().take(h.saturating_sub(2)){
        let is=i==sel;
        let la=if is{0x3F}else{at}; let ra=if is{0x3F}else{0x07};
        let is_dir = name.ends_with('/');
        let icon = if is_dir { 0x10u8 } else { 0x07u8 }; // triangle for dir, bullet for file
        s.put(x,y+1+i,if is{0x10}else{icon},la);
        s.print(x+1,y+1+i,&name[..name.len().min(lw.saturating_sub(2))],la);
        s.print(rx+1,y+1+i,&meta[..meta.len().min(rw.saturating_sub(2))],ra);
    }
    if h>1{
        let &(_,desc,_,_)=&FM[sel];
        s.fill(x,y+h-1,w,1,b' ',0x70);
        s.print(x+1,y+h-1,desc,0x70);
    }
}

fn render_network(s:&mut Screen, x:usize, y:usize, w:usize, h:usize, t:u32){
    let bw=w.saturating_sub(11).min(24);
    let rx=rx_v(t); let tx=tx_v(t);
    s.print(x,y,"RX",0x0F); s.print_n(x+3,y,rx,4,0x0B); s.print(x+7,y," KB/s [",0x0F);
    hbar(s,x+14,y,bw,(bw as u32*rx/100).min(bw as u32)as usize,0x0B); s.put(x+14+bw,y,b']',0x0F);
    if h>1{
        s.print(x,y+1,"TX",0x0F); s.print_n(x+3,y+1,tx,4,0x0D); s.print(x+7,y+1," KB/s [",0x0F);
        hbar(s,x+14,y+1,bw,(bw as u32*tx/100).min(bw as u32)as usize,0x0D); s.put(x+14+bw,y+1,b']',0x0F);
    }
    if h>3{s.print(x,y+3,"RX history:",0x08); spark(s,x,y+4,w.min(38),t,rx_v,0x0B);}
    if h>5{s.print(x,y+6,"TX history:",0x08); spark(s,x,y+7,w.min(38),t,tx_v,0x0D);}
    if h>9{
        s.print(x,y+9," Iface  IP              MAC",0x70);
        s.print(x,y+10," eth0   10.0.2.15       52:54:00:12:34:56",0x07);
        s.print(x,y+11," lo     127.0.0.1       00:00:00:00:00:00",0x08);
    }
    if h>0{
        let tot=t as u64*rx as u64/1024;
        s.print(x,y+h-1,"Total RX: ",0x08);
        s.print_n(x+10,y+h-1,tot as u32%99999,5,0x0B);
        s.print(x+15,y+h-1," KB",0x08);
    }
}

fn render_editor(s:&mut Screen, x:usize, y:usize, w:usize, h:usize, ed:&EdBuf, t:u32){
    // Line numbers + content
    for i in 0..h.saturating_sub(1){
        let lnum_at=if i==ed.cy{0x0B}else{0x08};
        s.print_n(x,y+i,(i+1)as u32,2,lnum_at);
        s.put(x+2,y+i,0xB3,0x08);
        if i<ed.nlines{
            let l=ed.lens[i]; let n=l.min(w.saturating_sub(4));
            for j in 0..n{s.put(x+3+j,y+i,ed.lines[i][j],0x0F);}
            // cursor
            if i==ed.cy{
                let cx=x+3+ed.cx;
                if (t/3)%2==0&&cx<x+w{s.put(cx,y+i,0xDB,0x0A);}
            }
        }
    }
    // Status line
    let st=if ed.dirty{"[modified]"}else{"[saved]   "};
    s.fill(x,y+h-1,w,1,b' ',0x70);
    s.print(x+1,y+h-1,st,0x70);
    s.print(x+w.saturating_sub(12),y+h-1,"s:save  ",0x70);
    s.print_n(x+w.saturating_sub(4),y+h-1,ed.cy as u32+1,2,0x70);
    s.put(x+w.saturating_sub(2),y+h-1,b':',0x70);
    s.print_n(x+w.saturating_sub(1),y+h-1,ed.cx as u32+1,1,0x70);
}

fn render_about(s:&mut Screen, x:usize, y:usize, w:usize, h:usize, t:u32){
    let cols:&[u8]=&[0x0C,0x0E,0x0A,0x0B,0x0D,0x0F];
    let title="*** ZiqaOS ***";
    s.print(x+(w.saturating_sub(title.len()))/2,y,title,cols[(t/10)as usize%cols.len()]);
    
    // Logo in about
    if h > 4 {
        let logo = [
            "  ▄▄▄▄▄▄▄ ▄▄▄ ▄▄▄▄▄▄▄ ▄▄▄▄▄▄▄  ",
            "  █▄▄▄▄▄█ █   █▄▄▄▄▄█ █▄▄▄▄▄█  ",
            "      ▄▀  █   █     █ █     █  ",
            "    ▄▀    █   █▄▄▄▄▄█ █▄▄▄▄▄█  ",
        ];
        for (i, line) in logo.iter().enumerate() {
            if i + 2 >= h { break; }
            s.print(x, y + 2 + i, line, 0x0B);
        }
    }

    let lines:&[(&str,u8)]=&[
        ("  OS Research Playground",0x0B),("",0x07),
        ("  Arch:     x86_64 bare metal",0x07),
        ("  Language: Rust + Zig FFI",0x07),
        ("  Scheduler:MLFQ 5 queues",0x07),
        ("  FS:       ZiqaFS (journal)",0x07),
        ("  ABI:      Linux / WASM / eBPF",0x07),
        ("  Syscalls: 111+",0x07),
        ("  Shell:    34 commands",0x07),
        ("  Security: SMEP/SMAP/UMIP",0x0A),
        ("  Boot:     3-stage VGA pipeline",0x07),
        ("  IPC:      chan / shm / io_uring",0x07),
        ("",0x07),
        ("  (c) 2026 ZiqaOS Project",0x08),
    ];
    for (i,&(txt,at)) in lines.iter().enumerate().take(h.saturating_sub(1)){
        s.print(x,y+1+i,txt,at);
    }
}

// ── Mouse cursor ──────────────────────────────────────────────────────────────

fn draw_selection(s: &mut Screen, start: (usize, usize), current: (usize, usize)) {
    let (x1, y1) = start;
    let (x2, y2) = current;
    let x = x1.min(x2);
    let y = y1.min(y2);
    let w = x1.max(x2).saturating_sub(x);
    let h = y1.max(y2).saturating_sub(y);
    if w < 1 || h < 1 { return; }

    // Draw dashed rectangle using dots
    for i in 0..w {
        if i % 2 == 0 {
            s.put(x + i, y, 0xFA, 0x1B);
            s.put(x + i, y + h, 0xFA, 0x1B);
        }
    }
    for i in 0..h {
        if i % 2 == 0 {
            s.put(x, y + i, 0xFA, 0x1B);
            s.put(x + w, y + i, 0xFA, 0x1B);
        }
    }
}

fn draw_cursor(s: &mut Screen, m: &Mouse, wins: &[Option<Win>; 6], zorder: &[usize; 6], zlen: usize) {
    let (cx, cy) = (m.cx, m.cy);

    let hit = hit_window(wins, zorder, zlen, cx, cy);
    let is_dragging  = m.drag_slot.is_some();
    let is_resizing  = m.resize_slot.is_some() || matches!(hit, Some((_, 4)));
    let is_selecting = m.select_start.is_some();
    let over_title   = matches!(hit, Some((_, 0)));
    let over_close   = matches!(hit, Some((_, 1)));
    let over_btn     = matches!(hit, Some((_, 2))) || matches!(hit, Some((_, 5)));

    let (cursor_char, cursor_color): (u8, u8) = if m.btn && (is_dragging || over_title) {
        (0x0F, 0x0F) // ☼ white — move/drag
    } else if is_resizing {
        (0x12, 0x0B) // ↕ cyan — resize
    } else if is_selecting {
        (0x1B, 0x0A) // ← green — selection
    } else if over_close {
        (0x58, 0x0C) // X red — close
    } else if over_title || over_btn {
        (0x1E, 0x0F) // ▲ white — title bar
    } else if hit.is_some() {
        (0x10, 0x0F) // ► white — content
    } else {
        (0x10, 0x0E) // ► yellow — desktop
    };

    if cx < W && cy < H {
        let bg = (s.shadow[cy][cx] >> 8) as u8 & 0xF0;
        s.put(cx, cy, cursor_char, bg | cursor_color);
    }
    // Highlight cell below-right as cursor "tail" (not while dragging/resizing)
    if !is_dragging && !is_resizing && cx + 1 < W && cy + 1 < H {
        let bg = (s.shadow[cy+1][cx+1] >> 8) as u8 & 0xF0;
        s.put(cx+1, cy+1, 0xFA, bg | 0x08);
    }
    // Click indicator — flash on press
    if m.btn && cx < W && cy < H && !is_dragging && !is_resizing {
        s.put(cx, cy, 0x04, 0x4F); // ♦ bright white on red
    }
}

// ── Hit testing ───────────────────────────────────────────────────────────────

/// Returns (slot, hit_kind) for the topmost window under (cx,cy).
/// hit_kind: 0=title, 1=close_btn, 2=min_btn, 3=content, 4=resize_corner
fn hit_window(wins: &[Option<Win>; 6], zorder: &[usize; 6], zlen: usize,
              cx: usize, cy: usize) -> Option<(usize, u8)> {
    // iterate front-to-back
    for i in (0..zlen).rev() {
        let slot = zorder[i];
        let w = match &wins[slot] { Some(w) if !w.minimized => w, _ => continue };
        
        // Resize corner (2x2 area bottom right)
        if cx >= w.x + w.w - 2 && cx < w.x + w.w && cy >= w.y + w.h - 2 && cy < w.y + w.h {
            return Some((slot, 4));
        }

        if cx < w.x || cy < w.y || cx >= w.x + w.w || cy >= w.y + w.h { continue; }
        
        if cy == w.y {
            // title bar row
            if w.w > 10 {
                let bx = w.x + w.w - 10;
                if cx >= bx + 6 && cx <= bx + 8 { return Some((slot, 1)); } // [X]
                if cx >= bx + 3 && cx <= bx + 5 { return Some((slot, 5)); } // [-] (Maximize)
                if cx >= bx && cx <= bx + 2 { return Some((slot, 2)); }     // [_]
            }
            return Some((slot, 0)); // title drag
        }
        return Some((slot, 3)); // content
    }
    None
}

fn draw_wallpaper(s: &mut Screen, tick: u32, selected_icon: Option<usize>, mouse_cx: usize, mouse_cy: usize) {
    // Gradient-like background: vary blue shade by column zone
    for r in DESK_TOP..=DESK_BOT {
        for c in 0..W {
            let shade: u8 = match c {
                0..=19  => C_DESK, // white on blue
                20..=39 => 0x19, // white on light-blue
                40..=59 => C_DESK,
                _       => 0x18, // white on dark-cyan
            };
            s.put(c, r, b' ', shade);
        }
    }

    // --- Starfield effect ---
    for i in 0..12 {
        let sx = (i * 137 + (tick as usize / 2)) % W;
        let sy = (i * 53 + (tick as usize / 3)) % (DESK_BOT - DESK_TOP) + DESK_TOP;
        if sx < W && sy <= DESK_BOT {
            let ch = if i % 3 == 0 { b'*' } else { 0xFA };
            s.put(sx, sy, ch, 0x18); // dim stars
        }
    }

    // --- Centered Logo (Block Art) ---
    // ZIQA
    let lx = W / 2 - 10;
    let ly = H / 2 - 2;
    let logo = [
        "██▀ ██▀ █▀█ █▀█",
        "█▄█ █▄▄ █▄█ █▀█",
    ];
    for (i, line) in logo.iter().enumerate() {
        s.print(lx, ly + i, line, 0x18); // dark cyan branding
    }

    // Subtle animated scanline every 6 rows
    let scan = ((tick / 5) as usize % (DESK_BOT - DESK_TOP + 1)) + DESK_TOP;
    for c in 0..W { s.put(c, scan, b' ', 0x1B); } // bright-cyan line

    // Desktop icons — 2 columns, each icon is glyph + 2-line label
    // Format: (x, y, glyph, line1, line2, app_idx)
    let icons: &[(usize, usize, u8, &str, &str)] = &[
        (2,  2,  0x01, "Terminal", "[1]"),   // ☺
        (2,  6,  0x02, "SysMon",   "[2]"),   // ☻
        (2,  10, 0x03, "Files",    "[3]"),   // ♥
        (2,  14, 0x04, "Network",  "[4]"),   // ♦
        (2,  18, 0x05, "Editor",   "[5]"),   // ♣
        (12, 2,  0x06, "About",    "[6]"),   // ♠
        (12, 6,  0x0F, "3D Cube",  "[7]"),   // ☼
        (12, 10, 0xFE, "Snake",    "[8]"),   // ■
    ];
    for (i, &(ix, iy, glyph, lbl, num)) in icons.iter().enumerate() {
        let hovered = (mouse_cx >= ix && mouse_cx <= ix + 7)
                   && (mouse_cy >= iy && mouse_cy <= iy + 4);
        let selected = selected_icon == Some(i);
        let bg: u8 = if selected { 0x4E } else if hovered { 0x3E } else { 0x1E };
        let lbg: u8 = if selected { 0x4F } else if hovered { 0x3F } else { 0x17 };
        // Icon box: glyph + border
        s.put(ix,   iy, 0xDA, 0x18); // ┌
        s.put(ix+1, iy, 0xC4, 0x18); // ─
        s.put(ix+2, iy, 0xC4, 0x18);
        s.put(ix+3, iy, 0xBF, 0x18); // ┐
        s.put(ix,   iy+1, 0xB3, 0x18); // │
        s.put(ix+1, iy+1, glyph, bg);
        s.put(ix+2, iy+1, b' ', bg);
        s.put(ix+3, iy+1, 0xB3, 0x18); // │
        s.put(ix,   iy+2, 0xC0, 0x18); // └
        s.put(ix+1, iy+2, 0xC4, 0x18);
        s.put(ix+2, iy+2, 0xC4, 0x18);
        s.put(ix+3, iy+2, 0xD9, 0x18); // ┘
        // Label
        let llen = lbl.len().min(8);
        s.print(ix, iy+3, &lbl[..llen], lbg);
        s.print(ix, iy+4, num, lbg);
        let _ = i;
    }

    // Clock widget bottom-right of desktop
    let hh = (tick / 3600) % 24;
    let mm = (tick / 60) % 60;
    let ss = tick % 60;
    // Box around clock
    let clk_x = W - 12; let clk_y = DESK_BOT - 2;
    s.print(clk_x, clk_y,   "+---------+", 0x1B);
    s.print(clk_x, clk_y+1, "|         |", 0x1B);
    s.print(clk_x, clk_y+2, "+---------+", 0x1B);
    // HH:MM:SS inside
    let clk = [
        b'0'+(hh/10)as u8, b'0'+(hh%10)as u8, b':',
        b'0'+(mm/10)as u8, b'0'+(mm%10)as u8, b':',
        b'0'+(ss/10)as u8, b'0'+(ss%10)as u8,
    ];
    for (i, &b) in clk.iter().enumerate() {
        s.put(clk_x + 1 + i, clk_y + 1, b, 0x1F); // bright white on blue
    }

    // Branding
    s.print(W/2 - 7, DESK_BOT, "ZiqaOS Desktop", 0x18);
}

fn hit_icon(cx: usize, cy: usize) -> Option<usize> {
    // Icons at (2,2),(2,6),(2,10),(2,14),(2,18),(12,2),(12,6),(12,10) — each 4×5 cells
    let pos: &[(usize, usize)] = &[(2,2),(2,6),(2,10),(2,14),(2,18),(12,2),(12,6),(12,10)];
    for (i, &(ix, iy)) in pos.iter().enumerate() {
        if cx >= ix && cx <= ix+3 && cy >= iy && cy <= iy+4 {
            return Some(i);
        }
    }
    None
}

// ── Start menu ────────────────────────────────────────────────────────────────

const MENU_APPS:&[(App,&str,&str)]=&[
    (App::Terminal,"[1] Terminal",    "Command line shell"),
    (App::SysMon,  "[2] System Mon",  "CPU/MEM/process monitor"),
    (App::Files,   "[3] File Manager","Browse filesystem"),
    (App::Network, "[4] Network",     "Network stats & info"),
    (App::Editor,  "[5] Text Editor", "Edit text files"),
    (App::About,   "[6] About",       "About ZiqaOS"),
    (App::Cube3D,  "[7] 3D Cube",     "Rotating wireframe cube"),
    (App::Snake,   "[8] Snake",       "Play classic Snake"),
];

fn draw_startmenu(s:&mut Screen, sel:usize){
    let mx=0; let my=H-1-MENU_APPS.len()-3;
    let mw=36; let mh=MENU_APPS.len()+3;
    s.fill(mx,my,mw,mh,b' ',C_MENU);
    box_draw(s,mx,my,mw,mh,0x70);
    s.print(mx+1,my," \x10 ZiqaOS Applications \x11 ",0x4F);
    for (i,&(_,name,desc)) in MENU_APPS.iter().enumerate(){
        let at=if i==sel{C_MENUSEL}else{C_MENU};
        let prefix=if i==sel{0x10u8}else{b' '};
        s.put(mx+1,my+1+i,prefix,at);
        s.print(mx+2,my+1+i,name,at);
        // description in dim
        let dx=mx+16; let dl=desc.len().min(mw.saturating_sub(17));
        s.print(dx,my+1+i,&desc[..dl],if i==sel{0x1F}else{0x08});
    }
    s.print(mx+1,my+mh-2," Enter:open  ESC:close  j/k:nav ",0x08);
}

// ── Window chrome ─────────────────────────────────────────────────────────────

fn draw_window(s:&mut Screen, win:&Win, active:bool, t:u32,
               tout:&TOut, tinp:&IBuf, ed:&EdBuf, snake:&mut Snake, mx:usize, my:usize, mbtn:bool){
    if win.minimized{return;}
    let (wx,wy,ww,wh)=(win.x,win.y,win.w,win.h);
    if ww<4||wh<3{return;}

    // Shadow (2 cells right, 1 cell down, stippled)
    for r in wy+1..wy+wh+1{
        for c in wx+2..wx+ww+2{
            if c<W&&r<H{
                // 0xB0 = ░ (light stipple), provides a fake "transparency" over desktop
                let bg = (s.shadow[r][c] >> 8) as u8 & 0xF0; 
                s.put(c,r,0xB0, bg | 0x08);
            }
        }
    }

    // Border
    let bat=if active{C_BORDEHI}else{C_BORDER};
    box_draw(s,wx,wy,ww,wh,bat);

    // Title bar
    let tat=if active{C_TITLEHI}else{C_TITLE};
    for c in wx+1..wx+ww-1{s.put(c,wy,b' ',tat);}
    s.print(wx+1,wy,win.app.title(),tat);

    // Buttons: [_][-][X]
    if ww>10{
        let bx = wx + ww - 10;
        let mut b_min = 0x70; let mut b_max = 0x70; let mut b_cls = C_CLOSE;

        if my == wy {
            if mx >= bx && mx <= bx+2 { 
                b_min = if mbtn { 0x17 } else { 0x1F }; // invert on click
            }
            if mx >= bx+3 && mx <= bx+5 { 
                b_max = if mbtn { 0x17 } else { 0x1F };
            }
            if mx >= bx+6 && mx <= bx+8 { 
                b_cls = if mbtn { 0x4F } else { 0xCE }; // deeper red on click
            }
        }
        let max_char = if win.maximized { "[=]" } else { "[-]" };
        s.print(bx, wy, "[_]", b_min);
        s.print(bx+3, wy, max_char, b_max);
        s.print(bx+6, wy, "[X]", b_cls);
    }

    // Content
    let cx=wx+1; let cy=wy+1;
    let cw=ww.saturating_sub(2); let ch=wh.saturating_sub(2);
    s.fill(cx,cy,cw,ch,b' ',C_CONTENT);

    match win.app {
        App::Terminal => render_terminal(s,cx,cy,cw,ch,t,win.scroll,tout,tinp),
        App::SysMon   => render_sysmon(s,cx,cy,cw,ch,t),
        App::Files    => render_files(s,cx,cy,cw,ch,t),
        App::Network  => render_network(s,cx,cy,cw,ch,t),
        App::Editor   => render_editor(s,cx,cy,cw,ch,ed,t),
        App::About    => render_about(s,cx,cy,cw,ch,t),
        App::Cube3D   => render_cube3d(s, cx, cy, cw, ch, t),
        App::Snake    => render_snake(s, cx, cy, cw, ch, snake, t),
    }
}

// ── Menu bar ──────────────────────────────────────────────────────────────────

fn draw_menubar(s: &mut Screen, t: u32, win_count: usize, mcx: usize, mcy: usize) {
    s.hline(0, b' ', C_MBAR);
    s.print(0, 0, " \x10 ZiqaOS ", 0x4F);
    s.print(9, 0, "  Apps  Windows  Help ", C_MBAR);
    s.print(32, 0, "[", C_MBAR);
    s.print_n(33, 0, win_count as u32, 1, 0x4F);
    s.print(34, 0, " open]", C_MBAR);
    // Mouse position indicator
    s.print(42, 0, "M:", C_MBAR);
    s.print_n(44, 0, mcx as u32, 2, 0x4F);
    s.put(46, 0, b',', C_MBAR);
    s.print_n(47, 0, mcy as u32, 2, 0x4F);
    // Clock
    let hh=(t/3600)%24; let mm=(t/60)%60; let ss=t%60;
    let clk=[b'0'+(hh/10)as u8,b'0'+(hh%10)as u8,b':',
             b'0'+(mm/10)as u8,b'0'+(mm%10)as u8,b':',
             b'0'+(ss/10)as u8,b'0'+(ss%10)as u8];
    for (i,&b) in clk.iter().enumerate() { s.put(W-9+i, 0, b, C_MBAR); }
}

// ── Taskbar ───────────────────────────────────────────────────────────────────

fn draw_taskbar(s: &mut Screen, wins: &[Option<Win>; 6], zorder: &[usize; 6], zlen: usize, t: u32) {
    // Taskbar border (double line)
    for x in 0..W { s.put(x, H-2, 0xCD, 0x08); }
    s.fill(0, H-1, W, 1, b' ', C_TBAR);

    // Start button with special glyphs
    s.print(0, H-1, " \x11 ZiqaOS \x10 ", 0x4F);

    // Open window buttons
    let mut bx = 12usize;
    for i in 0..zlen {
        let slot = zorder[i];
        if let Some(w) = &wins[slot] {
            if w.minimized { continue; }
            let is_active = i == zlen - 1;
            let at: u8 = if is_active { 0x1F } else { 0x70 };
            let title = w.app.title();
            let tlen = title.len().min(12);
            
            s.put(bx, H-1, 0xB3, 0x08); // Separator │
            s.print(bx + 1, H-1, &title[..tlen], at);
            bx += tlen + 2;
            if bx + 14 >= W { break; }
        }
    }
    
    // System tray area
    let tx = W - 18;
    s.put(tx, H-1, 0xB3, 0x08);
    // Battery
    let bat = 100u32.saturating_sub(t/300%101);
    let bc: u8 = if bat>50{0x0A}else if bat>20{0x0E}else{0x0C};
    s.print(tx+2, H-1, "BAT:", 0x08); 
    s.print_n(tx+6, H-1, bat, 3, bc); 
    s.put(tx+9, H-1, b'%', 0x08);
}

// ── Help overlay ──────────────────────────────────────────────────────────────

fn draw_help(s:&mut Screen){
    let bx=16;let by=3;let bw=48;let bh=18;
    s.fill(bx,by,bw,bh,b' ',0x30);
    box_draw(s,bx,by,bw,bh,0x3F);
    s.print(bx+1,by," ZiqaOS Desktop Help ",0x3F);
    let lines:&[&str]=&[
        " Space      Open/close Start Menu",
        " Tab        Cycle window focus",
        " Arrow keys Move focused window",
        " +/-        Resize window width",
        " m          Minimize/restore window",
        " f          Maximize/restore window",
        " x          Close focused window",
        " 1-8        Launch app directly",
        " h          Toggle this help",
        " q / ESC    Quit desktop",
        "",
        " Mouse Controls:",
        "  Drag title to move (drop top to max)",
        "  Double-click title to maximize/restore",
        "  Drag desktop to select icons",
        "  Double-click icon to launch app",
    ];
    for (i,&l) in lines.iter().enumerate().take(bh.saturating_sub(2)){
        s.print(bx+1,by+1+i,l,0x30);
    }
    s.print(bx+1,by+bh-2," (Click anywhere to close) ",0x38);
}

// ── Right-click context menu ──────────────────────────────────────────────────

struct CtxMenu { x: usize, y: usize, sel: usize, open: bool }
impl CtxMenu {
    const fn new() -> Self { Self { x:0, y:0, sel:0, open:false } }
    fn show(&mut self, x: usize, y: usize) { self.x=x; self.y=y; self.sel=0; self.open=true; }
}

const CTX_ITEMS: &[&str] = &[
    "Open Terminal",
    "Open SysMon",
    "Open Files",
    "Open Network",
    "Open Editor",
    "Open About",
    "Open Snake",
    "-------------",
    "Help",
];

fn draw_ctxmenu(s: &mut Screen, m: &CtxMenu) {
    if !m.open { return; }
    let w = 16usize; let h = CTX_ITEMS.len() + 2;
    let mx = m.x.min(W.saturating_sub(w));
    let my = m.y.min(H.saturating_sub(h));
    s.fill(mx, my, w, h, b' ', C_MENU);
    box_draw(s, mx, my, w, h, 0x70);
    for (i, &item) in CTX_ITEMS.iter().enumerate() {
        let is_sep = item.starts_with('-');
        let at = if i == m.sel && !is_sep { C_MENUSEL } else { C_MENU };
        let prefix = if i == m.sel && !is_sep { 0x10u8 } else { b' ' };
        s.put(mx+1, my+1+i, prefix, at);
        // print byte-by-byte up to column limit (safe for any UTF-8)
        let max_cols = w.saturating_sub(3);
        let mut col = 0usize;
        for b in item.bytes() {
            if col >= max_cols { break; }
            s.put(mx+2+col, my+1+i, b, at);
            col += 1;
        }
    }
}

// Hit test context menu: returns item index if clicked inside
fn hit_ctxmenu(m: &CtxMenu, cx: usize, cy: usize) -> Option<usize> {
    if !m.open { return None; }
    let w = 16usize; let h = CTX_ITEMS.len() + 2;
    let mx = m.x.min(W.saturating_sub(w));
    let my = m.y.min(H.saturating_sub(h));
    if cx >= mx && cx < mx+w && cy > my && cy < my+h-1 {
        Some(cy - my - 1)
    } else {
        None
    }
}

fn read_key() -> Option<u8> {
    let mut b = [0u8; 1];
    if crate::drivers::keyboard::read_stdin(&mut b) > 0 {
        if b[0] == 0x1B {
            crate::timer::sleep_ms(crate::process::Pid(0), 2);
            let mut next1 = [0u8; 1];
            if crate::drivers::keyboard::read_stdin(&mut next1) > 0 {
                if next1[0] == b'[' {
                    let mut next2 = [0u8; 1];
                    if crate::drivers::keyboard::read_stdin(&mut next2) > 0 {
                        match next2[0] {
                            b'A' => return Some(0x80), // Arrow Up
                            b'B' => return Some(0x81), // Arrow Down
                            b'D' => return Some(0x82), // Arrow Left
                            b'C' => return Some(0x83), // Arrow Right
                            _ => {}
                        }
                    }
                }
            }
        }
        Some(b[0])
    } else {
        None
    }
}
fn frame_delay(ms: u64) {
    let deadline = crate::timer::uptime_ms().wrapping_add(ms);
    while crate::timer::uptime_ms().wrapping_sub(deadline) > u64::MAX / 2 {
        spin_loop();
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn run(){
    let phys_offset=crate::BOOT_INFO.lock()
        .as_ref().map(|bi|bi.physical_memory_offset).unwrap_or(0);
    let mut s=Screen::new(phys_offset);
    crate::drivers::keyboard::clear_stdin();

    let mut wins:[Option<Win>;6]=[None,None,None,None,None,None];
    let mut zorder=[0usize;6];
    let mut zlen;
    let mut tout=TOut::new();
    let mut tinp=IBuf::new();
    let mut ed=EdBuf::new();
    let mut mouse=Mouse::new();
    let mut ctx=CtxMenu::new();
    let mut snake = Snake::new();
    let mut selected_icon: Option<usize> = None;
    let mut last_click_tick = 0u32;
    let mut last_click_pos = (0usize, 0usize);

    for &(at,line) in &[
        (0x0A,"ZiqaOS v0.1 (x86_64)"),
        (0x08,"(c) 2026 ZiqaOS Project"),
        (0x07,""),
        (0x07,"Type 'help' for commands."),
    ]{ tout.push(line,at); }

    let mut tick:u32=0;
    let mut menu_open=false;
    let mut menu_sel=0usize;
    let mut show_help=false;

    // Open Terminal on start
    wins[0]=Some(Win::new(App::Terminal,18,2));
    zorder[0]=0; zlen=1;

    let find_slot=|wins:&[Option<Win>;6]|->Option<usize>{
        for i in 0..6{if wins[i].is_none(){return Some(i);}} None
    };
    let active_slot=|zo:&[usize;6],zl:usize|->Option<usize>{
        if zl>0{Some(zo[zl-1])}else{None}
    };

    macro_rules! launch {
        ($app:expr) => {{
            let offx=(tick as usize*7)%20;
            let offy=(tick as usize*5)%8;
            if let Some(slot)=find_slot(&wins){
                let mut w=Win::new($app,18+offx,2+offy);
                w.clamp();
                wins[slot]=Some(w);
                let mut nz=[0usize;6]; let mut nl=0;
                for i in 0..zlen{if zorder[i]!=slot{nz[nl]=zorder[i];nl+=1;}}
                nz[nl]=slot; nl+=1;
                zorder=nz; zlen=nl;
            }
        }};
    }

    macro_rules! bring_front {
        ($slot:expr) => {{
            let slot=$slot;
            let mut nz=[0usize;6]; let mut nl=0;
            for i in 0..zlen{if zorder[i]!=slot{nz[nl]=zorder[i];nl+=1;}}
            nz[nl]=slot; nl+=1;
            zorder=nz; zlen=nl;
        }};
    }

    macro_rules! close_slot {
        ($slot:expr) => {{
            let slot=$slot;
            wins[slot]=None;
            let mut nz=[0usize;6]; let mut nl=0;
            for i in 0..zlen{if zorder[i]!=slot{nz[nl]=zorder[i];nl+=1;}}
            zorder=nz; zlen=nl;
        }};
    }

    loop {
        // ── Mouse ─────────────────────────────────────────────────────────────
        mouse.poll();
        let (mcx, mcy) = (mouse.cx, mouse.cy);

        if mouse.just_pressed() {
            let is_double_click = tick.wrapping_sub(last_click_tick) < 15 && last_click_pos == (mcx, mcy);
            last_click_tick = tick;
            last_click_pos = (mcx, mcy);

            if ctx.open {
                selected_icon = None;
                if let Some(item)=hit_ctxmenu(&ctx,mcx,mcy){
                    ctx.open=false;
                    match item {
                        0 => launch!(App::Terminal),
                        1 => launch!(App::SysMon),
                        2 => launch!(App::Files),
                        3 => launch!(App::Network),
                        4 => launch!(App::Editor),
                        5 => launch!(App::About),
                        6 => launch!(App::Snake),
                        8 => { show_help=!show_help; }
                        _ => {}
                    }
                } else { ctx.open=false; }
            } else if menu_open {
                selected_icon = None;
                menu_open=false;
            } else {
                selected_icon = None;
                match hit_window(&wins,&zorder,zlen,mcx,mcy){
                    Some((slot,1)) => { close_slot!(slot); }   // [X]
                    Some((slot,2)) => {                         // [_]
                        if let Some(w)=&mut wins[slot]{w.minimized=!w.minimized;}
                        bring_front!(slot);
                    }
                    Some((slot,5)) => {                         // [-] Maximize/Restore
                        if let Some(w)=&mut wins[slot]{w.toggle_maximize();}
                        bring_front!(slot);
                    }
                    Some((slot,4)) => {                         // Resize corner
                        bring_front!(slot);
                        if let Some(w)=&wins[slot] {
                            if !w.maximized {
                                mouse.resize_slot = Some(slot);
                            }
                        }
                    }
                    Some((slot,0)) => {                         // title bar drag
                        bring_front!(slot);
                        if is_double_click {
                            if let Some(w)=&mut wins[slot]{
                                w.toggle_maximize();
                            }
                            last_click_tick = 0;
                        } else {
                            if let Some(w)=&wins[slot]{
                                mouse.drag_slot=Some(slot);
                                mouse.drag_ox=mcx.saturating_sub(w.x);
                                mouse.drag_oy=mcy.saturating_sub(w.y);
                            }
                        }
                    }
                    Some((slot,_)) => { bring_front!(slot); }  // content click
                    None => {
                        // Click on taskbar
                        if mcy == H - 1 {
                            if mcx < 11 {
                                // ZiqaOS start button
                                menu_open = true;
                                menu_sel = 0;
                            } else {
                                // Hit-test taskbar window buttons
                                let mut bx = 12usize;
                                for i in 0..zlen {
                                    let slot = zorder[i];
                                    if let Some(w) = &wins[slot] {
                                        if w.minimized { continue; }
                                        let tlen = w.app.title().len().min(12);
                                        if mcx >= bx && mcx < bx + tlen + 2 {
                                            bring_front!(slot);
                                            break;
                                        }
                                        bx += tlen + 2;
                                    }
                                }
                            }
                        }
                        // Click on minimized strip (H-2)
                        else if mcy == H - 2 {
                            let mut tx = 0usize;
                            for i in 0..6 {
                                if let Some(w) = &wins[i] {
                                    if w.minimized {
                                        let tl = w.app.title().len().min(12);
                                        if mcx >= tx && mcx < tx + tl + 3 {
                                            if let Some(w) = &mut wins[i] { w.minimized = false; }
                                            bring_front!(i);
                                            break;
                                        }
                                        tx += tl + 3;
                                    }
                                }
                            }
                        }
                        // Click on desktop icon
                        else if let Some(idx) = hit_icon(mcx, mcy) {
                            if selected_icon == Some(idx) && is_double_click {
                                launch!(MENU_APPS[idx].0);
                                selected_icon = None;
                                last_click_tick = 0;
                            } else {
                                selected_icon = Some(idx);
                            }
                        } else {
                            selected_icon = None;
                            mouse.select_start = Some((mcx, mcy));
                        }
                    }
                }
            }
        }

        if mouse.btn {
            if let Some(slot)=mouse.drag_slot{
                if let Some(w)=&mut wins[slot]{
                    if w.maximized {
                        w.toggle_maximize();
                        w.x = mcx.saturating_sub(w.w / 2);
                        mouse.drag_ox = mcx.saturating_sub(w.x);
                        mouse.drag_oy = 0;
                    }
                    w.x=mcx.saturating_sub(mouse.drag_ox);
                    w.y=mcy.saturating_sub(mouse.drag_oy);
                    w.clamp();
                }
            }
            if let Some(slot) = mouse.resize_slot {
                if let Some(w) = &mut wins[slot] {
                    if !w.maximized {
                        let nw = mcx.saturating_sub(w.x).max(12);
                        let nh = mcy.saturating_sub(w.y).max(5);
                        w.w = nw.min(W - w.x);
                        w.h = nh.min(H - w.y);
                    }
                }
            }
        }
        if mouse.just_released() { 
            if let Some(slot) = mouse.drag_slot {
                if mcy == DESK_TOP {
                    if let Some(w) = &mut wins[slot] {
                        if !w.maximized {
                            w.toggle_maximize();
                        }
                    }
                }
            }
            mouse.drag_slot=None; 
            mouse.resize_slot=None;
            mouse.select_start=None;
        }

        // Right-click → context menu
        if mouse.rjust_pressed() && !menu_open {
            ctx.show(mcx, mcy);
        }

        // ── Keyboard ──────────────────────────────────────────────────────────
        if let Some(key)=read_key(){
            if show_help { show_help=false; }
            else if ctx.open { ctx.open=false; }
            else if menu_open {
                match key {
                    b'1'..=b'8' => { let idx=(key-b'1')as usize; if idx<MENU_APPS.len(){launch!(MENU_APPS[idx].0);} menu_open=false; }
                    b'k'|b'A'|0x80 => { menu_sel=menu_sel.saturating_sub(1); }
                    b'j'|b'B'|0x81 => { menu_sel=(menu_sel+1).min(MENU_APPS.len()-1); }
                    b'\r'|b'\n' => { launch!(MENU_APPS[menu_sel].0); menu_open=false; }
                    b' '|0x1B  => { menu_open=false; }
                    _ => {}
                }
            }
            else {
                let act=active_slot(&zorder,zlen);
                let act_app=act.and_then(|i|wins[i].as_ref()).map(|w|w.app);
                match key {
                    b'q'|0x1B => break,
                    b' ' => { menu_open=true; menu_sel=0; }
                    b'h' => { show_help=!show_help; }
                    b'\t' => {
                        if zlen>1{
                            let front=zorder[zlen-1];
                            for i in (1..zlen).rev(){zorder[i]=zorder[i-1];}
                            zorder[0]=front;
                        }
                    }
                    b'm' => { if let Some(i)=act{if let Some(w)=&mut wins[i]{w.minimized=!w.minimized;}}}
                    b'f' => { if let Some(i)=act{if let Some(w)=&mut wins[i]{w.toggle_maximize();}}}
                    b'x' => { if let Some(i)=act{ close_slot!(i); }}
                    b'1'..=b'8' => { launch!(MENU_APPS[(key-b'1')as usize].0); }
                    b'r' if act_app == Some(App::Snake) => { snake.reset(); }
                    b'w'|b'A'|0x80 if act_app == Some(App::Snake) => { if snake.dir.1 != 1 { snake.dir = (0, -1); } }
                    b's'|b'B'|0x81 if act_app == Some(App::Snake) => { if snake.dir.1 != -1 { snake.dir = (0, 1); } }
                    b'a'|b'D'|0x82 if act_app == Some(App::Snake) => { if snake.dir.0 != 1 { snake.dir = (-1, 0); } }
                    b'd'|b'C'|0x83 if act_app == Some(App::Snake) => { if snake.dir.0 != -1 { snake.dir = (1, 0); } }
                    b'A'|0x80 if act_app!=Some(App::Editor) => { if let Some(i)=act{if let Some(w)=&mut wins[i]{if w.y>DESK_TOP{w.y-=1;}}}}
                    b'B'|0x81 if act_app!=Some(App::Editor) => { if let Some(i)=act{if let Some(w)=&mut wins[i]{if w.y+w.h<=DESK_BOT{w.y+=1;}}}}
                    b'C'|0x83 if act_app!=Some(App::Editor) => { if let Some(i)=act{if let Some(w)=&mut wins[i]{if w.x+w.w<W{w.x+=1;}}}}
                    b'D'|0x82 if act_app!=Some(App::Editor) => { if let Some(i)=act{if let Some(w)=&mut wins[i]{if w.x>0{w.x-=1;}}}}
                    b'+' => { if let Some(i)=act{if let Some(w)=&mut wins[i]{if w.x+w.w+1<W{w.w+=1;}}}}
                    b'-' => { if let Some(i)=act{if let Some(w)=&mut wins[i]{if w.w>10{w.w-=1;}}}}
                    b'j' if act_app!=Some(App::Editor) && act_app!=Some(App::Snake) => { if let Some(i)=act{if let Some(w)=&mut wins[i]{w.scroll+=1;}}}
                    b'k' if act_app!=Some(App::Editor) && act_app!=Some(App::Snake) => { if let Some(i)=act{if let Some(w)=&mut wins[i]{w.scroll=w.scroll.saturating_sub(1);}}}
                    // Terminal
                    b'\r'|b'\n' if act_app==Some(App::Terminal) => {
                        let mut echo=[b' ';48]; echo[0]=b'$'; echo[1]=b' ';
                        let cb=tinp.as_str().as_bytes(); let n=cb.len().min(46);
                        echo[2..2+n].copy_from_slice(&cb[..n]);
                        tout.push(core::str::from_utf8(&echo[..2+n]).unwrap_or("$ "),0x07);
                        match tinp.as_str().trim(){
                            "help"  =>{ tout.push("  help clear date uptime ps ls neofetch zram",0x07); }
                            "clear" =>{ for _ in 0..30{tout.push("",0x07);} }
                            "date"  =>{ tout.push("  Fri May 30 00:00:00 UTC 2026",0x0B); }
                            "uptime"=>{ let u=tick; tout.push("  up 0h 0m (tick-based)",0x0B); let _=u; }
                            "ps"    =>{
                                tout.push("  PID STAT NAME",0x0F);
                                for &l in &["    1 S    init","    2 S    shell","    3 R    nwm_demo"]{
                                    tout.push(l,0x0A);
                                }
                            }
                            "ls"    =>{
                                tout.push("  bin/ dev/ etc/ proc/ tmp/",0x0E);
                                tout.push("  kernel.elf  init  .config  README.md",0x0F);
                            }
                            "neofetch" => {
                                tout.push("  ZiqaOS v0.1 | x86_64 | Rust+Zig",0x0B);
                                tout.push("  Kernel: ziqa-0.1.0-experimental",0x0B);
                                tout.push("  Shell:  34 cmds | FS: ZiqaFS",0x0B);
                                let (c, _, _) = crate::memory::compression::PAGE_STORE.get_stats();
                                let msg = alloc::format!("  ZRAM:   {} compressed pages", c);
                                tout.push(&msg, 0x0B);
                            }
                            "zram" => {
                                let (count, orig, comp) = crate::memory::compression::PAGE_STORE.get_stats();
                                let savings = orig.saturating_sub(comp);
                                let msg1 = alloc::format!("  Pages:           {} compressed", count);
                                let msg2 = alloc::format!("  Original size:   {} KB", orig / 1024);
                                let msg3 = alloc::format!("  Compressed size: {} KB", comp / 1024);
                                let msg4 = alloc::format!("  Saved memory:    {} KB", savings / 1024);
                                tout.push(&msg1, 0x07);
                                tout.push(&msg2, 0x07);
                                tout.push(&msg3, 0x07);
                                tout.push(&msg4, 0x0A);
                            }
                            "" => {}
                            _  =>{ tout.push("  command not found",0x0C); }
                        }
                        tinp.clear();
                        if let Some(i)=act{if let Some(w)=&mut wins[i]{w.scroll=0;}}
                    }
                    0x7F|8 if act_app==Some(App::Terminal) => { tinp.pop(); }
                    32..=126 if act_app==Some(App::Terminal) => { tinp.push(key); }
                    // Editor
                    b's' if act_app==Some(App::Editor) => { ed.dirty=false; }
                    b'\r'|b'\n' if act_app==Some(App::Editor) => { ed.newline(); }
                    0x7F|8 if act_app==Some(App::Editor) => { ed.backspace(); }
                    b'A'|0x80 if act_app==Some(App::Editor) => { ed.move_up(); }
                    b'B'|0x81 if act_app==Some(App::Editor) => { ed.move_down(); }
                    b'C'|0x83 if act_app==Some(App::Editor) => { ed.move_right(); }
                    b'D'|0x82 if act_app==Some(App::Editor) => { ed.move_left(); }
                    32..=126 if act_app==Some(App::Editor) => { ed.insert(key); }
                    _ => {}
                }
            }
        }

        // ── Render ────────────────────────────────────────────────────────────
        draw_wallpaper(&mut s, tick, selected_icon, mcx, mcy);
        draw_menubar(&mut s, tick, zlen, mcx, mcy);

        for i in 0..zlen{
            let slot=zorder[i];
            let active=i==zlen-1;
            if let Some(w)=&wins[slot]{
                draw_window(&mut s,w,active,tick,&tout,&tinp,&ed,&mut snake,mcx,mcy,mouse.btn);
            }
        }

        // Minimized strips above taskbar
        {
            let mut tx=0usize;
            for i in 0..6{
                if let Some(w)=&wins[i]{
                    if w.minimized{
                        let is_act=zlen>0&&zorder[zlen-1]==i;
                        let at=if is_act{C_TITLEHI}else{C_TITLE};
                        let t2=w.app.title(); let tl=t2.len().min(12);
                        if tx+tl+4<W{
                            s.put(tx,H-2,b'[',at);
                            s.print(tx+1,H-2,&t2[..tl],at);
                            s.put(tx+1+tl,H-2,b']',at);
                            tx+=tl+3;
                        }
                    }
                }
            }
        }

        draw_taskbar(&mut s,&wins,&zorder,zlen,tick);
        if menu_open { draw_startmenu(&mut s,menu_sel); }
        if ctx.open  { draw_ctxmenu(&mut s,&ctx); }
        if show_help { draw_help(&mut s); }

        // Desktop selection box
        if let Some(start) = mouse.select_start {
            draw_selection(&mut s, start, (mcx, mcy));
            
            let sx = start.0.min(mcx);
            let sy = start.1.min(mcy);
            let sw = start.0.max(mcx) - sx;
            let sh = start.1.max(mcy) - sy;
            
            let icon_pos = &[(2,2),(2,6),(2,10),(2,14),(2,18),(12,2),(12,6),(12,10)];
            selected_icon = None;
            for (i, &(ix, iy)) in icon_pos.iter().enumerate() {
                let icon_w = 8;
                let icon_h = 5;
                if sx < ix + icon_w && sx + sw > ix && sy < iy + icon_h && sy + sh > iy {
                    selected_icon = Some(i);
                    break;
                }
            }
        }

        // Mouse cursor — drawn last so it's always on top
        draw_cursor(&mut s, &mouse, &wins, &zorder, zlen);

        s.flush();
        tick=tick.wrapping_add(1);
        frame_delay(16);
    }

    for y in 0..H{s.fill(0,y,W,1,b' ',0x07);}
    s.flush();
}
