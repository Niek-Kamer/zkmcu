# Benchmarks

Raw and structured results from every zkmcu benchmark run. This is the **single source of truth** — both the research PDFs and the web docs consume these files; nothing is duplicated in prose.

## Layout

```
benchmarks/
├── README.md               # this file
├── schema.md               # TOML schema for result.toml
└── runs/
    └── YYYY-MM-DD-<slug>/
        ├── raw.log         # verbatim capture (serial, CI log, etc.)
        ├── result.toml     # structured, machine-readable
        └── notes.md        # optional: observations, anomalies
```

One directory per run. Runs are append-only; if a result is invalidated, add a new dated run and note it in the old run's `notes.md`.

## Conventions

- Dates are ISO-8601 (`YYYY-MM-DD`) and timezone-naïve — the date is when the run was started, not published.
- Slugs are lowercase, hyphenated: `m33-groth16-baseline`, `hazard3-groth16-baseline`, `m33-dsp-optimized`, etc.
- `result.toml` schema is at [`schema.md`](./schema.md).
- `raw.log` is whatever the firmware (or host tool) actually printed. Do not edit it.

## Adding a run

1. `mkdir benchmarks/runs/$(date -I)-<slug>`
2. Capture `raw.log` (e.g., `cat /dev/ttyACM0 | tee raw.log`).
3. Parse into `result.toml` matching the schema.
4. Optionally write `notes.md` with context.
5. Commit.
