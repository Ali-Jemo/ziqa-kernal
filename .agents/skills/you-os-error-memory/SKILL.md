# Incident Database Schema

```sql
CREATE TABLE incidents (
    id INTEGER PRIMARY KEY,
    symptom TEXT NOT NULL,
    subsystem TEXT NOT NULL,
    root_cause TEXT,
    files_changed TEXT,
    verification TEXT
);
```

# Error Memory Workflow

Before debugging:
1. Search with `errmem search "<symptom>"`
2. Compare current symptoms with top 3 incidents.

After fixing:
1. Run `errmem add` to record the incident.

```bash
python .agents/skills/you-os-error-memory/scripts/errmem.py add --symptom="..." --root_cause="..."
```
