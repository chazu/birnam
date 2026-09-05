# Birnam work tracker

The repository-local **Beads** tracker (`.beads/`, prefix `birnam`) is the
authoritative active backlog. Run commands from this checkout:

```sh
br where
br ready
br list
br show <id>
br sync --flush-only
```

Commit `.beads/issues.jsonl` with related work; the SQLite database is local.
Before changing issues, `br where` must resolve this repository's `.beads/`,
not the unrelated home-directory Maggie tracker.

## Selected sequence — 2026-09-05

1. Close out Design 65 self-describing project publications: complete acceptance,
   verify Git/native dependency and relocation behavior, and record launch timing.
2. Research and document stackless throughput recovery: refresh focused evidence,
   identify the dominant costs, and propose bounded optimization slices preserving
   stackless execution and lifecycle contracts. Implementation follows the plan.

Issue IDs and validation evidence live in Beads. Design documents describe
contracts and plans; this page does not duplicate issue status.

## Cleanup decision

The [previous tracker](../archive/TASKS-2026-09-05.md) is archived intact for
provenance. Its outstanding Zig shim deduplication is obsolete after the Rust
cutover. Old Zig-specific JSON, process, networking, module override, and REPL
packaging follow-ups are removed from the active backlog. Broader YAML,
concurrency, and networking ideas remain unselected, not declared implemented.
Completed historical work belongs in the changelog/design history rather than
the active queue.
