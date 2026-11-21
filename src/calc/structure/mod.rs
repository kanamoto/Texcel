//! Updates formulas and validation references during table structure changes.

mod formulas;
mod operations;

pub(crate) use formulas::rewrite_structure_formulas;
