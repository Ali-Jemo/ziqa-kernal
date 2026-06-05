# FAT32 Write Support Implementation Plan

## Objective
Wire up the existing FAT32 write primitives and directory operations into the Virtual File System (VFS) and the Linux ABI `sys_write` handler, bridging the gap between the core driver and userspace.

## Key Files & Context
- `src/abi/linux/mod.rs`: The current `sys_write` implementation for files is a dummy that updates the file offset and returns success without actually writing any data to the disk.
- `src/fs/vfs.rs`: The VFS handles operations like `mkdir`, `remove`, `rmdir`, and `truncate` locally in memory but lacks the necessary delegation to persist these operations to the underlying FAT32 partition.
- `src/fs/fat32.rs`: The core logic (`Fat32File::write`, `mkdir_on_disk`, `unlink_on_disk`, `rmdir_on_disk`, `truncate_on_disk`) is already written and ready to be connected.
- `README.md`: Needs to be updated to reflect that FAT32 write support has been successfully implemented.

## Implementation Steps
1. **ABI Layer Extension:**
   - In `src/abi/linux/mod.rs`, update the `sys_write` handler for `FdTarget::File`.
   - Retrieve the path associated with the file descriptor.
   - Invoke `crate::fs::vfs::VFS.read().write_raw(path_str, bytes, offset)` to actually perform the write.
   - Update the file descriptor's offset on success.

2. **VFS Directory & File Hooks:**
   - In `src/fs/vfs.rs`, modify `mkdir` to intercept paths starting with `/fat/` and delegate to `fat32::mkdir_on_disk`.
   - Modify `remove` to intercept `/fat/` paths. If the target is a directory, delegate to `fat32::rmdir_on_disk`; if it is a file, delegate to `fat32::unlink_on_disk`. Upon success, remove the node from the VFS tree.
   - Modify `truncate` to intercept `/fat/` paths and delegate to `fat32::truncate_on_disk`.
   - Modify `rename` to intercept `/fat/` paths and delegate to `fat32::rename_on_disk` (which currently fails gracefully).

3. **Documentation:**
   - Update `README.md` to move "FAT32 Write Support" from the incomplete roadmap section to the completed list, noting that the write paths are now fully wired up.

## Verification & Testing
- Compile the kernel and verify it boots successfully.
- Verify `touch`, `mkdir`, and redirection from shell output (e.g. `echo hello > /fat/test.txt`) execute without errors and persist to the disk.
- Verify `rm` (unlink) and `rmdir` correctly delete items.
