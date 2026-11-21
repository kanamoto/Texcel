//!  Document operations for ranges.

use std::collections::BTreeSet;

use crate::document::*;

impl Document {
    /// Replaces Fresh formulas from several disjoint selections in one history command.
    pub fn replace_formula_results_in_ranges(
        &mut self,
        table_id: TableId,
        ranges: &[CellRange],
    ) -> Result<usize, crate::i18n::Message> {
        let calculation_revision = self.calculation_revision;
        let formula_results = self.calculation_results.clone();
        let table = self
            .tables
            .iter()
            .find(|table| table.id == table_id)
            .ok_or_else(|| {
                crate::i18n::Message::new("status-table-id-unavailable")
                    .with_number("table_id", table_id as i64)
            })?;
        let mut selected_addresses = BTreeSet::new();
        for range in ranges {
            let (_, end) = range.normalized();
            if end.row >= MAX_ROWS || end.column >= MAX_COLUMNS {
                return Err(
                    crate::i18n::Message::new("status-formula-result-range-outside")
                        .with_text("cell", crate::xlsx_adapter::cell_reference(end)),
                );
            }
            selected_addresses.extend(
                table
                    .cells_in_range(*range)
                    .into_iter()
                    .map(|(address, _)| address),
            );
        }
        let mut changes = Vec::new();
        let mut string_overrides = Vec::new();
        for address in selected_addresses {
            let Some(cell) = table.cell(address) else {
                continue;
            };
            let is_text_formula = !table.is_string_cell(address)
                && matches!(&cell.input, CellInput::Text(source) if source.starts_with('='));
            if !matches!(cell.input, CellInput::Formula(_)) && !is_text_formula {
                continue;
            }
            let result = formula_results
                .as_ref()
                .filter(|results| results.revision == calculation_revision)
                .and_then(|results| results.get(table_id, address))
                .filter(|(status, _)| *status == CalcStatus::Fresh)
                .and_then(|(_, value)| value.cloned())
                .ok_or_else(|| {
                    crate::i18n::Message::new("status-formula-result-not-current")
                        .with_text("cell", crate::xlsx_adapter::cell_reference(address))
                })?;
            let mut replacement = cell.clone();
            replacement.input = match result {
                CalcValue::Text(value) => {
                    CellInput::Text(crate::document::encode_literal_text(&value))
                }
                CalcValue::Number(value) => CellInput::Number(value),
                CalcValue::Boolean(value) => CellInput::Boolean(value),
                CalcValue::Error(value) => CellInput::Error(value),
            };
            replacement.calculation = None;
            replacement.imported_result = None;
            let after_is_string =
                matches!(&replacement.input, CellInput::Text(value) if value.starts_with('='));
            let before_is_string = table.is_string_cell(address);
            if before_is_string != after_is_string {
                string_overrides.push((address, before_is_string, after_is_string));
            }
            changes.push(CellChange {
                address,
                before: Some(cell.clone()),
                after: Some(replacement),
            });
        }
        if changes.is_empty() {
            return Err(crate::i18n::Message::new(
                "status-formula-result-no-formulas",
            ));
        }
        let count = changes.len();
        self.execute(EditCommand::SetFormulaResults {
            table_id,
            changes,
            string_overrides,
        })
        .map_err(crate::i18n::document_error_message)?;
        Ok(count)
    }

    /// Clears several disjoint rectangles in one document command.
    pub fn clear_ranges(
        &mut self,
        table_id: TableId,
        ranges: &[CellRange],
    ) -> Result<bool, DocumentError> {
        let table = self
            .tables
            .iter()
            .find(|table| table.id == table_id)
            .ok_or(DocumentError::MissingTable(table_id))?;
        let mut addresses = BTreeSet::new();
        for range in ranges {
            let (_, end) = range.normalized();
            if end.row >= MAX_ROWS || end.column >= MAX_COLUMNS {
                return Err(DocumentError::OutOfBounds(end));
            }
            addresses.extend(
                table
                    .cells_in_range(*range)
                    .into_iter()
                    .map(|(address, _)| address),
            );
        }
        let changes = addresses
            .into_iter()
            .filter_map(|address| {
                let cell = table.cell(address)?;
                if cell.input == CellInput::Empty
                    && cell.imported_result.is_none()
                    && cell.calculation.is_none()
                {
                    return None;
                }
                let mut cleared = cell.clone();
                cleared.input = CellInput::Empty;
                cleared.imported_result = None;
                cleared.calculation = None;
                Some(CellChange {
                    address,
                    before: Some(cell.clone()),
                    after: Some(cleared),
                })
            })
            .collect::<Vec<_>>();
        if changes.is_empty() {
            return Ok(false);
        }
        self.execute(EditCommand::SetCells { table_id, changes })?;
        Ok(true)
    }
}

#[cfg(test)]
impl Document {
    // Clears input in the selected rectangle, keeps formatting, and applies one undo command.
    pub fn clear_range(
        &mut self,
        table_id: TableId,
        range: CellRange,
    ) -> Result<bool, DocumentError> {
        self.clear_ranges(table_id, &[range])
    }
    /// Replaces selected Fresh formulas with typed values in one undoable document command.
    pub fn replace_formula_results(
        &mut self,
        table_id: TableId,
        range: CellRange,
    ) -> Result<usize, crate::i18n::Message> {
        self.replace_formula_results_in_ranges(table_id, &[range])
    }
}
