//!  Holds IronCalc models and applies sparse changes on the worker thread.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use ironcalc_base::Model;

use crate::calc::snapshot::{CalculationChange, CalculationSnapshot, CalculationTable};
use crate::document::{CalcStatus, CalcValue, CellAddress, CellInput, TableId};
use formula::engine_error_message;
use formula::{formula_references, one_based_engine_index, read_result, update_model_cell};

pub(crate) mod formula;
pub(crate) use formula::{formula_source, unsupported_initial_results};

pub(super) struct EngineState {
    cross_sheet_references: bool,
    models: Vec<Model<'static>>,
    table_location: HashMap<TableId, (usize, u32)>,
    formulas: BTreeMap<(TableId, CellAddress), FormulaState>,
}

#[derive(Clone)]
struct FormulaState {
    model_index: usize,
    sheet_index: u32,
    address: CellAddress,
    source: String,
    directly_supported: bool,
    installed_in_engine: bool,
    references: Vec<FormulaReference>,
    imported_value: Option<CalcValue>,
}

/// A1 targets referenced by one parsed formula, resolved against its origin.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FormulaReference {
    pub(crate) model_index: usize,
    pub(crate) sheet_index: u32,
    pub(crate) first: CellAddress,
    pub(crate) last: CellAddress,
}

impl FormulaReference {
    /// Checks whether the engine contains a table.
    fn contains(self, address: CellAddress) -> bool {
        self.first.row <= address.row
            && address.row <= self.last.row
            && self.first.column <= address.column
            && address.column <= self.last.column
    }
}

/// Creates an IronCalc engine from a document snapshot.
pub(super) fn create_engine(
    source_snapshot: Arc<CalculationSnapshot>,
) -> Result<EngineState, crate::i18n::Message> {
    let snapshot = source_snapshot.as_ref();
    let mut models = Vec::new();
    let mut table_location = HashMap::new();
    let mut formulas = BTreeMap::new();
    if snapshot.cross_sheet_references {
        let Some(first) = snapshot.tables.first() else {
            return Err(crate::i18n::Message::new("status-calculation-no-sheets"));
        };
        let mut model = Model::new_empty("texcel-calculation", "en", "UTC", "en")
            .map_err(engine_error_message)?;
        model
            .rename_sheet("Sheet1", &first.name)
            .map_err(engine_error_message)?;
        for (table_index, table) in snapshot.tables.iter().enumerate() {
            if table_index > 0 {
                model.add_sheet(&table.name).map_err(engine_error_message)?;
            }
            table_location.insert(table.id, (0, table_index as u32));
            load_table(
                &mut model,
                0,
                table_index as u32,
                table,
                true,
                &mut formulas,
            )?;
        }
        models.push(model);
    } else {
        for table in &snapshot.tables {
            let mut model = Model::new_empty("texcel-calculation", "en", "UTC", "en")
                .map_err(engine_error_message)?;
            let model_index = models.len();
            table_location.insert(table.id, (model_index, 0));
            load_table(&mut model, model_index, 0, table, false, &mut formulas)?;
            models.push(model);
        }
    }
    Ok(EngineState {
        cross_sheet_references: snapshot.cross_sheet_references,
        models,
        table_location,
        formulas,
    })
}

/// Loads one table and its string-cell set into the engine.
fn load_table(
    model: &mut Model<'static>,
    model_index: usize,
    sheet_index: u32,
    table: &CalculationTable,
    allow_sheet_reference: bool,
    formulas: &mut BTreeMap<(TableId, CellAddress), FormulaState>,
) -> Result<(), crate::i18n::Message> {
    for (address, cell) in table.cells.iter() {
        let supported = update_model_cell(
            model,
            sheet_index,
            *address,
            &cell.input,
            table.string_cells.contains(address),
            allow_sheet_reference,
        )?;
        if let Some(source) = formula_source(&cell.input, table.string_cells.contains(address)) {
            let imported_value = cell.imported_result.clone();
            let references = if supported {
                formula_references(model, model_index, sheet_index, *address)
            } else {
                Vec::new()
            };
            formulas.insert(
                (table.id, *address),
                FormulaState {
                    model_index,
                    sheet_index,
                    address: *address,
                    source,
                    directly_supported: supported,
                    installed_in_engine: true,
                    references,
                    imported_value,
                },
            );
        }
    }
    Ok(())
}

impl EngineState {
    /// Runs formula evaluation for the current engine state.
    pub(super) fn evaluate(&mut self) {
        for model in &mut self.models {
            model.evaluate();
        }
    }

    /// Applies changed cells to the existing engine.
    pub(super) fn apply_changes(
        &mut self,
        changes: &[CalculationChange],
    ) -> Result<(), crate::i18n::Message> {
        for change in changes {
            let Some((model_index, sheet_index)) =
                self.table_location.get(&change.table_id).copied()
            else {
                continue;
            };
            let model = self
                .models
                .get_mut(model_index)
                .ok_or_else(|| crate::i18n::Message::new("status-calculation-model-unavailable"))?;
            let input = change
                .cell
                .as_ref()
                .map(|cell| &cell.input)
                .unwrap_or(&CellInput::Empty);
            let supported = update_model_cell(
                model,
                sheet_index,
                change.address,
                input,
                change.string_cell,
                self.cross_sheet_references,
            )?;
            if let Some(source) = formula_source(input, change.string_cell) {
                let imported_value = change
                    .cell
                    .as_ref()
                    .and_then(|cell| cell.imported_result.clone());
                let references = if supported {
                    formula_references(model, model_index, sheet_index, change.address)
                } else {
                    Vec::new()
                };
                self.formulas.insert(
                    (change.table_id, change.address),
                    FormulaState {
                        model_index,
                        sheet_index,
                        address: change.address,
                        source,
                        directly_supported: supported,
                        installed_in_engine: true,
                        references,
                        imported_value,
                    },
                );
            } else {
                self.formulas.remove(&(change.table_id, change.address));
            }
        }
        Ok(())
    }

    /// Removes unsupported formulas before evaluation and restores dependents
    /// when the unsupported input that blocked them is replaced.
    pub(super) fn refresh_formula_masking(&mut self) -> Result<(), crate::i18n::Message> {
        let mut blocked = BTreeSet::<(usize, u32, CellAddress)>::new();
        for formula in self.formulas.values() {
            if !formula.directly_supported {
                blocked.insert((formula.model_index, formula.sheet_index, formula.address));
            }
        }

        loop {
            let mut added = Vec::new();
            for formula in self.formulas.values() {
                if !formula.directly_supported
                    || blocked.contains(&(
                        formula.model_index,
                        formula.sheet_index,
                        formula.address,
                    ))
                {
                    continue;
                }
                if formula.references.iter().any(|reference| {
                    blocked.iter().any(|(model, sheet, address)| {
                        *model == reference.model_index
                            && *sheet == reference.sheet_index
                            && reference.contains(*address)
                    })
                }) {
                    added.push((formula.model_index, formula.sheet_index, formula.address));
                }
            }
            if added.is_empty() {
                break;
            }
            blocked.extend(added);
        }

        for formula in self.formulas.values_mut() {
            let should_be_installed = formula.directly_supported
                && !blocked.contains(&(formula.model_index, formula.sheet_index, formula.address));
            if formula.installed_in_engine == should_be_installed {
                continue;
            }
            let row = one_based_engine_index(formula.address.row)?;
            let column = one_based_engine_index(formula.address.column)?;
            let model = self
                .models
                .get_mut(formula.model_index)
                .ok_or_else(|| crate::i18n::Message::new("status-calculation-model-unavailable"))?;
            if should_be_installed {
                model
                    .update_cell_with_formula(
                        formula.sheet_index,
                        row,
                        column,
                        formula.source.clone(),
                    )
                    .map_err(engine_error_message)?;
            } else {
                model
                    .set_user_input(formula.sheet_index, row, column, String::new())
                    .map_err(engine_error_message)?;
            }
            formula.installed_in_engine = should_be_installed;
        }
        Ok(())
    }

    /// Collects formula results with their source revisions.
    pub(super) fn results(&self) -> Vec<(TableId, CellAddress, CalcStatus, Option<CalcValue>)> {
        self.formulas
            .iter()
            .map(|((table_id, address), formula)| {
                if !formula.installed_in_engine {
                    return (
                        *table_id,
                        *address,
                        CalcStatus::Unsupported,
                        formula.imported_value.clone(),
                    );
                }
                let Some(model) = self.models.get(formula.model_index) else {
                    return (*table_id, *address, CalcStatus::Stale, None);
                };
                match read_result(model, formula.sheet_index, *address) {
                    Ok((status, value)) => (*table_id, *address, status, value),
                    Err(_) => (*table_id, *address, CalcStatus::Stale, None),
                }
            })
            .collect()
    }
}
