//!  Prepared coordinate maps and metadata transforms for row, column, and fill changes.

mod fill;
mod plan;
mod prepared;

pub(crate) use fill::{SeriesKind, SeriesValue, replace_formula_input, validate_series_source};
pub use plan::{CellChange, FillMode, StructureAxis, StructureOperation, StructurePlan};
pub(crate) use plan::{frozen_prefix, structure_table_id};
pub use prepared::PreparedStructure;
pub(crate) use prepared::StructureSwapResult;
