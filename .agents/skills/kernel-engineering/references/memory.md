# Memory Management Reference

## Key Concepts
- **Physical frame allocator** - manages physical memory pages
- **Virtual memory** - page tables, mapping virtual→physical
- **Higher-half mapping** - kernel mapped in upper virtual address space
- **Heap allocator** - kernel malloc/free (kmalloc, kfree)

## Page Fault Debug Flow

```
Page fault occurs
  → Read CR2 (faulting address)
  → Read error code (bit 0: present, bit 1: write, bit 2: user)
  → Check if address is in valid mapped range
  → Check page table entries for that address
  → Check permissions
  → Identify: demand page, copy-on-write, or access violation
```

## Address Space Layout (typical)

```
0x0000000000000000 - 0x00007FFFFFFFFFFF  Userspace
0xFFFF800000000000 - 0xFFFFFFFFFFFFFFFF  Kernel (higher half)
```

## Checklist
- [ ] Frame allocator initialized before first allocation
- [ ] Page tables identity-mapped early boot regions
- [ ] Kernel heap initialized after frame allocator
- [ ] User/kernel separation enforced in page tables
- [ ] copy_to/from_user validates user pointers
- [ ] Demand paging handler triggers on correct faults
