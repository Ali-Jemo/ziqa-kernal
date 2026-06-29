use crate::process::scheduler::SCHEDULER;
use crate::process::Pid;
use core::arch::x86_64::_rdtsc;

pub fn bench_fork() {
    crate::println!("[BENCH] Starting COW Fork latency test...");
    
    let start = unsafe { _rdtsc() };
    if let Some(_child_pid) = SCHEDULER.fork(Pid(0)) { // PID 0 is current task (shell)
        let end = unsafe { _rdtsc() };
        crate::println!("[BENCH]   Fork latched in {} cycles", end - start);
        
        // In a real bench we'd wait for child to do something and exit.
        // But for COW latency, the fork() call itself is the bottleneck.
        // We'll just leave the child process as a zombie for now or have scheduler clean it.
        // Actually, fork child will just return to shell loop and might conflict.
        // This is why we usually bench in a controlled environment.
    } else {
        crate::println!("[BENCH]   Fork failed!");
    }
}
