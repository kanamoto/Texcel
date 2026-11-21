use super::*;

impl TexcelApp {
    // Keep removed tables until dependent worker results are finished.
    pub(super) fn queue_retired_structure_tables(&mut self, tables: Vec<crate::document::Table>) {
        if let Err(tables) = self.file_worker.discard_structure_tables(tables) {
            self.retired_structure_roots.push(tables);
        }
    }

    // Release removed tables that no longer have pending worker results.
    pub(super) fn flush_retired_structure_tables(&mut self) {
        let pending = std::mem::take(&mut self.retired_structure_roots);
        for tables in pending {
            self.queue_retired_structure_tables(tables);
        }
    }
}
