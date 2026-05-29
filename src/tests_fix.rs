    // ── Capability tests ─────────────────────────────────────────────────────
    test!("capability: grant and lookup", {
        use crate::capability::{CapabilitySpace, ResourceKind, Permissions, CapabilityTarget};
        let mut space = CapabilitySpace::new();
        let id = space.grant(ResourceKind::Memory, Permissions::read_write(), CapabilityTarget::Address(0x1000));
        id.is_some() && space.lookup(id.unwrap()).is_some()
    });

    test!("capability: revoke removes cap", {
        use crate::capability::{CapabilitySpace, ResourceKind, Permissions, CapabilityTarget};
        let mut space = CapabilitySpace::new();
        let id = space.grant(ResourceKind::File, Permissions::read_only(), CapabilityTarget::Address(3)).unwrap();
        space.revoke(id);
        space.lookup(id).is_none()
    });
