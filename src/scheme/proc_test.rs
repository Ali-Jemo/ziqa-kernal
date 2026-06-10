#[cfg(test)]
mod proc_smoke {
    use crate::scheme::proc::ProcScheme;
    use crate::process::Pid;

    #[test]
    fn proc_scheme_new_and_open_smoke() {
        let scheme = ProcScheme::new();

        let h = scheme.open("proc:1/status", 0).unwrap();
        assert!(h > 0, "handle must be non-zero");

        let mut buf = [0u8; 256];
        let n = scheme.read(h, &mut buf).unwrap();
        assert!(n > 0, "status read must yield bytes");

        scheme.close(h).unwrap();
    }

    #[test]
    fn proc_open_all_known_paths() {
        let scheme = ProcScheme::new();
        let paths = [
            "proc:1/addrspace",
            "proc:1/mem",
            "proc:1/mem/0x1000",
            "proc:1/regs/int",
            "proc:1/regs/env",
            "proc:1/regs/float",
            "proc:1/start",
            "proc:1/status",
            "proc:1/sighandler",
        ];
        for p in &paths {
            let h = scheme.open(p, 0);
            assert!(h.is_ok(), "open {p} must succeed in smoke test");
            let _ = h.unwrap();
        }
    }
}
