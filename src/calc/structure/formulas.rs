//!  Rewrites formulas affected by row, column, fill, or duplicate operations.

use std::collections::{BTreeMap, BTreeSet};

use ironcalc_base::{Model, UserModel};

use crate::calc::engine::formula::{
    engine_error_message, one_based_engine_index, update_model_cell,
};
use crate::calc::engine::formula_source;
use crate::calc::snapshot::CalculationSnapshot;
use crate::calc::structure::operations::{apply_engine_structure, formula_copy_destinations};
use crate::document::{CellAddress, StructureOperation, StructurePlan, TableId};

/// Lets IronCalc update formula references for one prepared structure change.
pub(crate) fn rewrite_structure_formulas(
    snapshot: &CalculationSnapshot,
    plan: &StructurePlan,
) -> Result<BTreeMap<(TableId, CellAddress), String>, crate::i18n::Message> {
    let target_id = match &plan.operation {
        StructureOperation::Insert { table_id, .. }
        | StructureOperation::Delete { table_id, .. }
        | StructureOperation::Duplicate { table_id, .. }
        | StructureOperation::Move { table_id, .. }
        | StructureOperation::ReorderRows { table_id, .. }
        | StructureOperation::Fill { table_id, .. }
        | StructureOperation::SetTextCell { table_id, .. }
        | StructureOperation::SetTextCells { table_id, .. } => *table_id,
    };
    let Some(target_table) = snapshot.tables.iter().find(|table| table.id == target_id) else {
        return Err(crate::i18n::Message::new("status-table-id-unavailable")
            .with_number("table_id", target_id as i64));
    };
    let is_full_row_sort = matches!(
        &plan.operation,
        StructureOperation::ReorderRows {
            source_row_count,
            row_sources,
            ..
        } if row_sources.len() == *source_row_count
    );
    // Only formulas copied to a different row need an engine parse during sorting. Header,
    // fixed-coordinate, and other-table formulas retain their source text verbatim.
    let moved_sort_formulas = if is_full_row_sort {
        target_table
            .cells
            .iter()
            .filter_map(|(address, cell)| {
                formula_source(&cell.input, target_table.string_cells.contains(address))
                    .is_some()
                    .then_some(*address)
            })
            .filter(|address| {
                plan.output_positions(*address)
                    .iter()
                    .any(|output| output != address)
            })
            .collect::<BTreeSet<_>>()
    } else {
        BTreeSet::new()
    };
    let groups = if snapshot.cross_sheet_references {
        vec![snapshot.tables.iter().collect::<Vec<_>>()]
    } else {
        snapshot
            .tables
            .iter()
            .map(|table| vec![table])
            .collect::<Vec<_>>()
    };
    let mut rewritten = BTreeMap::new();
    for group in groups {
        let mut model = Model::new_empty("texcel-structure", "en", "UTC", "en")
            .map_err(engine_error_message)?;
        let mut locations = BTreeMap::<TableId, u32>::new();
        for (sheet_index, table) in group.iter().enumerate() {
            if snapshot.cross_sheet_references {
                if sheet_index == 0 {
                    model
                        .rename_sheet("Sheet1", &table.name)
                        .map_err(engine_error_message)?;
                } else {
                    model.add_sheet(&table.name).map_err(engine_error_message)?;
                }
            }
            let sheet_index = u32::try_from(sheet_index).map_err(|_| {
                crate::i18n::Message::new("status-calculation-index-too-large")
                    .with_number("index", sheet_index as i64)
            })?;
            locations.insert(table.id, sheet_index);
            for (address, cell) in table.cells.iter() {
                if is_full_row_sort
                    && (table.id != target_id || !moved_sort_formulas.contains(address))
                {
                    continue;
                }
                let supported = update_model_cell(
                    &mut model,
                    sheet_index,
                    *address,
                    &cell.input,
                    table.string_cells.contains(address),
                    snapshot.cross_sheet_references,
                )?;
                if formula_source(&cell.input, table.string_cells.contains(address)).is_some()
                    && !supported
                {
                    return Err(crate::i18n::Message::new(
                        "status-structure-formula-cannot-update",
                    )
                    .with_text("table", &table.name)
                    .with_text("cell", crate::xlsx_adapter::cell_reference(*address)));
                }
            }
        }

        let mut formula_before = BTreeMap::<(TableId, CellAddress), String>::new();
        for table in &group {
            let sheet = locations[&table.id];
            for (address, cell) in table.cells.iter() {
                if is_full_row_sort
                    && (table.id != target_id || !moved_sort_formulas.contains(address))
                {
                    continue;
                }
                if formula_source(&cell.input, table.string_cells.contains(address)).is_none() {
                    continue;
                }
                if let Some(formula) = model
                    .get_cell_formula(
                        sheet,
                        one_based_engine_index(address.row)?,
                        one_based_engine_index(address.column)?,
                    )
                    .map_err(engine_error_message)?
                {
                    formula_before.insert((table.id, *address), formula);
                }
            }
        }
        if is_full_row_sort {
            if let Some(sheet) = locations.get(&target_id).copied() {
                for source_address in &moved_sort_formulas {
                    let Some(before) = formula_before.get(&(target_id, *source_address)) else {
                        continue;
                    };
                    for output in plan.output_positions(*source_address) {
                        if output == *source_address {
                            continue;
                        }
                        let after = model
                            .extend_to(
                                sheet,
                                one_based_engine_index(source_address.row)?,
                                one_based_engine_index(source_address.column)?,
                                one_based_engine_index(output.row)?,
                                one_based_engine_index(output.column)?,
                            )
                            .map_err(engine_error_message)?;
                        if &after != before {
                            rewritten.insert((target_id, output), after);
                        }
                    }
                }
            }
            // Sort copies formulas with relative references; formula cells outside the sorted
            // data rows and formulas on other tables retain their source coordinates.
            continue;
        }
        let mut user_model = UserModel::from_model(model);
        user_model.pause_evaluation();
        if let Some(sheet) = locations.get(&target_id).copied() {
            apply_engine_structure(&mut user_model, sheet, &plan.operation, target_table)?;
            for (source_address, cell) in target_table.cells.iter() {
                if formula_source(
                    &cell.input,
                    target_table.string_cells.contains(source_address),
                )
                .is_none()
                {
                    continue;
                }
                let Some(before) = formula_before.get(&(target_id, *source_address)) else {
                    continue;
                };
                for address in formula_copy_destinations(*source_address, plan) {
                    let row = one_based_engine_index(address.row)?;
                    let column = one_based_engine_index(address.column)?;
                    if let Some(after) = user_model
                        .get_model()
                        .get_cell_formula(sheet, row, column)
                        .map_err(engine_error_message)?
                        && &after != before
                    {
                        rewritten.insert((target_id, address), after);
                    }
                }
            }
        }
        for table in &group {
            let sheet = locations[&table.id];
            for (address, cell) in table.cells.iter() {
                if formula_source(&cell.input, table.string_cells.contains(address)).is_none() {
                    continue;
                }
                let outputs = if table.id == target_id {
                    plan.output_positions(*address)
                } else {
                    vec![*address]
                };
                for output in outputs {
                    let row = one_based_engine_index(output.row)?;
                    let column = one_based_engine_index(output.column)?;
                    if let Some(after) = user_model
                        .get_model()
                        .get_cell_formula(sheet, row, column)
                        .map_err(engine_error_message)?
                        && formula_before.get(&(table.id, *address)) != Some(&after)
                    {
                        rewritten.insert((table.id, output), after);
                    }
                }
            }
        }
    }
    Ok(rewritten)
}
