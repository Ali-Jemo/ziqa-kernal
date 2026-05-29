/// Capability-based security system for ZiqaKernel
///
/// Instead of traditional Unix ACLs, every resource access is mediated
/// by unforgeable capability tokens. A process can only operate on
/// resources for which it holds a valid capability.

/// Unique identifier for a capability token
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CapabilityId(pub u64);

/// What the capability grants access to
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    /// A memory region (virtual address range)
    Memory,
    /// A file descriptor or filesystem path
    File,
    /// A network socket
    Network,
    /// An I/O port or MMIO range
    DeviceIo,
    /// Permission to spawn processes
    ProcessCreate,
    /// Permission to load/register ABI plugins
    AbiPlugin,
    /// Inter-process communication channel
    IpcChannel,
}

/// Permission bits within a capability
#[derive(Debug, Clone, Copy)]
pub struct Permissions {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
    /// Can this capability be delegated to child processes?
    pub delegate: bool,
}

impl Permissions {
    pub const fn read_only() -> Self {
        Self {
            read: true,
            write: false,
            execute: false,
            delegate: false,
        }
    }

    pub const fn read_write() -> Self {
        Self {
            read: true,
            write: true,
            execute: false,
            delegate: false,
        }
    }

    pub const fn full() -> Self {
        Self {
            read: true,
            write: true,
            execute: true,
            delegate: true,
        }
    }
}

/// An unforgeable capability token
#[derive(Debug, Clone)]
pub struct CapabilityToken {
    pub id: CapabilityId,
    pub resource: ResourceKind,
    pub permissions: Permissions,
    /// Resource-specific identifier (e.g., FD number, address, port)
    pub target: u64,
}

impl CapabilityToken {
    pub fn new(
        id: CapabilityId,
        resource: ResourceKind,
        permissions: Permissions,
        target: u64,
    ) -> Self {
        Self {
            id,
            resource,
            permissions,
            target,
        }
    }

    /// Check if this capability grants the required permission
    pub fn allows(&self, resource: ResourceKind, needs_write: bool, needs_exec: bool) -> bool {
        if self.resource != resource {
            return false;
        }
        if needs_write && !self.permissions.write {
            return false;
        }
        if needs_exec && !self.permissions.execute {
            return false;
        }
        true
    }
}

/// Maximum capabilities a single process can hold
const MAX_CAPS_PER_PROCESS: usize = 64;

/// A process's capability space — the set of all capabilities it holds
pub struct CapabilitySpace {
    caps: [Option<CapabilityToken>; MAX_CAPS_PER_PROCESS],
    count: usize,
    next_id: u64,
}

impl CapabilitySpace {
    pub const fn new() -> Self {
        const NONE: Option<CapabilityToken> = None;
        Self {
            caps: [NONE; MAX_CAPS_PER_PROCESS],
            count: 0,
            next_id: 1,
        }
    }

    /// Grant a new capability and return its ID
    pub fn grant(
        &mut self,
        resource: ResourceKind,
        permissions: Permissions,
        target: u64,
    ) -> Option<CapabilityId> {
        if self.count >= MAX_CAPS_PER_PROCESS {
            return None;
        }
        let id = CapabilityId(self.next_id);
        self.next_id += 1;
        let token = CapabilityToken::new(id, resource, permissions, target);
        // Find first empty slot
        for slot in self.caps.iter_mut() {
            if slot.is_none() {
                *slot = Some(token);
                self.count += 1;
                return Some(id);
            }
        }
        None
    }

    /// Revoke a capability by its ID
    pub fn revoke(&mut self, id: CapabilityId) -> bool {
        for slot in self.caps.iter_mut() {
            let matches = match slot {
                Some(cap) => cap.id == id,
                None => false,
            };
            if matches {
                *slot = None;
                self.count -= 1;
                return true;
            }
        }
        false
    }

    /// Look up a capability by ID
    pub fn lookup(&self, id: CapabilityId) -> Option<&CapabilityToken> {
        for slot in self.caps.iter() {
            if let Some(cap) = slot {
                if cap.id == id {
                    return Some(cap);
                }
            }
        }
        None
    }

    /// Check if this space grants a specific permission on a resource kind
    pub fn has_permission(
        &self,
        resource: ResourceKind,
        needs_write: bool,
        needs_exec: bool,
    ) -> bool {
        self.caps.iter().any(|slot| {
            slot.as_ref()
                .map(|cap| cap.allows(resource, needs_write, needs_exec))
                .unwrap_or(false)
        })
    }
}
