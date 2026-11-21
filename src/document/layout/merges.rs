use super::{MergeGeometry, MergeRowBlock, TableLayout};
use crate::document::{CellAddress, MERGE_ROW_BLOCK_SIZE, MergedRange};

impl TableLayout {
    /// Iterates over the merged ranges.
    pub fn merges(&self) -> impl Iterator<Item = MergedRange> + '_ {
        self.merge_geometry.merges.iter().copied()
    }

    /// Returns ranges in row blocks whose local interval extent can reach this window.
    pub fn merges_intersecting(
        &self,
        window: MergedRange,
    ) -> impl Iterator<Item = MergedRange> + '_ {
        self.merge_geometry
            .merge_row_blocks
            .range(..=window.end.row / MERGE_ROW_BLOCK_SIZE)
            .filter(move |(_, block)| block.max_end_row >= window.start.row)
            .flat_map(move |(block_index, _)| {
                let block_start = block_index * MERGE_ROW_BLOCK_SIZE;
                let last_row = (block_start + MERGE_ROW_BLOCK_SIZE - 1).min(window.end.row);
                self.merge_geometry
                    .merges_by_start_row
                    .range(block_start..=last_row)
                    .flat_map(|(_, ranges)| ranges.iter().copied())
            })
            .filter(move |range| {
                range.end.row >= window.start.row
                    && range.start.column <= window.end.column
                    && range.end.column >= window.start.column
            })
    }

    /// Returns the merged range containing a cell, if any.
    pub fn merge_at(&self, address: CellAddress) -> Option<MergedRange> {
        self.merges_intersecting(MergedRange {
            start: address,
            end: address,
        })
        .find(|merge| merge.contains(address))
    }

    /// Updates the row-block index for merged ranges.
    pub(super) fn refresh_merge_block(geometry: &mut MergeGeometry, block_index: usize) {
        let start = block_index * MERGE_ROW_BLOCK_SIZE;
        let end = start + MERGE_ROW_BLOCK_SIZE - 1;
        let maximum = geometry
            .merges_by_start_row
            .range(start..=end)
            .flat_map(|(_, ranges)| ranges.iter().map(|range| range.end.row))
            .max();
        if let Some(max_end_row) = maximum {
            geometry
                .merge_row_blocks
                .insert(block_index, MergeRowBlock { max_end_row });
        } else {
            geometry.merge_row_blocks.remove(&block_index);
        }
    }
}
