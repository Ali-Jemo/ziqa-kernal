    // ── Capability tests ─────────────────────────────────────────────────────
    test!("capability: grant and lookup", {
        use crate::capability::{CapabilitySpace, ResourceKind, Permissions};
        let mut space = CapabilitySpace::new();
        let id = space.grant(ResourceKind::Memory, Permissions::read_write(), 0x1000, None);
        id.is_some() && space.lookup(id.unwrap()).is_some()
    });

    test!("capability: revoke removes cap", {
        use crate::capability::{CapabilitySpace, ResourceKind, Permissions};
        let mut space = CapabilitySpace::new();
        let id = space.grant(ResourceKind::File, Permissions::read_only(), 3, None).unwrap();
        space.revoke_local(id);
        space.lookup(id).is_none()
    });
