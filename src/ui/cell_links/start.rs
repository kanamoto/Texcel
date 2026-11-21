use super::*;

impl TexcelApp {
    // Opens the link editor from the active cell and its source representation.
    pub(in crate::ui) fn start_cell_link_dialog(&mut self) {
        if self.edit.is_editing() {
            self.commit_edit(None);
        }
        let Some((document_id, table_id, range, clean_link)) =
            self.workspace.active_document().and_then(|document| {
                let table = document.active_table()?;
                let table_id = table.id;
                let view = document.view(table_id)?;
                if !view.additional_selections.is_empty() {
                    return Some((document.id, table_id, view.selection, None));
                }
                let address = view.active_cell();
                let clean_link = (|| {
                    let index = document
                        .tables
                        .iter()
                        .position(|candidate| candidate.id == table_id)?;
                    if let SourceDocument::Markdown(source) = document.source.as_deref()? {
                        return source
                            .cell_display(index, address.row, address.column)
                            .and_then(|display| {
                                display
                                    .first_link_target
                                    .as_ref()
                                    .map(|target| CellHyperlink {
                                        target: target.clone(),
                                        tooltip: display.first_link_tooltip.clone(),
                                        location: false,
                                    })
                            });
                    }
                    // A CSV cell has no link until it is promoted to Markdown.
                    None
                })();
                Some((document.id, table_id, view.selection, clean_link))
            })
        else {
            return;
        };
        if self.workspace.active_document().is_some_and(|document| {
            document
                .view(table_id)
                .is_some_and(|view| !view.additional_selections.is_empty())
        }) {
            self.status = Some(Message::new("status-link-edit-single-range-required"));
            return;
        }
        let link = clean_link;
        self.cell_link_dialog = Some(CellLinkDialog {
            document_id,
            table_id,
            range,
            target: link
                .as_ref()
                .map_or_else(String::new, |link| link.target.clone()),
            tooltip: link
                .as_ref()
                .and_then(|link| link.tooltip.clone())
                .unwrap_or_default(),
            location: link.as_ref().is_some_and(|link| link.location),
            focus_input: true,
        });
    }
}
