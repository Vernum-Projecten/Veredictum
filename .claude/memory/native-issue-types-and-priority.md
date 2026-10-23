---
name: native-issue-types-and-priority
description: Owner ruling 2026-10-23 (#588) — type, priority and effort are the native issue type and the Vernum-Projecten Priority and Effort issue fields, set with scripts/gh/fields.sh; the bug, enhancement and P0–P3 labels are retired
metadata:
  type: project
---

Veredictum tracks the kind and urgency of its work with the native issue type
(`Bug`, `Feature`, `Task`), the organisation's `Priority` field (`Urgent`,
`High`, `Medium`, `Low`) and its `Effort` field (`High`, `Medium`, `Low`). The
Vernum-Projecten organisation defines all three.

**Why:** labels for type and priority duplicate what GitHub models natively,
and the board and the issue list filter on the fields. The old labels mapped
over as `bug` to Bug, `enhancement` to Feature, `P0`–`P3` to Urgent, High,
Medium and Low; an `upstream-report` with no type label became a Bug.

**How to apply:**

- File every issue with `scripts/gh/fields.sh new <type> <priority> <effort>
  <gh issue create args…>`; change one with `fields.sh type|priority|effort
  <n> <value>`; read with `fields.sh show <n>`.
- A Task carries exactly one work-kind label (`documentation`, `chore`,
  `refactor`, `perf`, `ci`, `test`); a Bug or a Feature carries none. Never
  apply `bug`, `enhancement` or a `P` label: they are retired.
- Pickup order is Priority (Urgent first), oldest first within a priority.
  Effort is an estimate and never reorders.
- A scheduled lane sets only the type, through the REST issue update, and only
  best-effort; Priority and Effort are set at pickup. The SessionStart dump
  shows `<?/?>` for an issue missing either.
