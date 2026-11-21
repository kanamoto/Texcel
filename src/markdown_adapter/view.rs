use super::*;

use std::borrow::Cow;

impl MarkdownDocument {
    // Returns the current source bytes, including unchanged document text.
    pub fn source_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the number of cells pulldown emitted from the header and separator width.
    pub fn parser_width(&self, table: usize) -> Option<usize> {
        self.tables.get(table).map(|table| table.parser_width)
    }

    /// Returns explicit GFM alignment for one source column.
    pub fn column_alignment(&self, table: usize, column: usize) -> Option<MarkdownTableAlignment> {
        self.tables
            .get(table)?
            .alignments
            .get(column)
            .copied()
            .flatten()
    }

    /// Returns pulldown's full-document inline parse for one table cell.
    pub fn cell_display(
        &self,
        table: usize,
        row: usize,
        column: usize,
    ) -> Option<&MarkdownCellDisplay> {
        self.displays.get(table)?.get(row)?.get(column)
    }

    /// Returns a cached full-document parse only while the cell still matches its source text.
    pub fn cell_display_for_source(
        &self,
        table: usize,
        row: usize,
        column: usize,
        source: &str,
    ) -> Option<&MarkdownCellDisplay> {
        let source_cell = self.tables.get(table)?.rows.get(row)?.get(column)?;
        (source_cell.value == source)
            .then(|| self.cell_display(table, row, column))
            .flatten()
    }

    /// Returns unbounded display text from the full-document Markdown parser for search.
    pub(crate) fn cell_display_text_for_search(
        &self,
        table: usize,
        row: usize,
        column: usize,
        source_text: &str,
        language: crate::i18n::Language,
    ) -> Result<String, MarkdownError> {
        if let Some(display) = self.cell_display_for_source(table, row, column, source_text) {
            return Ok(display
                .runs
                .iter()
                .filter_map(|run| run.text_for_language(Some(self), language))
                .map(Cow::into_owned)
                .collect());
        }

        // A materialized edit normally has a cache already. Reapply this cell through the
        // adopted full-document parser when it does not, preserving table parsing context.
        let updated = self.apply_changes(&[MarkdownCellChange {
            table,
            row,
            column,
            value: source_text.to_owned(),
        }])?;
        let display = updated
            .cell_display_for_source(table, row, column, source_text)
            .ok_or(MarkdownError::OutputMismatch)?;
        Ok(display
            .runs
            .iter()
            .filter_map(|run| run.text_for_language(Some(&updated), language))
            .map(Cow::into_owned)
            .collect())
    }

    /// Resolves a worker-parsed run to source bytes without copying ordinary cell text.
    pub fn display_run_text<'a>(&'a self, run: &'a MarkdownDisplayRun) -> Option<&'a str> {
        run.text.as_deref().or_else(|| {
            self.bytes
                .get(run.source_range.clone()?)
                .and_then(|bytes| std::str::from_utf8(bytes).ok())
        })
    }
}

#[cfg(test)]
impl MarkdownDocument {
    /// Returns the widest row shown in the grid, including source cells beyond GFM's width.
    pub fn display_width(&self, table: usize) -> Option<usize> {
        self.tables
            .get(table)
            .map(|table| table.rows.iter().map(Vec::len).max().unwrap_or(0))
    }
}
