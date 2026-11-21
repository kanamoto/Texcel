//!  Worker-side operations for duplicates.

use super::*;
use crate::i18n::Message;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum DuplicateValue {
    Text(String),
    Number(u64),
    Boolean(bool),
    Error(String),
    Formula(String),
}

/// The rows to compare for duplicates and whether the later copies should be deleted.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DuplicateScan {
    pub(crate) table_id: TableId,
    pub(crate) range: CellRange,
    pub(crate) header_row: Option<usize>,
    pub(crate) delete: bool,
}

// Keep the worker snapshot, selected range, delete confirmation, and cancellation token explicit.
pub(crate) fn prepare_duplicates(
    request: &DataEditRequest,
    document: &Document,
    source: Option<&SourceDocument>,
    scan: DuplicateScan,
    cancelled: &AtomicBool,
) -> Result<PreparedDataEdit, Message> {
    let DuplicateScan {
        table_id,
        range,
        header_row,
        delete,
    } = scan;
    let table = find_table(document, table_id)?;
    let (row_count, column_count) =
        used_dimensions(source, table_id, &request.structure.tables, table);
    let (start, raw_end) = range.normalized();
    if start.row >= row_count || start.column >= column_count {
        return Ok(PreparedDataEdit {
            document_id: request.structure.document_id,
            document_revision: request.structure.revision,
            calculation_revision: request.structure.calculation_revision,
            changed_cells: 0,
            changed_rows: Some(0),
            duplicate_count: 0,
            duplicate_groups: 0,
            duplicate_rows: Vec::new(),
            preview: Vec::new(),
            preview_truncated: false,
            prepared: None,
        });
    }
    let end = CellAddress {
        row: raw_end.row.min(row_count - 1),
        column: raw_end.column.min(column_count - 1),
    };
    if header_row.is_some_and(|row| row >= row_count) {
        return Err(Message::new("status-duplicate-header-out-of-range"));
    }

    let mut row_values = BTreeMap::<usize, Vec<(usize, DuplicateValue)>>::new();
    for (address, cell) in table.stored_cells_in_range(CellRange {
        anchor: start,
        head: end,
    }) {
        check_cancelled(cancelled)?;
        if matches!(&cell.input, CellInput::Number(number) if !number.is_finite()) {
            return Err(Message::new("status-duplicate-nonfinite-number"));
        }
        let Some(value) = duplicate_value(table, address, cell) else {
            continue;
        };
        row_values
            .entry(address.row)
            .or_default()
            .push((address.column, value));
    }
    let mut seen_keys = HashSet::<Vec<(usize, DuplicateValue)>>::new();
    let mut repeated_keys = HashSet::<Vec<(usize, DuplicateValue)>>::new();
    let mut duplicate_rows = Vec::<DuplicateRowPreview>::new();
    let mut duplicate_sources = BTreeSet::new();
    let data_start = start.row;
    for row in data_start..=end.row {
        if row % 1024 == 0 {
            check_cancelled(cancelled)?;
        }
        if Some(row) == header_row {
            continue;
        }
        let key = row_values
            .remove(&row)
            .unwrap_or_default()
            .into_iter()
            .collect::<Vec<_>>();
        // Rows with no selected values do not count as duplicates.
        if key.is_empty() {
            continue;
        }
        if !seen_keys.insert(key.clone()) {
            duplicate_sources.insert(row);
            repeated_keys.insert(key.clone());
            if duplicate_rows.len() < 10_000 {
                duplicate_rows.push(DuplicateRowPreview {
                    row,
                    key_values: key
                        .iter()
                        .map(|(column, value)| (*column, duplicate_value_label(value)))
                        .collect(),
                });
            }
        }
    }
    if duplicate_sources.is_empty() {
        return Ok(PreparedDataEdit {
            document_id: request.structure.document_id,
            document_revision: request.structure.revision,
            calculation_revision: request.structure.calculation_revision,
            changed_cells: 0,
            changed_rows: Some(0),
            duplicate_count: 0,
            duplicate_groups: 0,
            duplicate_rows: Vec::new(),
            preview: Vec::new(),
            preview_truncated: false,
            prepared: None,
        });
    }

    let duplicate_groups = repeated_keys.len();
    if !delete {
        return Ok(PreparedDataEdit {
            document_id: request.structure.document_id,
            document_revision: request.structure.revision,
            calculation_revision: request.structure.calculation_revision,
            changed_cells: 0,
            changed_rows: None,
            duplicate_count: duplicate_sources.len(),
            duplicate_groups,
            duplicate_rows,
            preview: Vec::new(),
            preview_truncated: duplicate_sources.len() > 10_000,
            prepared: None,
        });
    }

    let mut row_sources = Vec::with_capacity(row_count.saturating_sub(duplicate_sources.len()));
    for row in 0..row_count {
        if !duplicate_sources.contains(&row) {
            row_sources.push(row);
        }
    }
    if row_sources.is_empty() {
        return Err(Message::new("status-duplicate-delete-all-rows"));
    }
    let operation = StructureOperation::ReorderRows {
        table_id,
        source_row_count: row_count,
        row_sources,
    };
    let mut structure = request.structure.clone();
    structure.operation = Some(operation);
    let prepared = prepare_structure(&structure, cancelled)?;
    let after_table = prepared
        .tables
        .iter()
        .find(|table| table.id == table_id)
        .ok_or_else(|| Message::new("status-duplicate-result-missing"))?;
    let mut addresses = table
        .stored_cells()
        .map(|(address, _)| address)
        .chain(after_table.stored_cells().map(|(address, _)| address))
        .collect::<BTreeSet<_>>();
    let mut preview = Vec::new();
    let mut changed_cells = 0;
    while let Some(address) = addresses.pop_first() {
        check_cancelled(cancelled)?;
        let before = table.cell(address);
        let after = after_table.cell(address);
        if before == after {
            continue;
        }
        changed_cells += 1;
        if preview.len() < 500 {
            preview.push(DataCellDiff {
                table_id,
                address,
                before: before.map_or_else(String::new, |cell| cell.input.source_text()),
                after: after.map_or_else(String::new, |cell| cell.input.source_text()),
            });
        }
    }
    if after_table.stored_cells().next().is_none() {
        return Err(Message::new("status-duplicate-delete-empty-table"));
    }
    let duplicate_preview_truncated = duplicate_sources.len() > duplicate_rows.len();
    Ok(PreparedDataEdit {
        document_id: request.structure.document_id,
        document_revision: request.structure.revision,
        calculation_revision: request.structure.calculation_revision,
        changed_cells,
        changed_rows: Some(duplicate_sources.len()),
        duplicate_count: duplicate_sources.len(),
        duplicate_groups,
        duplicate_rows,
        preview,
        preview_truncated: changed_cells > 500 || duplicate_preview_truncated,
        prepared: Some(prepared),
    })
}

/// Returns the normalized value used to find duplicate rows.
fn duplicate_value(
    table: &crate::document::Table,
    address: CellAddress,
    cell: &Cell,
) -> Option<DuplicateValue> {
    if is_formula_cell(table, address, cell) {
        return crate::calc::formula_source(&cell.input, table.is_string_cell(address))
            .map(DuplicateValue::Formula);
    }
    match &cell.input {
        CellInput::Empty => None,
        CellInput::Text(text) if text.is_empty() => None,
        CellInput::Text(text) => Some(DuplicateValue::Text(text.clone())),
        CellInput::Number(number) if number.is_finite() => {
            Some(DuplicateValue::Number(if *number == 0.0 {
                0.0_f64.to_bits()
            } else {
                number.to_bits()
            }))
        }
        CellInput::Number(_) => None,
        CellInput::Boolean(value) => Some(DuplicateValue::Boolean(*value)),
        CellInput::Error(value) => Some(DuplicateValue::Error(value.clone())),
        CellInput::Formula(formula) => Some(DuplicateValue::Formula(formula.clone())),
    }
}

/// Formats a duplicate value for the preview.
fn duplicate_value_label(value: &DuplicateValue) -> String {
    match value {
        DuplicateValue::Text(value) => value.clone(),
        DuplicateValue::Number(value) => f64::from_bits(*value).to_string(),
        DuplicateValue::Boolean(value) => if *value { "TRUE" } else { "FALSE" }.to_owned(),
        DuplicateValue::Error(value) | DuplicateValue::Formula(value) => value.clone(),
    }
}

/// Converts a calculated value to a stable sort value.
pub(crate) fn calc_value_sort_atom(
    value: &crate::document::CalcValue,
    kind: SortValueKind,
) -> SortAtom {
    match value {
        crate::document::CalcValue::Text(text) => sort_text_atom(text, kind),
        crate::document::CalcValue::Number(number) => sort_number_atom(*number, kind),
        crate::document::CalcValue::Boolean(value) => {
            sort_text_atom(if *value { "TRUE" } else { "FALSE" }, kind)
        }
        crate::document::CalcValue::Error(_) => SortAtom::Invalid,
    }
}

/// Builds a comparison value for text sorting.
pub(crate) fn sort_text_atom(value: &str, kind: SortValueKind) -> SortAtom {
    if value.is_empty() {
        return SortAtom::Blank;
    }
    match kind {
        SortValueKind::Text => SortAtom::Text(value.to_owned()),
        SortValueKind::Number => crate::numeric_text::parse_ascii_decimal(value)
            .map_or(SortAtom::Invalid, SortAtom::Number),
        SortValueKind::Date => parse_iso_date(value).map_or(SortAtom::Invalid, SortAtom::Date),
    }
}

/// Builds a comparison value for number sorting.
pub(crate) fn sort_number_atom(value: f64, kind: SortValueKind) -> SortAtom {
    if !value.is_finite() {
        return SortAtom::Invalid;
    }
    match kind {
        SortValueKind::Text => SortAtom::Text(value.to_string()),
        SortValueKind::Number => SortAtom::Number(value),
        // Editing cells never carry a number format, so a number is not a date here.
        SortValueKind::Date => SortAtom::Invalid,
    }
}

/// Compares two values using the selected sort order.
pub(crate) fn compare_sort_atoms(
    left: &SortAtom,
    right: &SortAtom,
    descending: bool,
) -> std::cmp::Ordering {
    let class = |value: &SortAtom| match value {
        SortAtom::Text(_) | SortAtom::Number(_) | SortAtom::Date(_) => 0,
        SortAtom::Invalid => 1,
        SortAtom::Blank => 2,
    };
    let class_order = class(left).cmp(&class(right));
    if !class_order.is_eq() {
        return class_order;
    }
    let value_order = match (left, right) {
        (SortAtom::Text(left), SortAtom::Text(right)) => left.cmp(right),
        (SortAtom::Number(left), SortAtom::Number(right)) => left.total_cmp(right),
        (SortAtom::Date(left), SortAtom::Date(right)) => left.cmp(right),
        _ => std::cmp::Ordering::Equal,
    };
    if descending && class(left) == 0 {
        value_order.reverse()
    } else {
        value_order
    }
}

/// Parses an ISO date into a day count.
fn parse_iso_date(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return None;
    }
    let year = value[..4].parse::<i32>().ok()?;
    let month = value[5..7].parse::<u32>().ok()?;
    let day = value[8..10].parse::<u32>().ok()?;
    date_key(year, month, day)
}

/// Builds a stable key for a parsed date.
fn date_key(year: i32, month: u32, day: u32) -> Option<i64> {
    if !(1..=9999).contains(&year) || !(1..=12).contains(&month) {
        return None;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if day == 0 || day > month_days {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

/// Converts a calendar date to a day count.
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let mut year = i64::from(year);
    let month = i64::from(month);
    let day = i64::from(day);
    year -= i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let shifted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}
