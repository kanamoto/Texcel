use super::*;

/// A sheet rule that Markdown cannot keep, so the import report can list it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum XlsxRuleKind {
    Filter,
    Validation,
    ConditionalFormat,
}

/// One filter, input rule, or conditional format found in a sheet, with the cells it covers.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct XlsxRuleLoss {
    pub kind: XlsxRuleKind,
    /// `None` when the rule's cell range cannot be read.
    pub range: Option<CellRange>,
}

/// Lists the AutoFilter, data validations, and conditional formats of one sheet.
pub(super) fn scan_sheet_rules(sheet: &umya_spreadsheet::Worksheet) -> Vec<XlsxRuleLoss> {
    let mut losses = Vec::new();
    if let Some(auto_filter) = sheet.auto_filter() {
        losses.push(XlsxRuleLoss {
            kind: XlsxRuleKind::Filter,
            range: parse_merged_range(&auto_filter.range().range()).map(|range| CellRange {
                anchor: range.start,
                head: range.end,
            }),
        });
    }
    if let Some(validations) = sheet.data_validations() {
        for validation in validations.data_validation_list() {
            push_ranges(
                &mut losses,
                XlsxRuleKind::Validation,
                validation.sequence_of_references().get_sqref(),
            );
        }
    }
    for formatting in sheet.conditional_formatting_collection() {
        push_ranges(
            &mut losses,
            XlsxRuleKind::ConditionalFormat,
            formatting.sequence_of_references().get_sqref(),
        );
    }
    losses
}

/// Adds one loss per range in a space-separated reference list, or one without a range.
fn push_ranges(losses: &mut Vec<XlsxRuleLoss>, kind: XlsxRuleKind, references: String) {
    let ranges = references
        .split_whitespace()
        .filter_map(|reference| {
            parse_merged_range(reference)
                .map(|range| CellRange {
                    anchor: range.start,
                    head: range.end,
                })
                .or_else(|| parse_cell_reference(reference).map(CellRange::single))
        })
        .collect::<Vec<_>>();
    if ranges.is_empty() {
        losses.push(XlsxRuleLoss { kind, range: None });
    }
    losses.extend(ranges.into_iter().map(|range| XlsxRuleLoss {
        kind,
        range: Some(range),
    }));
}
