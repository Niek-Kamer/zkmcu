// Shared Typst template for all zkmcu research docs.
//
// Usage:
//   #import "/research/lib/template.typ": *
//   #show: paper.with(
//     title: "My report",
//     authors: ("J. Smith",),
//     date: "2026-04-21",
//     kind: "report",
//   )

#let paper(
  title: "",
  authors: ("",),
  date: "",
  kind: "report", // "report" | "whitepaper" | "survey"
  abstract: none,
  body,
) = {
  set document(title: title, author: authors.first())
  set page(
    paper: "a4",
    margin: (x: 2.2cm, y: 2.4cm),
    numbering: "1 / 1",
    header: context {
      set text(size: 8pt, fill: luma(120))
      grid(
        columns: (1fr, auto),
        align: (left, right),
        upper(kind),
        [zkmcu · #date],
      )
    },
  )
  set text(font: "New Computer Modern", size: 10.5pt)
  set par(justify: true, leading: 0.65em)
  show heading: set text(weight: "semibold")
  show heading.where(level: 1): set text(size: 18pt)
  show heading.where(level: 2): set text(size: 13pt)
  show heading.where(level: 3): set text(size: 11pt)
  show link: set text(fill: rgb("#2a6099"))
  show raw: set text(font: "DejaVu Sans Mono", size: 9pt)

  // Title block
  align(center)[
    #block(above: 0em, below: 0.5em)[
      #text(size: 20pt, weight: "semibold", title)
    ]
    #block[
      #text(size: 10pt, authors.join(", "))
      · #text(size: 10pt, date)
    ]
  ]

  if abstract != none {
    block(inset: (x: 1em, y: 0.5em), width: 100%, fill: luma(248))[
      #text(size: 9.5pt)[*Abstract.* #abstract]
    ]
    v(0.5em)
  }

  body
}

// Convenience helper for rendering a benchmark result row from TOML data.
#let bench-row(name, cycles, us) = {
  [*#name* · #cycles cycles · #us μs]
}

// A simple table builder for before/after comparison tables.
#let compare-table(headers, rows) = {
  table(
    columns: headers.len(),
    stroke: 0.4pt + luma(180),
    ..headers.map(h => text(weight: "semibold")[#h]),
    ..rows.flatten(),
  )
}
