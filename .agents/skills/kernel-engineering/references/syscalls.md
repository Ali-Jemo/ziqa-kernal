# Syscall and ABI Reference

## Key Concepts
- **Syscall Entry** - int80 / syscall instruction
- **Register Convention** - ABI defines which registers pass arguments
- **User Copy** - copy_to_user / copy_from_user validation
- **Blocking Path** - Scheduler yield when syscall blocks
- **Error Mapping** - Mapping kernel-internal error to errno

## Syscall Debug Flow
1. **Entry** - Validate syscall number and register arguments.
2. **Copy** - Validate user pointer, copy to kernel space.
3. **Execution** - Perform subsystem task.
4. **Result** - Set errno or return value.
5. **Exit** - Restore registers, return to userspace.

## Checklist
- [ ] Syscall number valid?
- [ ] Argument registers follow ABI?
- [ ] User pointers validated? (EFAULT check)
- [ ] Error codes consistent?
- [ ] Does syscall block? If yes, does it yield?
- [ ] Re-entrant?
- [ ] Assumes kernel pointer by mistake?
