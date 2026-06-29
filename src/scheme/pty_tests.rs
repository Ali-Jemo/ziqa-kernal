#[cfg(test)]
mod pty_smoke {
    use super::*;
    use crate::scheme::pty::PtyPair;

    #[test]
    fn test_buffer_isolation() {
        let pty = PtyPair::new();
        
        // Push to master_to_slave
        {
            let mut m2s = pty.master_to_slave.lock();
            m2s.push(b'a');
            m2s.push(b'b');
        }

        // Verify slave_to_master is empty
        {
            let mut s2m = pty.slave_to_master.lock();
            assert!(s2m.pop().is_none());
        }

        // Verify master_to_slave has data
        {
            let mut m2s = pty.master_to_slave.lock();
            assert_eq!(m2s.pop(), Some(b'a'));
            assert_eq!(m2s.pop(), Some(b'b'));
            assert!(m2s.pop().is_none());
        }
    }

    #[test]
    fn test_rw_cycles() {
        let pty = PtyPair::new();

        // Write a stream
        let stream = b"hello world";
        for &b in stream {
            pty.master_to_slave.lock().push(b);
        }

        // Read stream from slave side
        for &b in stream {
            let mut m2s = pty.master_to_slave.lock();
            assert_eq!(m2s.pop(), Some(b));
        }

        // Check empty
        assert!(pty.master_to_slave.lock().pop().is_none());
    }
}
