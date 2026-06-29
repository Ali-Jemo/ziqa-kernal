# YOU OS / ZiqaKernel Agent Accelerator

Comprehensive agentic toolkit for Rust `no_std` kernel development. Provides diagnostic skills, slash commands, and an MCP server for structured reasoning and database-backed incident tracking.

Compatible with: Claude Code, OpenCode, Kilo Code, Kiro, Codex, Gemini CLI, Cursor, Windsurf, and any MCP-capable client.

## Features

- **Slash Commands**: `/zk`, `/zk-fix`, `/zk-plan`, `/zk-review`, `/zk-elf`, `/zk-sched`, `/zk-gui`, `/zk-mem`, `/zk-ipc`, `/zk-input`, `/zk-driver`.
- **Master Skill**: `you-os-kernel-accelerator` provides a comprehensive diagnostic framework, Ponytail policy, and Rust kernel discipline.
- **MCP Server**: Real tools for kernel diagnosis (`kernel.diagnose`, `kernel.analyze_serial`), incident tracking (`kernel.errmem_search/add`), architectural lookups (`kernel.project_rules`), and knowledge graph queries (`kernel.graph_query`).
- **Rust Kernel Skill**: `rust-kernel-development` integrates `no_std`, `unsafe` invariants, and bare-metal Rust discipline.
- **Project Rules**: `you-os-project-rules` stores project-specific architecture truths.

## Installation

Run the installation script in your kernel project root:

```bash
chmod +x scripts/install-zk-agent.sh
./scripts/install-zk-agent.sh
```

## MCP Server Setup

1. Install dependencies:
   ```bash
   .agents/mcp-server/setup.sh
   ```
2. Add to your agent's MCP configuration (e.g., `~/.claude/mcp.json`):

```json
{
  "mcpServers": {
    "kernel-engineering": {
      "command": "/path/to/repo/.agents/mcp-server/run-mcp.sh"
    }
  }
}
```

## Usage

### Slash Commands
Once installed, use slash commands directly:
```
/zk-fix "your bug symptom"
/zk-sched "describe scheduler issue"
```

### MCP Tools
If you use an MCP-capable client, you have access to specialized kernel tools:
- `kernel.diagnose`: Symptom analysis & checklist retrieval
- `kernel.analyze_serial`: Automatic panic/fault pattern matching
- `kernel.errmem_search`: Incident DB lookup
- `kernel.graph_query`: Knowledge graph codebase querying

## Publishing

To publish your own version:
1. Package the `.agents/` and `.opencode/` structures.
2. Commit to a new repository.
3. Push to GitHub.

## License

MIT
