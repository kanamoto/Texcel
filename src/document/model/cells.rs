//!  Document operations for cells.

use std::collections::BTreeSet;

use crate::document::*;

impl Document {
    // Applies one cell change through the shared command and undo history.
    pub fn set_cell(
        &mut self,
        table_id: TableId,
        address: CellAddress,
        after: Option<Cell>,
    ) -> Result<(), DocumentError> {
        let table = self
            .tables
            .iter()
            .find(|table| table.id == table_id)
            .ok_or(DocumentError::MissingTable(table_id))?;
        if address.row >= MAX_ROWS || address.column >= MAX_COLUMNS {
            return Err(DocumentError::OutOfBounds(address));
        }
        let before = table.cell(address).cloned();
        let command = EditCommand::SetCells {
            table_id,
            changes: vec![CellChange {
                address,
                before,
                after,
            }],
        };
        self.execute(command)
    }

    /// Applies a prepared batch of cells in one bounds-checked undoable command.
    pub fn set_cells(
        &mut self,
        table_id: TableId,
        cells: Vec<(CellAddress, Option<Cell>)>,
    ) -> Result<bool, DocumentError> {
        let table = self
            .tables
            .iter()
            .find(|table| table.id == table_id)
            .ok_or(DocumentError::MissingTable(table_id))?;
        let mut addresses = BTreeSet::new();
        let mut changes = Vec::with_capacity(cells.len());
        for (address, after) in cells {
            if address.row >= MAX_ROWS || address.column >= MAX_COLUMNS {
                return Err(DocumentError::OutOfBounds(address));
            }
            if !addresses.insert(address) {
                return Err(DocumentError::DuplicateCell(address));
            }
            let before = table.cell(address).cloned();
            if before != after {
                changes.push(CellChange {
                    address,
                    before,
                    after,
                });
            }
        }
        if changes.is_empty() {
            return Ok(false);
        }
        self.execute(EditCommand::SetCells { table_id, changes })?;
        Ok(true)
    }

    /// Installs worker-built root cells before a single root-swap undo command is committed.
    pub(crate) fn install_prepared_cells(
        &mut self,
        table_id: TableId,
        cells: &[(CellAddress, Option<Cell>)],
    ) -> Result<(), DocumentError> {
        let table = self
            .tables
            .iter_mut()
            .find(|table| table.id == table_id)
            .ok_or(DocumentError::MissingTable(table_id))?;
        for (address, cell) in cells {
            if address.row >= MAX_ROWS || address.column >= MAX_COLUMNS {
                return Err(DocumentError::OutOfBounds(*address));
            }
            table.set_cell(*address, cell.clone());
        }
        Ok(())
    }
}
