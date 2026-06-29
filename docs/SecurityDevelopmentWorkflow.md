# Security Development Workflow

For all ZiqaKernel patches:

1. **Threat Modeling**: Identify TCB boundaries. Does this patch increase attack surface?
2. **`unsafe` Review**:
   - Isolate `unsafe` within SALs.
   - Justify every `unsafe` block with a `// SAFETY: ...` comment documenting invariants.
3. **Invariant Verification**:
   - Verify that all kernel invariants are maintained.
   - Use `kernel.invariant_check` MCP tool.
4. **Audit**:
   - Run `kernel.security_audit` tool to review `unsafe` usage.

*Security is not an add-on; it is built into the architecture.*
