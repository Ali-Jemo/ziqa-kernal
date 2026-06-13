# Documentation Revision Notes

Files reviewed against current tracked source:
- `README.md`
- `ARCHITECTURE.md`
- `ZIQA_KERNEL_ROADMAP.md`

## Changes Needed / Noted
- `ZIQA_KERNEL_ROADMAP.md` was already updated in a prior pass: network stack status corrected from "Full TCP/UDP Lifecycle" to "Experimental TCP/UDP Lifecycle" (partial `listen`/`accept` support via smoltcp).
- No new README/ARCHITECTURE rewrites were applied in this session because the safer validated path is:
  1) organize-plan.md-driven file relocation, and
  2) targeted cleanup of tracked throwaways first.
- Current validated diff set is from `git status --short` + `git diff --stat` and should remain the source of truth before any doc path moves.

## Next Recommended Doc Steps
- After `organize-plan.md` is executed, update internal doc links/paths from root to docs/.
- Re-run `node .gitnexus/run.cjs detect_changes` before committing documents.
