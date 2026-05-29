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

/// Handle DRM ioctl from userspace
pub fn handle_ioctl(cmd: u64, arg: *mut u8) -> Result<i64, &'static str> {
    // Dummy reference for graph analysis
    #[allow(unused_imports)]
    use crate::abi::syscall as _ref_to_syscall;

    match cmd {
        ioctl::MODE_FB_CREATE => {
            let fmt = DrmFormat::XRGB8888;
            let (width, height) = {
                let drm = DRM.lock();
                let res = drm.get_resources();
                (res.width, res.height)
            };
            let vma_addr = 0x1000_0000; // Placeholder
            let fb_id = DRM
                .lock()
                .create_framebuffer(width, height, fmt, vma_addr)?;
            unsafe {
                if !arg.is_null() {
                    core::ptr::write(arg as *mut u32, fb_id);
                }
            }
            Ok(0)
        }

        ioctl::MODE_FB_DESTROY => {
            let fb_id = unsafe {
                if arg.is_null() {
                    return Err("Null argument");
                }
                core::ptr::read(arg as *const u32)
            };
            DRM.lock().destroy_framebuffer(fb_id)?;
            Ok(0)
        }

        ioctl::MODE_PAGE_FLIP => {
            let fb_id = unsafe {
                if arg.is_null() {
                    return Err("Null argument");
                }
                core::ptr::read(arg as *const u32)
            };
            DRM.lock().queue_page_flip(fb_id)?;
            Ok(0)
        }

        ioctl::MODE_GETRESOURCES => {
            let (crtc_id, connector_id) = {
                let drm = DRM.lock();
                let res = drm.get_resources();
                (res.crtc_id, res.connector_id)
            };
            unsafe {
                if !arg.is_null() {
                    core::ptr::write((arg as *mut u32).add(0), crtc_id);
                    core::ptr::write((arg as *mut u32).add(1), connector_id);
                }
            }
            Ok(0)
        }

        _ => {
            println!("[DRM] Unhandled ioctl 0x{:x}", cmd);
            Err("Unsupported ioctl")
        }
    }
}

/// Initialize DRM device
pub fn init() {
    println!(" ~ DRM/KMS .............................. ready");
}
