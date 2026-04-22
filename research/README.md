# Research

Typst sources for durable, publishable artifacts: the whitepaper, the prior-art survey, and per-milestone benchmark reports. Consumers: grant committees, conference submissions, a downloadable PDF on the product site, archived snapshots of claims.

## Layout

```
research/
├── README.md                 # this file
├── lib/                      # shared Typst template + bib
│   ├── template.typ
│   └── refs.bib
├── prior-art/                # living survey of the field
│   └── main.typ
├── whitepaper/               # the canonical zkmcu paper
│   └── main.typ
└── reports/                  # short per-milestone writeups
    └── 2026-04-21-groth16-baseline.typ
```

## Conventions

- **Every doc is one `.typ` file that `#import "/research/lib/template.typ": *"`** — the template owns page setup, fonts, colors, title block, footers.
- **All numbers come from `/benchmarks/runs/*/result.toml`.** Do not hardcode benchmark results in prose; Typst can read and render TOML directly.
- **Reports are immutable** — each has a date-slug filename. If a result needs revision, add a new dated report rather than mutating history.
- **The whitepaper and prior-art survey are living** — they evolve alongside the project.

## Building

```bash
# Single file
typst compile research/reports/2026-04-21-groth16-baseline.typ

# Everything (via justfile)
just docs
```

Produced PDFs land in `research/out/` (gitignored).

## What goes where

| File | Purpose | Audience |
|------|---------|----------|
| `whitepaper/main.typ` | The canonical technical artifact: what zkmcu is, why, how, numbers | grant committees, conference reviewers, serious integrators |
| `prior-art/main.typ` | Survey of the embedded-ZK landscape; updated as the field moves | anyone asking "is this novel?" |
| `reports/<date>-<slug>.typ` | Tight 1–3 page writeup of one benchmark milestone | blog readers, the web docs site (PDFs linked in) |
