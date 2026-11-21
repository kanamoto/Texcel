# Texcel local patches

This directory holds the `umya-spreadsheet` 3.1.1 source from crates.io. The
original MIT `LICENSE` and README are kept unchanged. Texcel selects this copy
through `[patch.crates-io]` in the root `Cargo.toml`. The changes below are
offered under the same license as this crate, not under Texcel's MIT license.
Texcel uses the crate only to import XLSX workbooks into Markdown.

## Inline rich text

- `src/structs/shared_string_item.rs` lets the existing `<si>` run reader stop
  at either the shared-string `<si>` or the inline-string `<is>` element.
- An empty `<t/>` is recorded as an explicit empty string, so it stays distinct
  from an empty cell.
- `src/structs/cell.rs` reads `<is>` through that same reader.

The release assigned each nested `<t>` directly to the cell, so later runs
replaced earlier ones.

## Pane and row attributes

Pane and row-dimension accessors report whether an XML attribute was present,
separately from its default value. A pane without `topLeftCell` is
distinguishable from an explicit `A1`, and row `ht` is independent of
`customHeight`.
