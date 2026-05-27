# كتاب الأنوية الشامل
## من التاريخ إلى المستقبل — دليل بناء ZiqaKernel

> **ملاحظة للـ AI Agent:** هذا الكتاب مرجع تقني شامل لبناء نواة تجريبية اسمها ZiqaKernel
> مكتوبة بلغة Rust، تعمل على معمارية x86_64، للتشغيل على QEMU.
> كل فصل يحتوي على النظرية + الخوارزمية + كود Rust جاهز للتنفيذ.

---

# الجزء الأول: التاريخ والأساسيات

## الفصل 1: تاريخ أنظمة التشغيل والأنوية

### 1.1 الحقبة الأولى (1950s) — لا يوجد نظام تشغيل
في البداية، لم يكن هناك ما يُسمى "نظام تشغيل". المبرمج كان يكتب برنامجه مباشرة
على العتاد (Bare Metal Programming). كل برنامج يتحكم في الجهاز بالكامل.

**المشكلة:** إذا أخطأ البرنامج، الجهاز بالكامل يتوقف. ولا يمكن تشغيل أكثر من برنامج واحد.

### 1.2 الحقبة الثانية (1960s) — Batch Systems
IBM طورت أول أنظمة Batch حيث يتم تحميل برامج متعددة وتنفيذها بالتسلسل.
ظهر مفهوم **Monitor** — برنامج بسيط يدير تنفيذ البرامج الأخرى.
هذا المونيتور هو الجد الأول للكيرنال الحديث.

### 1.3 MULTICS (1965) — أب أنظمة التشغيل الحديثة
مشروع مشترك بين MIT وBell Labs وGeneral Electric.
أول نظام يطبق:
- **Protection Rings** — مستويات الحماية
- **Virtual Memory** — الذاكرة الافتراضية
- **Multi-user** — تعدد المستخدمين

رغم أن MULTICS فشل تجارياً لتعقيده، إلا أنه أرسى كل المفاهيم الحديثة.

### 1.4 UNIX (1969) — الثورة الحقيقية
Ken Thompson وDennis Ritchie في Bell Labs كتبا UNIX كنظام مبسط مستوحى من MULTICS.
القرارات الذكية في UNIX:
- كل شيء ملف (Everything is a File)
- برامج صغيرة تتعاون (Unix Philosophy)
- كُتب بلغة C بدل Assembly — قابلية النقل للمنصات المختلفة

### 1.5 Linux (1991) — الكيرنال المفتوح
Linus Torvalds، طالب فنلندي عمره 21 سنة، أعلن على Usenet:
> "أعمل على نظام تشغيل حر (هواية فقط، لن يكون كبيراً أو محترفاً...)."

اليوم Linux يشغّل:
- 96% من أقوى 500 سيرفر في العالم
- 100% من محطات الفضاء (ISS)
- 70%+ من الهواتف الذكية (Android)
- معظم البنية التحتية للإنترنت

### 1.6 النقاش التاريخي: Torvalds vs Tanenbaum (1992)
أندرو تانينباوم (مخترع MINIX) هاجم لينكس علناً قائلاً إن معماريته المتجانسة
(Monolithic) خطوة للوراء، والـ Microkernel هو المستقبل.

لينوس رد: "الأداء العملي أهم من الأناقة النظرية."

التاريخ أثبت أن لينوس كان محقاً من الناحية العملية —
لكن تانينباوم كان محقاً من الناحية النظرية.
هذا التوتر هو ما يدفع أبحاث الأنوية حتى اليوم.

---

## الفصل 2: أنواع الأنوية ومقارنتها

### 2.1 Monolithic Kernel (النواة المتجانسة)
```
+------------------------------------------+
|              User Space                   |
|   [App1]  [App2]  [App3]  [Browser]      |
+------------------------------------------+
|              Kernel Space (Ring 0)        |
|  [Scheduler][Memory][FS][Drivers][Net]   |
+------------------------------------------+
|              Hardware                     |
+------------------------------------------+
```

**الأمثلة:** Linux, FreeBSD, OpenBSD

**المبدأ:** كل خدمات النظام تعمل في مساحة ذاكرة واحدة بأعلى صلاحية.

**المميزات:**
- أداء خارق — لا حاجة لنقل بيانات بين مساحات
- نضج وموثوقية عالية (Linux عمره 30+ سنة)
- دعم واسع للعتاد

**العيوب:**
- خطأ في تعريف (Driver) قد يسقط النظام كله (Kernel Panic)
- الكود كبير ومعقد (Linux: 35 مليون سطر)
- صعب التحقق الرسمي من صحته

### 2.2 Microkernel (النواة الصغيرة)
```
+------------------------------------------+
|              User Space                   |
|  [App1] [FileServer] [DriverServer] [Net]|
|   كل الخدمات كعمليات عادية              |
+------------------------------------------+
|     Kernel (Ring 0) — أصغر ما يمكن      |
|     [IPC] [Memory] [Scheduling فقط]     |
+------------------------------------------+
```

**الأمثلة:** Mach, MINIX, QNX, GNU Hurd, seL4

**المبدأ:** النواة تحتوي فقط على الحد الأدنى الضروري. كل شيء آخر في User Space.

**المميزات:**
- أمان عالي — خطأ في Driver لا يسقط النظام
- قابل للتحقق الرسمي (seL4 مثبت رياضياً)
- مناسب للأنظمة الحساسة (طيران، فضاء، طبي)

**العيوب:**
- أبطأ بسبب كثرة Context Switches
- IPC overhead مرتفع
- GNU Hurd بدأ 1990 ولم يكتمل حتى اليوم

### 2.3 Hybrid Kernel (النواة الهجينة)
```
+------------------------------------------+
|              User Space                   |
|   [App1]  [App2]  [Some Drivers]        |
+------------------------------------------+
|              Kernel Space                 |
|   [Core Services] [Critical Drivers]    |
|   [Microkernel Base + Monolithic Parts] |
+------------------------------------------+
```

**الأمثلة:** Windows NT, macOS (XNU), DragonFlyBSD

**المبدأ:** محاولة الجمع بين أداء Monolithic وأمان Microkernel.
النقد الأكاديمي: هجين يعني أخذت عيوب الاثنين لا مميزاتهما.
الرد العملي: Windows وmacOS يعملان بشكل ممتاز في الواقع.

### 2.4 Exokernel (نواة الخروج)
```
+------------------------------------------+
|   [App + LibOS]  [DB + LibOS]  [Web]    |
|   كل تطبيق يبني نظام ملفاته الخاص      |
+------------------------------------------+
|   Exokernel — يضمن فقط عدم التعارض     |
|   لا تجريد، لا نظام ملفات، لا إخفاء   |
+------------------------------------------+
```

**الأمثلة:** Aegis, XOK (MIT), Xok

**المبدأ:** النواة لا تخفي العتاد — فقط تضمن المشاركة الآمنة.

**الفائدة:** قواعد البيانات وبعض التطبيقات الحرجة يمكنها تحسين أداء I/O
بشكل مذهل لأنها تتحكم مباشرة في كيفية الوصول للقرص.

### 2.5 Unikernel (النواة الأحادية)
```
+------------------------------------------+
|   [Application + Kernel = ملف واحد]    |
|   يعمل مباشرة على VM أو Hardware       |
+------------------------------------------+
```

**الأمثلة:** MirageOS, Unikraft, IncludeOS, OSv

**المبدأ:** دمج التطبيق مع الكيرنال في ملف تنفيذي واحد.

**المميزات:**
- إقلاع بأجزاء من الثانية
- مساحة ضئيلة جداً (أحياناً أقل من 1MB)
- مثالي للـ Microservices والـ Cloud Functions

**العيوب:**
- تطبيق واحد فقط
- لا يصلح للحاسوب الشخصي
- صعب التطوير والتصحيح

### 2.6 جدول المقارنة الشامل

| المعيار | Monolithic | Micro | Hybrid | Exo | Uni |
|---------|-----------|-------|--------|-----|-----|
| الأداء | ★★★★★ | ★★★ | ★★★★ | ★★★★★ | ★★★★★ |
| الأمان | ★★★ | ★★★★★ | ★★★★ | ★★★ | ★★★★ |
| التعقيد | متوسط | عالي | عالي | عالي جداً | منخفض |
| مثال واقعي | Linux | QNX | Windows | — | Unikraft |
| مناسب لـ | كل شيء | حساس | Desktop | DB | Cloud |

---

# الجزء الثاني: خوارزميات الكيرنال الأساسية

## الفصل 3: جدولة العمليات (Process Scheduling)

### 3.1 المشكلة الجوهرية
معالج بـ 8 أنوية يشغّل 500 عملية في وقت واحد.
من يعمل الآن؟ لكم من الوقت؟ ماذا بعد؟
هذا عمل الـ Scheduler.

### 3.2 مفاهيم أساسية قبل الخوارزميات

**Process States (حالات العملية):**
```
New → Ready → Running → Waiting → Ready → Running → Terminated
                ↑                    ↓
            Scheduler            I/O, Sleep, Event
```

**معايير التقييم:**
- **CPU Utilization:** نسبة استخدام المعالج (نريده 100%)
- **Throughput:** عدد العمليات المنتهية في الوحدة الزمنية
- **Turnaround Time:** الوقت من البداية للنهاية
- **Waiting Time:** وقت الانتظار في قائمة Ready
- **Response Time:** الوقت حتى أول استجابة
- **Fairness:** كل عملية تحصل على نصيبها العادل

### 3.3 خوارزمية FCFS (First Come First Served)
الأبسط: من يصل أولاً يُخدَّم أولاً.

```
العمليات:  P1(burst=24ms)  P2(burst=3ms)  P3(burst=3ms)
الترتيب:   P1 → P2 → P3

Timeline:  |--P1(24ms)--|--P2(3ms)--|--P3(3ms)--|
           0            24          27          30

Waiting:   P1=0, P2=24, P3=27
Average Waiting = (0+24+27)/3 = 17ms  ← سيء جداً
```

**المشكلة:** Convoy Effect — P2 وP3 ينتظران طويلاً بسبب P1.
**لا تُستخدم في الأنظمة الحديثة بمفردها.**

```rust
// FCFS في Rust
use std::collections::VecDeque;

struct Process {
    id: u32,
    burst_time: u32,
    arrival_time: u32,
}

struct FCFSScheduler {
    queue: VecDeque<Process>,
}

impl FCFSScheduler {
    fn new() -> Self {
        FCFSScheduler { queue: VecDeque::new() }
    }

    fn add_process(&mut self, p: Process) {
        self.queue.push_back(p);
    }

    fn run(&mut self) {
        let mut current_time = 0u32;
        while let Some(process) = self.queue.pop_front() {
            println!("Running P{} at time {}", process.id, current_time);
            current_time += process.burst_time;
            println!("P{} finished at time {}", process.id, current_time);
        }
    }
}
```

### 3.4 خوارزمية SJF (Shortest Job First)
نُنفذ العملية ذات الوقت الأقصر أولاً.

```
العمليات:  P1(6ms)  P2(8ms)  P3(7ms)  P4(3ms)
بعد الترتيب: P4(3) → P1(6) → P3(7) → P2(8)

Average Waiting = (3+16+9+0)/4 = 7ms  ← أفضل من FCFS
```

**المشكلة الجوهرية:** كيف نعرف طول العملية مسبقاً؟
الحل: التنبؤ بالـ Burst التالي بناءً على التاريخ.

```
τ(n+1) = α × t(n) + (1-α) × τ(n)
حيث:
τ(n+1) = التنبؤ التالي
t(n)   = الـ Burst الفعلي الأخير
α      = عامل التعلم (عادة 0.5)
```

```rust
struct SJFPredictor {
    alpha: f64,
    predicted_burst: f64,
}

impl SJFPredictor {
    fn new(alpha: f64, initial: f64) -> Self {
        SJFPredictor { alpha, predicted_burst: initial }
    }

    fn update(&mut self, actual_burst: f64) -> f64 {
        self.predicted_burst = self.alpha * actual_burst 
                             + (1.0 - self.alpha) * self.predicted_burst;
        self.predicted_burst
    }
}
```

### 3.5 خوارزمية Round Robin (RR) — الأكثر استخداماً
كل عملية تحصل على quantum محدد من الوقت ثم تعود للطابور.

```
Quantum = 4ms
العمليات: P1(24ms)  P2(3ms)  P3(3ms)

Timeline:
|P1(4)|P2(3)|P3(3)|P1(4)|P1(4)|P1(4)|P1(4)|P1(4)|
0     4     7     10    14    18    22    26    30

نتيجة: P2 تنتهي في 7ms، P3 في 10ms، P1 في 30ms
```

**اختيار الـ Quantum أمر حرج:**
- قصير جداً → Context Switching overhead عالي
- طويل جداً → يصبح مثل FCFS

Linux يستخدم quantum يتراوح بين 10ms و200ms بحسب أولوية العملية.

```rust
use std::collections::VecDeque;

struct RRScheduler {
    queue: VecDeque<Process>,
    quantum: u32,
}

impl RRScheduler {
    fn new(quantum: u32) -> Self {
        RRScheduler { queue: VecDeque::new(), quantum }
    }

    fn run(&mut self) {
        let mut current_time = 0u32;
        
        while !self.queue.is_empty() {
            let mut process = self.queue.pop_front().unwrap();
            let run_time = self.quantum.min(process.burst_time);
            
            println!("Running P{} for {}ms at time {}", 
                     process.id, run_time, current_time);
            
            current_time += run_time;
            process.burst_time -= run_time;
            
            if process.burst_time > 0 {
                // لم تنتهِ — أرجعها للطابور
                self.queue.push_back(process);
            } else {
                println!("P{} FINISHED at time {}", process.id, current_time);
            }
        }
    }
}
```

### 3.6 خوارزمية CFS (Completely Fair Scheduler) — قلب Linux
هذه الخوارزمية التي يستخدمها Linux منذ 2007، مصممة بواسطة Ingo Molnár.

**المفهوم الأساسي:** بدل الـ Time Slices الثابتة، نتتبع **vruntime** (الوقت الافتراضي)
لكل عملية ودائماً نشغّل من لديه أقل vruntime.

```
vruntime يزداد بشكل عكسي مع الأولوية:
- أولوية عالية → vruntime يزداد ببطء → تحصل على وقت أكثر
- أولوية منخفضة → vruntime يزداد بسرعة → تحصل على وقت أقل

الـ Red-Black Tree مرتبة بحسب vruntime:
        [P3: vruntime=100]
       /                 \
[P1: vruntime=50]    [P5: vruntime=200]
                    /
            [P4: vruntime=150]

دائماً نشغّل أقصى يسار الشجرة (P1 في هذا المثال)
```

```rust
use std::collections::BTreeMap;

struct CFSScheduler {
    // مرتبة بحسب vruntime تلقائياً
    runqueue: BTreeMap<u64, Process>,
    target_latency: u64,  // نريد كل عملية تعمل خلال هذه الفترة
}

impl CFSScheduler {
    fn new(target_latency: u64) -> Self {
        CFSScheduler {
            runqueue: BTreeMap::new(),
            target_latency,
        }
    }

    fn pick_next(&mut self) -> Option<(u64, Process)> {
        // أخذ العملية ذات أقل vruntime (أقصى يسار الشجرة)
        self.runqueue.pop_first()
    }

    fn time_slice(&self, process_count: usize) -> u64 {
        // كل عملية تحصل على حصة متساوية من target_latency
        let min_granularity = 1; // minimum 1ms
        (self.target_latency / process_count as u64).max(min_granularity)
    }

    fn enqueue(&mut self, vruntime: u64, process: Process) {
        self.runqueue.insert(vruntime, process);
    }
}
```

### 3.7 Multilevel Feedback Queue (MLFQ) — الخوارزمية الذكية
تُستخدم في macOS وWindows وبعض أنظمة Linux.

**المبدأ:**
```
Queue 0 (أعلى أولوية، quantum صغير=8ms): للعمليات التفاعلية
Queue 1 (أولوية متوسطة، quantum=16ms)
Queue 2 (أولوية منخفضة، quantum=32ms): للعمليات الحسابية الطويلة

قواعد:
1. عملية جديدة → Queue 0 (نفترض أنها تفاعلية)
2. إذا انتهى quantum ولم تنتهِ → تنزل لـ Queue أدنى
3. إذا تركت CPU بنفسها (انتظار I/O) → ترجع لـ Queue أعلى
4. كل فترة زمنية → كل العمليات ترجع لـ Queue 0 (منع الجوع)
```

```rust
struct MLFQScheduler {
    queues: Vec<VecDeque<Process>>,
    quantums: Vec<u32>,  // [8, 16, 32, ...]
    boost_period: u32,   // كل كم ms نعمل Priority Boost
}

impl MLFQScheduler {
    fn new(levels: usize) -> Self {
        let quantums = (0..levels)
            .map(|i| 8u32 * 2u32.pow(i as u32))
            .collect();
        
        MLFQScheduler {
            queues: vec![VecDeque::new(); levels],
            quantums,
            boost_period: 1000, // كل ثانية
        }
    }

    fn pick_next(&mut self) -> Option<(usize, Process)> {
        for (level, queue) in self.queues.iter_mut().enumerate() {
            if let Some(process) = queue.pop_front() {
                return Some((level, process));
            }
        }
        None
    }

    fn priority_boost(&mut self) {
        // كل العمليات ترجع لأعلى قائمة — منع الجوع
        let all_processes: Vec<Process> = self.queues
            .iter_mut()
            .skip(1)
            .flat_map(|q| q.drain(..))
            .collect();
        
        for p in all_processes {
            self.queues[0].push_back(p);
        }
    }
}
```

---

## الفصل 4: إدارة الذاكرة (Memory Management)

### 4.1 المشكلة الجوهرية
كل عملية تعتقد أنها تملك الذاكرة كاملة. لكن الحقيقة أن مئات العمليات
تتشارك نفس الذاكرة الفيزيائية. كيف؟

### 4.2 Virtual Memory — الوهم الجميل
```
العملية ترى:              الواقع الفيزيائي:
0x0000 → 0xFFFF          قد يكون في:
                          - RAM الفعلية
                          - القرص الصلب (Swap)
                          - نفس المنطقة لعملية أخرى (Shared)
                          - غير موجود أصلاً (Lazy Allocation)
```

### 4.3 Paging — آلية التطبيق
الذاكرة مقسمة لـ Pages (صفحات) بحجم ثابت (عادة 4KB).

```
Virtual Address (48-bit في x86_64):
Bits [47:39] → PML4 Index  (مستوى 1)
Bits [38:30] → PDPT Index  (مستوى 2)
Bits [29:21] → PD Index    (مستوى 3)
Bits [20:12] → PT Index    (مستوى 4)
Bits [11:0]  → Page Offset (الموضع داخل الصفحة)
```

```rust
// Page Table Entry في x86_64
#[repr(transparent)]
struct PageTableEntry(u64);

impl PageTableEntry {
    const PRESENT:     u64 = 1 << 0;   // الصفحة موجودة في RAM
    const WRITABLE:    u64 = 1 << 1;   // يمكن الكتابة
    const USER:        u64 = 1 << 2;   // User Space يمكنه الوصول
    const ACCESSED:    u64 = 1 << 5;   // تم الوصول إليها
    const DIRTY:       u64 = 1 << 6;   // تم التعديل
    const NO_EXECUTE:  u64 = 1 << 63;  // لا يمكن تنفيذها

    fn new(physical_addr: u64, flags: u64) -> Self {
        // العنوان يجب أن يكون محاذياً على 4KB
        assert!(physical_addr & 0xFFF == 0);
        PageTableEntry(physical_addr | flags)
    }

    fn physical_address(&self) -> u64 {
        self.0 & 0x000FFFFF_FFFFF000  // mask للعنوان فقط
    }

    fn is_present(&self) -> bool {
        self.0 & Self::PRESENT != 0
    }
}
```

### 4.4 خوارزميات استبدال الصفحات (Page Replacement)

عندما تمتلئ الذاكرة ونحتاج صفحة جديدة، أي صفحة نُخرج للـ Swap؟

**أ) LRU (Least Recently Used) — الأكثر استخداماً:**
```
أخرج الصفحة التي لم تُستخدم منذ أطول وقت.

تاريخ الوصول: [1, 2, 3, 4, 2, 1, 5, 6, 2, 1]
ذاكرة بسعة 3 صفحات:

الوصول | RAM          | Page Fault?
1      | [1]          | نعم
2      | [1,2]        | نعم
3      | [1,2,3]      | نعم
4      | [4,2,3]      | نعم (أخرجنا 1 لأنه الأقدم)
2      | [4,2,3]      | لا
1      | [1,2,3]      | نعم (أخرجنا 4)
5      | [1,2,5]      | نعم (أخرجنا 3)
```

```rust
use std::collections::HashMap;

struct LRUCache {
    capacity: usize,
    map: HashMap<u64, usize>,  // page → last_access_time
    time: usize,
}

impl LRUCache {
    fn new(capacity: usize) -> Self {
        LRUCache { capacity, map: HashMap::new(), time: 0 }
    }

    fn access(&mut self, page: u64) -> bool {
        self.time += 1;
        
        if self.map.contains_key(&page) {
            // Page Hit — فقط حدّث وقت الوصول
            self.map.insert(page, self.time);
            return false; // لا Page Fault
        }
        
        // Page Miss — Page Fault
        if self.map.len() >= self.capacity {
            // أخرج الصفحة الأقل استخداماً
            let &evict_page = self.map
                .iter()
                .min_by_key(|(_, &t)| t)
                .unwrap().0;
            self.map.remove(&evict_page);
        }
        
        self.map.insert(page, self.time);
        true // Page Fault حصل
    }
}
```

**ب) Clock Algorithm — تقريب فعّال لـ LRU:**
خوارزمية الساعة هي تقريب رخيص لـ LRU تستخدمه معظم الأنوية الحقيقية.

```
الذاكرة كدائرة:
[P1:ref=1] → [P2:ref=0] → [P3:ref=1] → [P4:ref=0]
      ↑
   مؤشر الساعة

عند الحاجة لاستبدال:
- إذا ref=0 → استبدل هذه الصفحة
- إذا ref=1 → اجعل ref=0 وتحرك للتالي
```

```rust
struct ClockReplacer {
    frames: Vec<(u64, bool)>,  // (page, reference_bit)
    hand: usize,
}

impl ClockReplacer {
    fn new(capacity: usize) -> Self {
        ClockReplacer {
            frames: Vec::with_capacity(capacity),
            hand: 0,
        }
    }

    fn find_victim(&mut self) -> usize {
        loop {
            let (_, ref_bit) = &mut self.frames[self.hand];
            if !*ref_bit {
                let victim = self.hand;
                self.hand = (self.hand + 1) % self.frames.len();
                return victim;
            }
            *ref_bit = false;
            self.hand = (self.hand + 1) % self.frames.len();
        }
    }
}
```

### 4.5 خوارزميات تخصيص الذاكرة (Memory Allocation)

**أ) Buddy System — نظام الشريك:**
```
لديك 64 كيلوبايت:

طلب 7KB:
64KB → 32KB + 32KB
32KB → 16KB + 16KB
16KB → 8KB + 8KB
→ خصّص 8KB (أقرب قوة لـ 2 أكبر من 7)

الشجرة:
        [64KB]
       /      \
   [32KB]    [32KB]
   /    \
[16KB] [16KB]
        /   \
     [8KB] [8KB←مخصص]
```

```rust
struct BuddyAllocator {
    free_lists: Vec<Vec<usize>>,  // free_lists[n] = قائمة كتل بحجم 2^n
    base_addr: usize,
    total_size: usize,
}

impl BuddyAllocator {
    fn allocate(&mut self, size: usize) -> Option<usize> {
        let order = self.size_to_order(size);
        
        // ابحث عن أقرب كتلة حرة
        for o in order..self.free_lists.len() {
            if let Some(addr) = self.free_lists[o].pop() {
                // قسّم الكتل الكبيرة
                let mut current_order = o;
                let mut current_addr = addr;
                
                while current_order > order {
                    current_order -= 1;
                    let buddy = current_addr + (1 << current_order);
                    self.free_lists[current_order].push(buddy);
                }
                
                return Some(current_addr);
            }
        }
        None
    }

    fn free(&mut self, addr: usize, order: usize) {
        let mut current_addr = addr;
        let mut current_order = order;
        
        // دمج مع الشريك إذا كان حراً
        while current_order < self.free_lists.len() - 1 {
            let buddy = self.buddy_address(current_addr, current_order);
            
            if let Some(pos) = self.free_lists[current_order]
                                   .iter().position(|&a| a == buddy) {
                self.free_lists[current_order].remove(pos);
                current_addr = current_addr.min(buddy);
                current_order += 1;
            } else {
                break;
            }
        }
        
        self.free_lists[current_order].push(current_addr);
    }

    fn buddy_address(&self, addr: usize, order: usize) -> usize {
        addr ^ (1 << order)  // XOR للحصول على الشريك
    }

    fn size_to_order(&self, size: usize) -> usize {
        let mut order = 0;
        let mut s = 1;
        while s < size { s <<= 1; order += 1; }
        order
    }
}
```

**ب) Slab Allocator — ما يستخدمه Linux:**
للكائنات الصغيرة المتكررة (مثل process descriptors)، Buddy System مهدر.
الـ Slab يخصص مسبقاً مجموعات من الكائنات بنفس الحجم.

```rust
struct Slab<T> {
    objects: Vec<Option<T>>,
    free_indices: Vec<usize>,
}

impl<T: Default> Slab<T> {
    fn new(capacity: usize) -> Self {
        Slab {
            objects: (0..capacity).map(|_| None).collect(),
            free_indices: (0..capacity).rev().collect(),
        }
    }

    fn allocate(&mut self) -> Option<usize> {
        self.free_indices.pop()
    }

    fn free(&mut self, index: usize) {
        self.objects[index] = None;
        self.free_indices.push(index);
    }
}
```

---

## الفصل 5: التواصل بين العمليات (IPC — Inter-Process Communication)

### 5.1 أنواع IPC

**أ) Pipes:**
```
العملية A → [Buffer] → العملية B
أحادي الاتجاه، بسيط، سريع
مثال: ls | grep txt
```

**ب) Message Queues:**
```
A → [Queue: msg1, msg2, msg3] → B أو C أو D
غير متزامن، قابل للترتيب
```

**ج) Shared Memory — الأسرع:**
```
A ←→ [منطقة ذاكرة مشتركة] ←→ B
لا نسخ، مباشر، لكن يحتاج Synchronization
```

**د) Signals:**
```
Kernel → [SIGTERM] → Process
للإشعارات الفورية، محدود المعلومات
```

### 5.2 مشكلة Synchronization والحل
عندما تشارك عمليتان ذاكرة، قد يحدث Race Condition:

```
Thread A: x = x + 1    Thread B: x = x + 1
الأصل: x = 0

بدون Synchronization:
A يقرأ x=0, B يقرأ x=0, A يكتب x=1, B يكتب x=1
النتيجة: x=1 (مفروض تكون 2!)
```

**الحل — Mutex:**
```rust
use std::sync::{Arc, Mutex};
use std::thread;

fn shared_memory_example() {
    let counter = Arc::new(Mutex::new(0u32));
    let mut handles = vec![];

    for _ in 0..10 {
        let counter = Arc::clone(&counter);
        let handle = thread::spawn(move || {
            let mut num = counter.lock().unwrap(); // يقفل
            *num += 1;
            // يفتح تلقائياً عند خروج num من النطاق
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    println!("Result: {}", *counter.lock().unwrap()); // دائماً 10
}
```

**الحل المتقدم — Lock-Free بـ Atomics:**
```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

struct LockFreeCounter {
    value: AtomicU64,
}

impl LockFreeCounter {
    fn increment(&self) {
        // Compare-and-Swap: يكرر حتى ينجح بدون قفل
        let mut old = self.value.load(Ordering::Relaxed);
        loop {
            match self.value.compare_exchange_weak(
                old, old + 1, Ordering::SeqCst, Ordering::Relaxed
            ) {
                Ok(_) => break,
                Err(x) => old = x,
            }
        }
    }
}
```

---

## الفصل 6: أنظمة الملفات (File Systems)

### 6.1 هيكل القرص
```
القرص مقسم لـ Blocks (عادة 4KB):

[Boot Sector][Super Block][Inode Table][Data Blocks...]

Super Block: معلومات نظام الملفات (عدد الـ Inodes، الـ Blocks، إلخ)
Inode: معلومات الملف (الحجم، التواريخ، أين البيانات)
Data Blocks: البيانات الفعلية
```

### 6.2 Inode — روح الملف
```rust
#[repr(C)]
struct Inode {
    mode: u16,          // نوع الملف وصلاحياته
    uid: u16,           // المالك
    gid: u16,           // المجموعة
    size: u64,          // الحجم بالبايت
    atime: u64,         // آخر وصول
    mtime: u64,         // آخر تعديل
    ctime: u64,         // آخر تغيير للـ metadata
    links_count: u16,   // عدد الروابط الصلبة
    
    // روابط للبيانات (ext4):
    direct: [u32; 12],     // 12 × 4KB = 48KB مباشرة
    indirect: u32,          // يشير لـ block يحتوي مزيداً من العناوين
    double_indirect: u32,   // مستوى إضافي
    triple_indirect: u32,   // للملفات الضخمة جداً
}
```

### 6.3 الـ Journaling — منع فساد البيانات
```
بدون Journaling (FAT32):
إذا انقطع الكهرباء أثناء الكتابة → ملفات فاسدة إلى الأبد

مع Journaling (ext4, NTFS):
1. اكتب العملية في الـ Journal أولاً
2. نفّذ العملية الفعلية
3. احذف من الـ Journal

إذا انقطع الكهرباء:
- عند الإقلاع، اقرأ الـ Journal وأكمل أو تراجع
- النظام دائماً في حالة متسقة
```

### 6.4 خوارزمية B-Tree للدليل
Linux يستخدم B-Tree (HTree في ext4) للـ Directories الكبيرة:

```
البحث عن ملف:
بدل فحص كل الملفات خطياً O(n)
B-Tree يعطيك O(log n)

للدليل بـ 1,000,000 ملف:
Linear: 1,000,000 مقارنة
B-Tree: ~20 مقارنة فقط
```

---

## الفصل 7: المقاطعات والـ System Calls

### 7.1 Interrupt Descriptor Table (IDT)
```
IDT: جدول يربط كل رقم مقاطعة بمعالجها

[0]  → Division by Zero Handler
[1]  → Debug Handler
[2]  → NMI Handler
...
[14] → Page Fault Handler
[32] → Timer Interrupt Handler
[33] → Keyboard Interrupt Handler
...
[128]→ System Call Handler (Linux يستخدم 0x80)
```

```rust
// تعريف IDT في Rust
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};

static mut IDT: InterruptDescriptorTable = InterruptDescriptorTable::new();

pub fn init_idt() {
    unsafe {
        IDT.breakpoint.set_handler_fn(breakpoint_handler);
        IDT.page_fault.set_handler_fn(page_fault_handler);
        IDT[32].set_handler_fn(timer_handler);  // PIC IRQ0
        IDT[33].set_handler_fn(keyboard_handler); // PIC IRQ1
        IDT.load();
    }
}

extern "x86-interrupt" fn timer_handler(stack_frame: InterruptStackFrame) {
    // يُستدعى كل ~10ms
    scheduler_tick();  // أعطِ الـ Scheduler فرصة
    pic_send_eoi();    // أخبر PIC أننا انتهينا
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64
) {
    let fault_addr = x86_64::registers::control::Cr2::read();
    
    // هل هي صفحة صالحة لكنها في Swap؟
    if let Some(frame) = swap_lookup(fault_addr) {
        load_from_swap(fault_addr, frame);
        return;  // استمر التنفيذ
    }
    
    // خطأ حقيقي — أوقف العملية
    panic!("Segmentation Fault at {:?}", fault_addr);
}
```

### 7.2 System Calls — البوابة الرسمية للـ Kernel

```
التطبيق (Ring 3):
1. ضع رقم الـ syscall في RAX
2. الوسائط في RDI, RSI, RDX, R10, R8, R9
3. نفّذ SYSCALL instruction

الكيرنال (Ring 0):
4. احفظ سياق المستخدم
5. نفّذ الخدمة المطلوبة
6. ضع النتيجة في RAX
7. ارجع لـ Ring 3

مثال — write():
RAX = 1 (رقم syscall لـ write)
RDI = 1 (file descriptor: stdout)
RSI = pointer للبيانات
RDX = عدد الـ bytes
SYSCALL
```

```rust
// جدول System Calls في ZiqaKernel
type SyscallHandler = fn(u64, u64, u64, u64, u64, u64) -> i64;

static SYSCALL_TABLE: &[SyscallHandler] = &[
    sys_read,    // 0
    sys_write,   // 1
    sys_open,    // 2
    sys_close,   // 3
    sys_exit,    // 60
    // ...
];

pub extern "C" fn syscall_dispatch(
    num: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64
) -> i64 {
    if num as usize >= SYSCALL_TABLE.len() {
        return -38; // ENOSYS
    }
    SYSCALL_TABLE[num as usize](a1, a2, a3, a4, a5, 0)
}

fn sys_write(fd: u64, buf: u64, count: u64, _: u64, _: u64, _: u64) -> i64 {
    // تحقق من أن المؤشر في نطاق User Space
    if buf < 0x1000 || buf > 0x7FFFFFFFFFFF {
        return -14; // EFAULT
    }
    
    let data = unsafe {
        core::slice::from_raw_parts(buf as *const u8, count as usize)
    };
    
    match fd {
        1 => { /* stdout */ console_write(data); count as i64 },
        2 => { /* stderr */ console_write(data); count as i64 },
        _ => -9, // EBADF
    }
}
```

---

# الجزء الثالث: التقنيات الحديثة التي تغير مستقبل الأنوية

## الفصل 8: eBPF — البرمجة داخل الكيرنال بأمان

### 8.1 ما هو eBPF
eBPF (extended Berkeley Packet Filter) هو نظام يسمح بتشغيل كود في Kernel Space
لكن مع ضمانات أمان صارمة عبر **Verifier** رياضي.

```
بدون eBPF:
إضافة ميزة للكيرنال → كتابة Driver → patch لينكس → انتظار سنوات للقبول
إذا فيه bug → Kernel Panic

مع eBPF:
كتابة برنامج eBPF → Verifier يفحصه → يعمل مباشرة في الكيرنال بأمان
```

### 8.2 كيف يعمل الـ Verifier
```
برنامج eBPF يدخل الـ Verifier:

✓ لا loops لا نهائية (يحسب حد أعلى للتعقيد)
✓ لا وصول لذاكرة خارج النطاق (يتتبع كل pointer)
✓ لا unsafe function calls
✓ جميع code paths تصل لـ return

إذا نجح → JIT Compilation → يعمل بسرعة Native Code
إذا فشل → rejected بدون تنفيذ أي شيء
```

### 8.3 استخدامات eBPF الحقيقية
```
- Cloudflare: يصفّي DDoS attacks بـ eBPF → 10 مليون packet/sec
- Facebook: شبكة كاملة مبنية على eBPF (Katran)
- Kubernetes: Cilium يستبدل iptables بالكامل بـ eBPF
- Netflix: profiling الأداء بدون أي overhead
- Linux Security Modules: قواعد أمان مخصصة
```

### 8.4 eBPF في ZiqaKernel
```rust
// محاكاة Verifier بسيط في Rust

#[derive(Debug, Clone)]
enum BPFInstruction {
    LoadImm { dst: u8, value: i64 },
    Add { dst: u8, src: u8 },
    Mov { dst: u8, src: u8 },
    Jmp { offset: i16 },
    JmpIfEqual { reg: u8, val: i64, offset: i16 },
    Call { func: u32 },
    Exit,
}

struct BPFVerifier {
    instructions: Vec<BPFInstruction>,
    max_complexity: usize,
}

impl BPFVerifier {
    fn verify(&self) -> Result<(), &'static str> {
        // فحص 1: يجب أن ينتهي بـ Exit
        if !matches!(self.instructions.last(), Some(BPFInstruction::Exit)) {
            return Err("Program must end with Exit");
        }
        
        // فحص 2: لا loops (تحليل بسيط)
        if self.instructions.len() > self.max_complexity {
            return Err("Program too complex");
        }
        
        // فحص 3: جميع الـ jumps ضمن الحدود
        for (i, instr) in self.instructions.iter().enumerate() {
            if let BPFInstruction::Jmp { offset } = instr {
                let target = i as i64 + *offset as i64;
                if target < 0 || target >= self.instructions.len() as i64 {
                    return Err("Jump out of bounds");
                }
                // منع backward jumps (منع loops)
                if *offset <= 0 {
                    return Err("Backward jumps not allowed");
                }
            }
        }
        
        Ok(())
    }
}
```

---

## الفصل 9: io_uring — ثورة الـ I/O

### 9.1 المشكلة مع الـ I/O التقليدي
```
تطبيق يريد قراءة 1000 ملف:

التقليدي:
1. syscall read() → انتقال Ring 3→Ring 0 (مكلف)
2. انتظار I/O (blocking)
3. ارجع → انتقال Ring 0→Ring 3 (مكلف)
4. كرر 1000 مرة

= 2000 Context Switch + 1000 × وقت الانتظار = بطيء جداً
```

### 9.2 io_uring — الحل الذكي
```
io_uring يخلق Shared Memory Ring بين User Space وKernel:

[Submission Ring] ← التطبيق يضع الطلبات هنا
[Completion Ring] ← الكيرنال يضع النتائج هنا

التطبيق يضع 1000 طلب دفعة واحدة بدون syscalls متعددة
الكيرنال ينفذها بالتوازي
التطبيق يقرأ النتائج من الـ Completion Ring

النتيجة: 2 syscalls فقط بدل 2000!
```

```rust
// هيكل io_uring مبسط
#[repr(C)]
struct SubmissionQueueEntry {
    opcode: u8,      // نوع العملية (read, write, accept...)
    flags: u8,
    fd: i32,         // file descriptor
    addr: u64,       // عنوان البيانات
    len: u32,        // الحجم
    user_data: u64,  // معرف خاص للتطبيق
}

#[repr(C)]
struct CompletionQueueEntry {
    user_data: u64,  // نفس user_data من الطلب
    res: i32,        // النتيجة (bytes read/written أو error code)
    flags: u32,
}

struct IoUring {
    sq: *mut SubmissionQueueEntry,
    cq: *mut CompletionQueueEntry,
    sq_head: *mut u32,
    sq_tail: *mut u32,
    cq_head: *mut u32,
    cq_tail: *mut u32,
    sq_size: u32,
    cq_size: u32,
}

impl IoUring {
    fn submit(&mut self, entry: SubmissionQueueEntry) {
        unsafe {
            let tail = *self.sq_tail;
            let index = (tail % self.sq_size) as usize;
            self.sq.add(index).write(entry);
            *self.sq_tail = tail + 1;
            // Atomic fence لضمان الترتيب
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        }
    }

    fn get_completion(&mut self) -> Option<CompletionQueueEntry> {
        unsafe {
            if *self.cq_head == *self.cq_tail {
                return None; // لا نتائج بعد
            }
            let head = *self.cq_head;
            let entry = self.cq.add((head % self.cq_size) as usize).read();
            *self.cq_head = head + 1;
            Some(entry)
        }
    }
}
```

---

## الفصل 10: CHERI — أمان الذاكرة من العتاد

### 10.1 المشكلة الجوهرية
90% من ثغرات الأمان في الكيرنالات هي أخطاء ذاكرة:
- Buffer Overflow
- Use-After-Free
- Null Pointer Dereference
- Type Confusion

هذه الأخطاء موجودة لأن الـ Pointer في C مجرد رقم — لا حدود، لا صلاحيات.

### 10.2 CHERI — الـ Fat Pointer العتادي
```
Pointer تقليدي (64-bit):
[عنوان 64-bit فقط]

CHERI Capability (128-bit):
[عنوان 64-bit | حدود الوصول | الصلاحيات | Tag bit]

المعالج يفحص عند كل وصول:
✓ هل العنوان ضمن الحدود؟
✓ هل الصلاحية (read/write/execute) صحيحة؟
✓ هل الـ Tag bit صحيح؟ (منع Forgery)

إذا فشل أي فحص → Exception فوري من العتاد
لا overhead برمجي — الفحص بالتوازي مع تنفيذ التعليمة
```

### 10.3 محاكاة CHERI Capabilities في Rust
```rust
// بما أن Rust يعمل على x86 (لا CHERI)، نحاكيه برمجياً
#[derive(Debug, Clone)]
struct Capability {
    base: usize,     // بداية المنطقة المسموحة
    top: usize,      // نهاية المنطقة المسموحة
    cursor: usize,   // العنوان الحالي
    permissions: u8, // READ=1, WRITE=2, EXECUTE=4
    tag: bool,       // صحيح = Capability حقيقية، خطأ = مزورة
}

const READ:    u8 = 0b001;
const WRITE:   u8 = 0b010;
const EXECUTE: u8 = 0b100;

impl Capability {
    fn new(base: usize, size: usize, perms: u8) -> Self {
        Capability {
            base,
            top: base + size,
            cursor: base,
            permissions: perms,
            tag: true,
        }
    }

    fn check_bounds(&self, addr: usize, size: usize) -> Result<(), &'static str> {
        if !self.tag {
            return Err("CHERI: Untagged capability — possible forgery");
        }
        if addr < self.base || addr + size > self.top {
            return Err("CHERI: Out-of-bounds access");
        }
        Ok(())
    }

    fn read(&self, offset: usize, size: usize) -> Result<Vec<u8>, &'static str> {
        if self.permissions & READ == 0 {
            return Err("CHERI: No read permission");
        }
        self.check_bounds(self.cursor + offset, size)?;
        // الوصول الفعلي للذاكرة
        let data = unsafe {
            core::slice::from_raw_parts((self.cursor + offset) as *const u8, size)
        };
        Ok(data.to_vec())
    }

    fn narrow(&self, new_base: usize, new_size: usize) -> Result<Self, &'static str> {
        // يمكن تضييق الـ Capability لكن لا يمكن توسيعها
        if new_base < self.base || new_base + new_size > self.top {
            return Err("CHERI: Cannot widen capability");
        }
        Ok(Capability {
            base: new_base,
            top: new_base + new_size,
            cursor: new_base,
            permissions: self.permissions,
            tag: true,
        })
    }
}
```

---

## الفصل 11: CXL — مستقبل الذاكرة

### 11.1 ما هو CXL
Compute Express Link — بروتوكول ربط عالي السرعة يسمح للـ CPUs وGPUs
والمعجّلات بمشاركة ذاكرة موحدة بـ Cache Coherency مضمونة من العتاد.

### 11.2 البروتوكولات الثلاثة
```
CXL.io:    نقل البيانات وإدارة الأجهزة (مثل PCIe لكن أذكى)
CXL.cache: الجهاز يخزن مؤقتاً في ذاكرة الـ CPU ← Cache Coherent
CXL.memory: الـ CPU يصل لذاكرة الجهاز الخارجي ← Cache Coherent
```

### 11.3 تأثير CXL على الكيرنال
```
الآن:
Memory Manager في الكيرنال يدير كل شيء

مع CXL:
العتاد (CXL Controller) يضمن الاتساق تلقائياً
الكيرنال يحتاج فقط إدارة الـ Topology، لا الاتساق

النتيجة: وظائف كاملة من الكيرنال تنتقل للعتاد
```

### 11.4 Memory Pooling بـ CXL 3.0
```
بدل:  Server1[128GB] + Server2[128GB] = سيرفران محبوسان

مع CXL Switch:
Server1 ←─┐
Server2 ←─┼─→ CXL Memory Pool [256GB مشترك]
Server3 ←─┘
GPU     ←─┘

كل جهاز يأخذ ما يحتاج ديناميكياً
لا هدر، لا نقص، لا نسخ للبيانات
```

---

# الجزء الرابع: ما بعد الكيرنال — المستقبل

## الفصل 12: Capability OS — نظام القدرات

### 12.1 الفلسفة
بدل نظام الإذن التقليدي (ACL):
```
"المستخدم A مسموح له يقرأ /home/data.txt"
مشكلة: أي كود يعمل بصلاحيات A يرث كل إذن A
```

نظام القدرات (Capabilities):
```
"هذا الكود يملك capability مشفرة للقراءة من هذا الملف فقط"
لا يمكن تزويرها، لا يمكن نسخها بدون إذن صريح
```

### 12.2 seL4 — الكيرنال المثبت رياضياً
seL4 هو الكيرنال الوحيد الذي:
- مثبت رياضياً أنه خالٍ من Deadlocks
- مثبت أن الـ Isolation صحيح بالكامل
- يستخدمه الجيش الأمريكي وـ NASA وـ DARPA
- مكتوب بـ 8,700 سطر C فقط (لينكس: 35 مليون)

### 12.3 هيكل Capability System في ZiqaKernel
```rust
// كل كائن في النظام يُدار بـ Capability
#[derive(Debug, Clone, Copy, PartialEq)]
struct CapabilityToken {
    id: u64,         // معرف فريد
    rights: u32,     // الصلاحيات
    checksum: u32,   // للتحقق من الأصالة
}

const RIGHT_READ:    u32 = 1 << 0;
const RIGHT_WRITE:   u32 = 1 << 1;
const RIGHT_EXECUTE: u32 = 1 << 2;
const RIGHT_GRANT:   u32 = 1 << 3; // حق منح الـ capability لآخرين

struct CapabilitySpace {
    table: HashMap<u64, CapabilityToken>,
    next_id: u64,
}

impl CapabilitySpace {
    fn create(&mut self, rights: u32) -> CapabilityToken {
        let id = self.next_id;
        self.next_id += 1;
        
        let token = CapabilityToken {
            id,
            rights,
            checksum: self.compute_checksum(id, rights),
        };
        
        self.table.insert(id, token);
        token
    }

    fn derive(&self, token: &CapabilityToken, new_rights: u32) 
        -> Result<CapabilityToken, &'static str> {
        // يمكن فقط تضييق الصلاحيات، لا توسيعها
        if new_rights & !token.rights != 0 {
            return Err("Cannot escalate privileges");
        }
        if token.rights & RIGHT_GRANT == 0 {
            return Err("No grant permission");
        }
        Ok(CapabilityToken {
            id: token.id,
            rights: new_rights,
            checksum: self.compute_checksum(token.id, new_rights),
        })
    }

    fn verify(&self, token: &CapabilityToken) -> bool {
        self.compute_checksum(token.id, token.rights) == token.checksum
    }

    fn compute_checksum(&self, id: u64, rights: u32) -> u32 {
        // في الواقع يستخدم HMAC أو مشابه
        (id.wrapping_mul(0xDEADBEEF) ^ rights as u64) as u32
    }
}
```

---

## الفصل 13: Orthogonal Persistence — إلغاء نظام الملفات

### 13.1 الفكرة
```
الآن:
Process تعيش في الذاكرة (سريعة لكن مؤقتة)
Data تعيش في الملفات (بطيئة لكن دائمة)
البرنامج مسؤول عن النقل بينهما

مع Orthogonal Persistence:
لا فرق بين الذاكرة والملف
كل شيء في فضاء ذاكرة موحد ودائم تلقائياً
البرنامج لا يعرف حتى الفرق
```

### 13.2 يناسب العتاد الحديث
```
NVM (Non-Volatile Memory) مثل Intel Optane:
- سرعة RAM (أبطأ قليلاً)
- ثبات القرص
- يصلح للـ Persistent Address Space

مع CXL: يمكن ربطه لشبكة كاملة
```


---

## الفصل 13م: معمارية التطبيقات وتشغيل البرامج الجاهزة (ABI as a Plugin)

سؤال ممتاز — وهو يكشف شيئاً مهماً:

---

### أولاً: لماذا مشكلتك صعبة فعلاً

```
"تشغيل أي تطبيق بدون دعم يدوي"

التطبيقات تتكلم مع الكيرنال عبر ABI (Application Binary Interface)
ABI = لغة التواصل الرسمية بين App والكيرنال

Linux Apps  → تتكلم Linux ABI (300 syscall)
Windows Apps → تتكلم Win32 API
macOS Apps  → تتكلم Mach-O + BSD syscalls

إذا كيرنالك لا يفهم أي ABI → لا شيء يعمل
```

المشكلة ليست "كل تطبيق بحاله" — المشكلة هي **أي ABI تختار**.

---

### ثانياً: الفكرة الذكية — ABI كـ Plugin

بدل أن تختار ABI واحد وتحبس نفسك فيه، اجعل الـ ABI نفسه قابلاً للتحميل:

```
┌─────────────────────────────────────────────┐
│              USER SPACE                      │
│  [Linux ELF]  [WASM App]  [Future Binary]  │
└──────┬──────────────┬──────────────┬────────┘
       │              │              │
       ▼              ▼              ▼
┌─────────────────────────────────────────────┐
│           ABI PLUGIN LAYER                   │
│  [linux_abi]  [wasm_abi]  [custom_abi]      │
│   ↑ كل plugin مفحوص بـ eBPF Verifier        │
└──────────────────┬──────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────────┐
│           ZIQA KERNEL CORE                   │
│  Memory │ Scheduler │ Capabilities │ Drivers │
└─────────────────────────────────────────────┘
```

**الذكاء هنا:** الكيرنال لا يعرف شيئاً عن Linux أو WASM. فقط يوفر primitives. الـ ABI Plugin هو من يترجم.

---

### ثالثاً: للتطبيق الفعلي الآن — الحل الواقعي

للوصول لـ "انزّل من الموقع ويشتغل" تحتاج ثلاث طبقات:

#### الطبقة 1: Linux ABI Plugin (يعطيك كل Linux apps دفعة واحدة)

```rust
// هذا ليس "دعم يدوي لكل app"
// هذا دعم واحد يغطي كل Linux apps

// الـ ELF Loader يقرأ أي binary Linux
fn load_elf(binary: &[u8]) -> Result<Process, Error> {
    let elf = ElfFile::parse(binary)?;
    
    // detect ABI من الـ ELF header
    let abi = match elf.os_abi() {
        0x00 => ABI::Linux,
        0x09 => ABI::FreeBSD,
        _    => ABI::Unknown,
    };
    
    // حمّل الـ plugin المناسب
    let plugin = abi_registry.load(abi)?;
    plugin.setup_process(elf)
}

// الـ syscall dispatcher يمرر لـ plugin المناسب
fn handle_syscall(ctx: &SyscallContext) -> i64 {
    let process_abi = current_process().abi;
    abi_registry.dispatch(process_abi, ctx)
}
```

#### الطبقة 2: WASM Runtime Plugin (مستقبل التطبيقات)

```rust
// أي موقع يوفر .wasm → يشتغل مباشرة
fn load_wasm(binary: &[u8]) -> Result<Process, Error> {
    let module = WasmModule::parse(binary)?;
    
    // WASI = Linux syscalls نظيفة للـ WASM
    // 50 syscall فقط تغطي 90% من الاحتياجات
    let wasi = WasiPlugin::new();
    wasi.instantiate(module)
}
```

#### الطبقة 3: الـ Stack الكامل للتنزيل

```
Website → [Network Stack] → [TLS] → [HTTP] → [Binary Detector]
                                                      │
                              ┌───────────────────────┘
                              ▼
                    هل هو ELF؟    → Linux ABI Plugin
                    هل هو WASM؟   → WASM Plugin
                    هل هو PE؟     → Wine-style Plugin (لاحقاً)
```

---

### رابعاً: ترتيب التنفيذ العملي

```
المرحلة 1 (الآن):
└── Linux ABI Plugin بسيط
    ├── ELF Loader
    ├── 50 syscall أساسية (read, write, mmap, fork, exec...)
    └── النتيجة: busybox، bash، python تشتغل

المرحلة 2:
└── Network Stack
    ├── smoltcp (مكتبة Rust صغيرة لـ TCP/IP)
    ├── VirtIO-net driver للـ QEMU
    └── النتيجة: wget، curl تشتغل → تنزيل فعلي

المرحلة 3:
└── Display Stack
    ├── Framebuffer أو VirtIO-GPU
    ├── Wayland-lite protocol
    └── النتيجة: تطبيقات رسومية

المرحلة 4:
└── WASM Plugin
    └── النتيجة: أي موقع يوفر .wasm يشتغل مباشرة
```

---

### خامساً: لماذا هذا أذكى من فكرة Gemini

| Gemini اقترح | مشكلته | حلنا |
|------------|---------|------|
| eBPF Syscall VM | معقد جداً للبداية | ABI Plugin أبسط وأوضح |
| User-Space Drivers | صعب التنفيذ | VirtIO جاهز في QEMU |
| Orthogonal Persistence | لا علاقة له بـ 3D | نؤجله لمرحلة لاحقة |

**الفرق الجوهري:** فكرتنا تبدأ بشيء يشتغل غداً، ثم تتطور. فكرة Gemini كلها نظرية لا يمكن تشغيلها الآن.

---

### أول خطوة فعلية الآن

```bash
# أضف هذا لـ Cargo.toml
xmas-elf = "0.9"      # ELF parser
smoltcp = "0.11"      # TCP/IP stack  
```

```rust
// في process/elf_loader.rs
use xmas_elf::ElfFile;

pub fn execute(binary: &[u8]) -> Result<(), KernelError> {
    let elf = ElfFile::new(binary)
        .map_err(|_| KernelError::InvalidElf)?;
    
    // تحقق أنه Linux x86_64
    assert_eq!(elf.header.pt2.machine().as_machine(), 
               xmas_elf::header::Machine::X86_64);
    
    // حمّل الـ segments في الذاكرة
    for program_header in elf.program_iter() {
        if program_header.get_type()? == xmas_elf::program::Type::Load {
            map_segment(&program_header)?;
        }
    }
    
    // ابدأ التنفيذ من الـ entry point
    jump_to_userspace(elf.header.pt2.entry_point())
}
```

هذا الـ ELF Loader وحده يعطيك القدرة على تشغيل أي Linux binary بدون أن تعرف شيئاً عن التطبيق نفسه.

---

# الجزء الخامس: ZiqaKernel — خطة البناء الكاملة

## الفصل 14: مواصفات ZiqaKernel

### 14.1 نظرة عامة
```
الاسم:        ZiqaKernel (نواة ذاكية تجريبية)
اللغة:        Rust (بدون std)
المعمارية:    x86_64
بيئة التشغيل: QEMU (تطوير) → Hardware (لاحقاً)
النوع:        Monolithic مع مبادئ Capability
الهدف:        تعليمي + استكشاف مستقبل الأنوية
```

### 14.2 المكونات الأساسية
```
ZiqaKernel/
├── src/
│   ├── main.rs          ← Entry point، لا std
│   ├── arch/
│   │   └── x86_64/
│   │       ├── boot.s   ← Assembly للـ bootstrap
│   │       ├── gdt.rs   ← Global Descriptor Table
│   │       ├── idt.rs   ← Interrupt Descriptor Table
│   │       └── paging.rs← Page Tables
│   ├── memory/
│   │   ├── frame.rs     ← Physical Frame Allocator (Buddy)
│   │   ├── heap.rs      ← Kernel Heap (Slab)
│   │   └── vmm.rs       ← Virtual Memory Manager
│   ├── process/
│   │   ├── mod.rs       ← Process Control Block
│   │   ├── scheduler.rs ← CFS Scheduler
│   │   └── syscall.rs   ← System Call Handler
│   ├── capability/
│   │   └── mod.rs       ← Capability System (تجريبي)
│   ├── ebpf/
│   │   └── verifier.rs  ← eBPF Verifier (مبسط)
│   ├── drivers/
│   │   ├── uart.rs      ← Serial (للـ debugging)
│   │   ├── vga.rs       ← VGA Text Mode
│   │   └── pic.rs       ← Programmable Interrupt Controller
│   └── fs/
│       └── ramfs.rs     ← RAM-based File System بسيط
├── Cargo.toml
├── linker.ld            ← Linker Script
└── .cargo/config.toml   ← Build configuration
```

### 14.3 Cargo.toml
```toml
[package]
name = "ziqa-kernel"
version = "0.1.0"
edition = "2021"

[dependencies]
bootloader = "0.11"
x86_64 = "0.14"
uart_16550 = "0.3"
pic8259 = "0.10"
pc-keyboard = "0.7"
spin = "0.9"          # Mutex بدون OS
lazy_static = "1.0"

[dependencies.linked_list_allocator]
version = "0.10"

[profile.dev]
panic = "abort"        # لا stack unwinding في الكيرنال

[profile.release]
panic = "abort"
opt-level = 3
```

### 14.4 main.rs — نقطة الدخول
```rust
#![no_std]      // لا std library
#![no_main]     // نحدد entry point بأنفسنا
#![feature(abi_x86_interrupt)]

use bootloader::{BootInfo, entry_point};
use core::panic::PanicInfo;

// تسجيل kernel_main كـ entry point
entry_point!(kernel_main);

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    ziqa_kernel::init(boot_info);
    
    println!("╔══════════════════════════════╗");
    println!("║   ZIQA Kernel v0.1 — مرحباً   ║");
    println!("║   نواة تجريبية عراقية         ║");
    println!("╚══════════════════════════════╝");
    
    // تشغيل الـ shell البسيط
    ziqa_kernel::shell::run();
    
    // لا نصل هنا — الكيرنال لا يتوقف
    loop { x86_64::instructions::hlt(); }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("\n[ZIQA PANIC] {}", info);
    loop { x86_64::instructions::hlt(); }
}
```

### 14.5 خطوات البناء خطوة بخطوة

**المرحلة 0 — الإعداد:**
```bash
# تثبيت Rust nightly (الكيرنال يحتاج ميزات تجريبية)
rustup override set nightly
rustup target add x86_64-unknown-none
cargo install bootimage

# تثبيت QEMU
pacman -S qemu-system-x86  # Arch Linux
```

**المرحلة 1 — Hello World من Ring 0 (يوم 1):**
```
الهدف: طباعة نص على الشاشة من الكيرنال
التحقق: يعمل على QEMU
```

**المرحلة 2 — الذاكرة (أسبوع 1):**
```
Physical Frame Allocator (Buddy Algorithm)
Virtual Memory + Page Tables
Kernel Heap (Slab Allocator)
التحقق: يمكن تخصيص وتحرير الذاكرة بشكل صحيح
```

**المرحلة 3 — العمليات (أسبوع 2):**
```
Process Control Block
Context Switching
Round Robin Scheduler (بسيط)
System Calls الأساسية
التحقق: يمكن تشغيل برنامجين بالتوازي
```

**المرحلة 4 — المقاطعات (أسبوع 2-3):**
```
IDT Setup
Timer Interrupt (PIT)
Keyboard Interrupt
Page Fault Handler
التحقق: الكيرنال يستجيب للوحة المفاتيح
```

**المرحلة 5 — نظام الملفات (أسبوع 3-4):**
```
RAM FS بسيط
VFS Layer بسيط
System Calls: open, read, write, close
التحقق: يمكن قراءة وكتابة الملفات
```

**المرحلة 6 — الميزات التجريبية (لاحقاً):**
```
Capability System مبسط
eBPF Verifier بسيط
io_uring-style async I/O
```

### 14.6 أوامر التطوير اليومية
```bash
# بناء وتشغيل على QEMU
cargo run

# تشغيل مع debugging
cargo run -- -s -S
# ثم في terminal آخر: gdb target/x86_64-unknown-none/debug/ziqa-kernel

# تشغيل الاختبارات
cargo test

# تشغيل على hardware حقيقي (USB)
dd if=target/x86_64-unknown-none/debug/boot-ziqa-kernel.img \
   of=/dev/sdX bs=512
```

---

## الفصل 15: مصادر وقراءات إضافية

### 15.1 الكتب المرجعية

**للبداية:**
- "Writing an OS in Rust" — Philipp Oppermann (مجاني: os.phil-opp.com)
  أفضل مصدر لبدء كيرنال بـ Rust خطوة بخطوة

- "Operating System Concepts" — Silberschatz (الكتاب الديناصور)
  المرجع الأكاديمي الكلاسيكي، الفصول 1-9 أساسية

**للتعمق:**
- "Modern Operating Systems" — Tanenbaum
  نفس تانينباوم صاحب MINIX والنقاش مع لينوس

- "The Linux Kernel Development" — Robert Love
  كيف يعمل لينكس من الداخل

**للمتقدمين:**
- "seL4 Reference Manual" — مجاني من sel4.systems
  النواة المثبتة رياضياً

### 15.2 المصادر الإلكترونية
```
os.phil-opp.com          ← البداية المثالية بـ Rust
wiki.osdev.org           ← موسوعة OS development
github.com/torvalds/linux← كود لينكس الأصلي
ebpf.io                  ← مصدر eBPF الرسمي
```

### 15.3 أنوية تعليمية للدراسة
```
xv6 (MIT):     كيرنال بسيط وجميل، 10,000 سطر C
               يُدرَّس في معظم جامعات العالم
               github.com/mit-pdos/xv6-public

seL4:          الكيرنال المثبت رياضياً
               github.com/seL4/seL4

Redox OS:      كيرنال Microkernel كامل بـ Rust
               gitlab.redox-os.org/redox-os/kernel
```

---

# ملحق: خلاصة للـ AI Agent

## التعليمات للـ Agent

أنت مطلوب منك بناء ZiqaKernel خطوة بخطوة.

**الأولويات:**
1. ابدأ بـ Philipp Oppermann's blog كمرجع رئيسي
2. كل مرحلة يجب أن تُختبر على QEMU قبل الانتقال للتالية
3. استخدم `cargo test` بعد كل ميزة
4. أضف comments بالعربي لكل دالة مهمة

**القيود:**
- لا `std` في كود الكيرنال — فقط `core` و`alloc`
- لا `unwrap()` في Kernel Space — استخدم `match` أو `if let`
- كل unsafe block يجب أن يكون موثقاً بتعليق يشرح لماذا هو آمن
- Kernel Panic = اطبع المعلومات + `hlt loop`، لا تعود أبداً

**المعمارية المستهدفة:**
```
Monolithic Kernel
+ Capability System (طبقة فوق Monolithic)
+ eBPF-style safe extensions
+ io_uring-style async I/O
```

**ابدأ بهذا الأمر:**
```bash
cargo new ziqa-kernel --bin
cd ziqa-kernel
rustup override set nightly
rustup target add x86_64-unknown-none
cargo add bootloader x86_64 uart_16550
```

ثم اتبع المراحل من 1 إلى 6 بالترتيب.

---

*ZiqaKernel — مشروع تجريبي لاستكشاف مستقبل الأنوية*
*كتب بمحبة في الكوفة، العراق — 2026*
