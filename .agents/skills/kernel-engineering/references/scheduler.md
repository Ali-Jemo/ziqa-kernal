# Scheduler Reference

## Key Concepts
- **Task struct** - per-process metadata (PID, state, registers, stack)
- **Ready queue** - runnable tasks waiting for CPU
- **Context switch** - save current registers, load next task's registers
- **Time slice** - max CPU time before involuntary preemption

## State Machine

```
CREATE → READY → RUNNING → BLOCKED
                ↑         ↓
                ← READY ←
                ↓
             TERMINATED
```

## Debug Checklist
- [ ] Timer interrupt fires at expected frequency
- [ ] Scheduler invoked on timer tick
- [ ] Context switch saves/restores all callee-saved registers
- [ ] Kernel stack is per-task, not shared
- [ ] Current task marked appropriately (runnable/sleeping)
- [ ] Sleep queue woken correctly
- [ ] Yield syscall invokes scheduler
- [ ] Blocking syscall marks task as blocked + yields
