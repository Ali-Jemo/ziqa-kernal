/// Capability-based security system for ZiqaKernel
///
/// Instead of traditional Unix ACLs, every resource access is mediated
/// unforgeable capability tokens. A process can only operate on
/// resources for which it holds a valid capability.

use alloc::vec::Vec;

use spin::Mutex;
use alloc::collections::BTreeMap;

/// Unique identifier for a capability token (Global across system for revocation)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
    pub parent_id: Option<CapabilityId>,
    pub resource: ResourceKind,
    pub permissions: Permissions,
    /// Resource-specific identifier (e.g., FD number, address, port)
    pub target: u64,
}

impl CapabilityToken {
    pub fn new(
        id: CapabilityId,
        parent_id: Option<CapabilityId>,
        resource: ResourceKind,
        permissions: Permissions,
        target: u64,
    ) -> Self {
        Self {
            id,
            parent_id,
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
const MAX_CAPS_PER_PROCESS: usize = 32;

/// A process's capability space — the set of all capabilities it holds
pub struct CapabilitySpace {
    caps: [Option<CapabilityToken>; MAX_CAPS_PER_PROCESS],
    count: usize,
}

impl CapabilitySpace {
    pub const fn new() -> Self {
        const NONE: Option<CapabilityToken> = None;
        Self {
            caps: [NONE; MAX_CAPS_PER_PROCESS],
            count: 0,
        }
    }

    /// Grant a new capability and return its ID
    pub fn grant(
        &mut self,
        resource: ResourceKind,
        permissions: Permissions,
        target: u64,
        parent_id: Option<CapabilityId>,
    ) -> Option<CapabilityId> {
        if self.count >= MAX_CAPS_PER_PROCESS {
            return None;
        }
        let id = alloc_id();
        let token = CapabilityToken::new(id, parent_id, resource, permissions, target);
        
        // Track relationship for revocation
        if let Some(pid) = parent_id {
            REVOCATION_TREE.lock().add_child(pid, id);
        }

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

    /// Revoke a capability by its ID (Local operation)
    pub fn revoke_local(&mut self, id: CapabilityId) -> bool {
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

    /// System-wide instant revocation
    pub fn revoke_global(id: CapabilityId) {
        let mut descendants = Vec::new();
        {
            let tree = REVOCATION_TREE.lock();
            tree.get_all_descendants(id, &mut descendants);
        }

        descendants.push(id);

        // Iterate over all processes and remove these capabilities
        let pids = crate::process::scheduler::list_pids();
        for pid in pids {
            crate::process::scheduler::with_process_mut(pid, |proc| {
                for &target_id in &descendants {
                    if proc.capabilities.revoke_local(target_id) {
                        // If it's a memory capability, we should ideally unmap it.
                        // For now, this severes future access checks.
                        crate::println!("[CAP] Revoked {} from PID {}", target_id.0, pid.0);
                    }
                }
            });
        }

        // Cleanup tree
        let mut tree = REVOCATION_TREE.lock();
        for &target_id in &descendants {
            tree.remove_node(target_id);
        }
    }
}

// ── Global ID Allocation and Revocation Tree ──────────────────────────────────

static NEXT_ID: Mutex<u64> = Mutex::new(1);

fn alloc_id() -> CapabilityId {
    let mut lock = NEXT_ID.lock();
    let id = *lock;
    *lock += 1;
    CapabilityId(id)
}

struct RevocationTree {
    /// Maps parent ID to its direct children
    children: BTreeMap<CapabilityId, Vec<CapabilityId>>,
}

impl RevocationTree {
    pub const fn new() -> Self {
        Self {
            children: BTreeMap::new(),
        }
    }

    pub fn add_child(&mut self, parent: CapabilityId, child: CapabilityId) {
        self.children.entry(parent).or_insert_with(Vec::new).push(child);
    }

    pub fn get_all_descendants(&self, id: CapabilityId, out: &mut Vec<CapabilityId>) {
        if let Some(kids) = self.children.get(&id) {
            for &kid in kids {
                out.push(kid);
                self.get_all_descendants(kid, out);
            }
        }
    }

    pub fn remove_node(&mut self, id: CapabilityId) {
        self.children.remove(&id);
    }
}

static REVOCATION_TREE: Mutex<RevocationTree> = Mutex::new(RevocationTree::new());
