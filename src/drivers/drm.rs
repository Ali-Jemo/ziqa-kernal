use crate::println;
/// DRM (Direct Rendering Manager) / KMS (Kernel Mode Setting) Driver for ZiqaKernel
///
/// Provides the core DRM ioctls needed for Wayland compositors like Hyprland:
/// - DRM_IOCTL_MODE_FB_CREATE: Create framebuffer objects
/// - DRM_IOCTL_MODE_FB_DESTROY: Destroy framebuffer objects
/// - DRM_IOCTL_MODE_PAGE_FLIP: Page flipping for vsync
/// - DRM_IOCTL_MODE_GETRESOURCES: Enumerate CRTCs, connectors, encoders
use spin::Mutex;

pub const DRM_DRIVER_NAME: &str = "card0";

/// DRM ioctl command numbers (Linux DRM_IOCTL_BASE = 'd')
pub mod ioctl {
    /// Get driver version
    pub const GET_VERSION: u64 = 0x0000640b;
    /// Get card name
    pub const GET_CARD_NAME: u64 = 0x0000640c;
    /// Get unique name
    pub const GET_UNIQUE_NAME: u64 = 0x0000640d;

    // Mode setting ioctls
    pub const MODE_GETRESOURCES: u64 = 0xc0046401;
    pub const MODE_GETPLANE: u64 = 0xc0406402;

    // Framebuffer ioctls
    pub const MODE_FB_CREATE: u64 = 0xc0286417;
    pub const MODE_FB_DESTROY: u64 = 0x80046418;

    /// Page flip ioctl
    pub const MODE_PAGE_FLIP: u64 = 0xc0206407;
    /// Damage ioctl – placeholder value for tracking damaged regions
    pub const MODE_DAMAGE: u64 = 0x4504;
}

/// Framebuffer object identifier
pub type FramebufferId = u32;
pub type CrtcId = u32;

/// Pixel format for framebuffer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrmFormat {
    XRGB8888,
    ARGB8888,
    RGB565,
}

impl DrmFormat {
    pub fn bytes_per_pixel(&self) -> usize {
        match self {
            DrmFormat::XRGB8888 | DrmFormat::ARGB8888 => 4,
            DrmFormat::RGB565 => 2,
        }
    }
}

/// A DRM framebuffer object
#[derive(Debug, Clone, Copy)]
pub struct DrmFramebuffer {
    pub id: FramebufferId,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub format: DrmFormat,
    pub vma_addr: u64, // Virtual memory address of backing store
}

/// Display resource enumeration
#[derive(Debug, Clone, Copy)]
pub struct DrmResources {
    pub crtc_id: CrtcId,
    pub connector_id: u32,
    pub width: u32,
    pub height: u32,
}
/// Rectangle used for damage tracking. Mirrors userspace layout.
#[repr(C)]
pub struct DrmRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Damage descriptor passed from userspace.
#[repr(C)]
pub struct DrmDamage {
    pub rects: *const DrmRect,
    pub count: u32,
}

/// DRM device state
pub struct DrmDevice {
    /// Available framebuffers
    framebuffers: [Option<DrmFramebuffer>; MAX_FB],
    /// Current front buffer (for page flipping)
    front_buffer: Option<FramebufferId>,
    /// Display resources
    resources: DrmResources,
    /// Framebuffer counter for IDs
    fb_counter: FramebufferId,
}

const MAX_FB: usize = 16;

impl DrmDevice {
    pub const fn new() -> Self {
        Self {
            framebuffers: [None; MAX_FB],
            front_buffer: None,
            resources: DrmResources {
                crtc_id: 1,
                connector_id: 1,
                width: 1920,
                height: 1080,
            },
            fb_counter: 1,
        }
    }

    /// Create a new framebuffer
    pub fn create_framebuffer(
        &mut self,
        width: u32,
        height: u32,
        format: DrmFormat,
        vma_addr: u64,
    ) -> Result<FramebufferId, &'static str> {
        // Find free slot
        let slot = self
            .framebuffers
            .iter_mut()
            .find(|fb| fb.is_none())
            .ok_or("No framebuffer slots available")?;

        let id = self.fb_counter;
        self.fb_counter = self.fb_counter.wrapping_add(1);

        *slot = Some(DrmFramebuffer {
            id,
            width,
            height,
            pitch: width * format.bytes_per_pixel() as u32,
            format,
            vma_addr,
        });

        // First FB becomes front buffer
        if self.front_buffer.is_none() {
            self.front_buffer = Some(id);
        }

        println!("[DRM] Created framebuffer {} ({}x{})", id, width, height);
        Ok(id)
    }

    /// Destroy a framebuffer
    pub fn destroy_framebuffer(&mut self, fb_id: FramebufferId) -> Result<(), &'static str> {
        let removed = self
            .framebuffers
            .iter_mut()
            .find(|fb| fb.as_ref().map(|f| f.id) == Some(fb_id))
            .ok_or("Framebuffer not found")?;

        *removed = None;
        println!("[DRM] Destroyed framebuffer {}", fb_id);
        Ok(())
    }

    /// Get framebuffer by ID
    pub fn get_framebuffer(&self, fb_id: FramebufferId) -> Option<&DrmFramebuffer> {
        self.framebuffers
            .iter()
            .find(|fb| fb.as_ref().map(|f| f.id) == Some(fb_id))
            .and_then(|fb| fb.as_ref())
    }

    /// Queue a page flip to the given framebuffer
    pub fn queue_page_flip(&mut self, fb_id: FramebufferId) -> Result<bool, &'static str> {
        // Validate framebuffer exists
        if self.get_framebuffer(fb_id).is_none() {
            return Err("Framebuffer not found");
        }

        self.front_buffer = Some(fb_id);
        println!("[DRM] Page flipped to framebuffer {}", fb_id);
        Ok(true)
    }

    /// Get display resources
    pub fn get_resources(&self) -> &DrmResources {
        &self.resources
    }
}

pub static DRM: Mutex<DrmDevice> = Mutex::new(DrmDevice::new());

/// Handle DRM ioctl from userspace.
///
/// Maps DRM ioctls to `DrmDevice` methods. Userspace passes a pointer to
/// the ioctl struct; we read/write fields directly.
pub fn handle_ioctl(cmd: u64, arg: *mut u8) -> Result<i64, &'static str> {
    let mut drm = DRM.lock();

    match cmd {
        // ── MODE_GETRESOURCES (0xc0046401) ────────────────────────────────────
        // struct drm_mode_card_res { fb_id_ptr, crtc_id_ptr, connector_id_ptr,
        //   encoder_id_ptr, count_fbs, count_crtcs, count_connectors,
        //   count_encoders, min_width, max_width, min_height, max_height }
        ioctl::MODE_GETRESOURCES => {
            unsafe {
                if arg.is_null() {
                    return Err("NULL arg");
                }
                let res = drm.get_resources();
                // Write at the right offsets inside drm_mode_card_res
                // fb_id_ptr (u64 at +0), crtc_id_ptr (u64 at +8)
                // connector_id_ptr (u64 at +16), encoder_id_ptr (u64 at +24)
                // count_fbs (u32 at +32), count_crtcs (u32 at +36)
                // count_connectors (u32 at +40), count_encoders (u32 at +44)
                // min/max width/height (u32 at +48..+60)
                core::ptr::write((arg as *mut u64).add(4), 1); // count_crtcs
                core::ptr::write((arg as *mut u64).add(5), 1); // count_connectors
                core::ptr::write((arg as *mut u64).add(6), 0); // count_fbs = filled below
                core::ptr::write((arg as *mut u64).add(7), 0); // count_encoders
                core::ptr::write((arg as *mut u32).add(12), res.width.min(4096));   // min_width
                core::ptr::write((arg as *mut u32).add(13), res.width);              // max_width
                core::ptr::write((arg as *mut u32).add(14), res.height.min(4096));   // min_height
                core::ptr::write((arg as *mut u32).add(15), res.height);             // max_height
            }
            Ok(0)
        }

        // ── MODE_FB_CREATE (0xc0286417) ───────────────────────────────────────
        // struct drm_mode_fb_cmd {
        //   fb_id: u32, width: u32, height: u32, pitch: u32,
        //   bpp: u32, depth: u32, handle: u32   (28 bytes)
        // }
        ioctl::MODE_FB_CREATE => {
            unsafe {
                if arg.is_null() {
                    return Err("NULL arg");
                }
                let width  = core::ptr::read((arg as *const u32).add(1));
                let height = core::ptr::read((arg as *const u32).add(2));
                let bpp    = core::ptr::read((arg as *const u32).add(4));
                // Map bpp → DrmFormat
                let format = match bpp {
                    32 => DrmFormat::XRGB8888,
                    16 => DrmFormat::RGB565,
                    _  => return Err("Unsupported bpp"),
                };
                let fb_id = drm.create_framebuffer(width, height, format, 0)?;
                // Write back fb_id to first field
                core::ptr::write(arg as *mut u32, fb_id);
            }
            Ok(0)
        }

        // ── MODE_FB_DESTROY (0x80046418) ─────────────────────────────────────
        // struct { fb_id: u32 }
        ioctl::MODE_FB_DESTROY => {
            unsafe {
                if arg.is_null() {
                    return Err("NULL arg");
                }
                let fb_id = core::ptr::read(arg as *const u32);
                drm.destroy_framebuffer(fb_id)?;
            }
            Ok(0)
        }

        // ── MODE_PAGE_FLIP (0xc0206407) ──────────────────────────────────────
        // struct drm_mode_crtc_page_flip {
        //   crtc_id: u32, fb_id: u32, flags: u32, reserved: u32, user_data: u64
        // }  (24 bytes)
        ioctl::MODE_PAGE_FLIP => {
            unsafe {
                if arg.is_null() {
                    return Err("NULL arg");
                }
                let crtc_id = core::ptr::read((arg as *const u32).add(0));
                let fb_id   = core::ptr::read((arg as *const u32).add(1));
                let _flags  = core::ptr::read((arg as *const u32).add(2));
                if crtc_id != drm.get_resources().crtc_id {
                    return Err("Unknown CRTC");
                }
                drm.queue_page_flip(fb_id)?;
            }
            Ok(0)
        }

        _ => {
            println!("[DRM Gateway] Unhandled ioctl 0x{:x}", cmd);
            Ok(0)
        }
    }
}

/// Initialize DRM device
pub fn init() {
    println!(" ~ DRM/KMS (Microkernel Gateway) ........ ready");
}
