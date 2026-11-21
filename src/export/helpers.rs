use super::*;

/// Adds a converted cell with a message that can change with the app language.
pub(super) fn add_conversion_with_message(
    entries: &mut Vec<ConversionEntry>,
    table: &ExportTable,
    address: CellAddress,
    reason: &str,
    message: crate::i18n::Message,
) {
    entries.push(
        ConversionEntry::new(
            ConversionClass::Converted,
            Some(table.order + 1),
            Some(table.name.clone()),
            Some(cell_address(address)),
            reason,
        )
        .with_message(message),
    );
}

/// Adds an omitted cell with a message that can change with the app language.
pub(super) fn add_omission_with_message(
    entries: &mut Vec<ConversionEntry>,
    table: &ExportTable,
    address: CellAddress,
    reason: &str,
    message: crate::i18n::Message,
) {
    entries.push(
        ConversionEntry::new(
            ConversionClass::Omitted,
            Some(table.order + 1),
            Some(table.name.clone()),
            Some(cell_address(address)),
            reason,
        )
        .with_message(message),
    );
}

/// Adds an omitted cell detail that changes presentation only.
pub(super) fn add_formatting_omission_with_message(
    entries: &mut Vec<ConversionEntry>,
    table: &ExportTable,
    address: CellAddress,
    reason: &str,
    message: crate::i18n::Message,
) {
    entries.push(
        ConversionEntry::new(
            ConversionClass::Omitted,
            Some(table.order + 1),
            Some(table.name.clone()),
            Some(cell_address(address)),
            reason,
        )
        .formatting_only()
        .with_message(message),
    );
}

/// Converts a calculation value to its plain text form.
pub(super) fn calc_value_to_string(value: &CalcValue) -> String {
    match value {
        CalcValue::Text(text) | CalcValue::Error(text) => text.clone(),
        CalcValue::Number(value) => value.to_string(),
        CalcValue::Boolean(value) => if *value { "TRUE" } else { "FALSE" }.to_owned(),
    }
}

/// Formats a zero-based cell address in spreadsheet notation.
pub(super) fn cell_address(address: CellAddress) -> String {
    let mut column = address.column + 1;
    let mut label = String::new();
    while column > 0 {
        let remainder = (column - 1) % 26;
        label.insert(0, char::from(b'A' + remainder as u8));
        column = (column - 1) / 26;
    }
    format!("{label}{}", address.row + 1)
}

/// Returns the smallest row and column counts that contain the sheet cells.
pub(super) fn xlsx_cell_dimensions(sheet: &XlsxSheet) -> (usize, usize) {
    let rows = sheet
        .cells
        .keys()
        .map(|address| address.row + 1)
        .max()
        .unwrap_or(0);
    let columns = sheet
        .cells
        .keys()
        .map(|address| address.column + 1)
        .max()
        .unwrap_or(0);
    (rows, columns)
}

/// Escapes Markdown syntax and whitespace at the edges of a table cell.
pub(super) fn escape_markdown_text(value: &str) -> String {
    let characters = value.chars().collect::<Vec<_>>();
    let leading_end = characters
        .iter()
        .position(|character| !matches!(character, ' ' | '\t'))
        .unwrap_or(characters.len());
    let trailing_start = characters
        .iter()
        .rposition(|character| !matches!(character, ' ' | '\t'))
        .map_or(0, |index| index + 1);
    let mut result = String::new();
    for (index, character) in characters.into_iter().enumerate() {
        if index < leading_end || index >= trailing_start {
            match character {
                ' ' => result.push_str("&#32;"),
                '\t' => result.push_str("&#9;"),
                _ => (),
            }
            if matches!(character, ' ' | '\t') {
                continue;
            }
        }
        if character == '&' {
            result.push_str("&amp;");
            continue;
        }
        if matches!(
            character,
            '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '<' | '>' | '~'
        ) {
            result.push('\\');
        }
        result.push(character);
    }
    result
}

/// Keeps a calculated value as literal text in a Markdown cell.
pub(super) fn formula_result_markdown_source(value: &str) -> String {
    crate::document::encode_literal_text(&escape_markdown_text(value))
}

/// Escapes a value for a Markdown table cell.
pub(super) fn escape_table_cell(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    let mut pending_backslashes = 0_usize;
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\\' => pending_backslashes = pending_backslashes.saturating_add(1),
            '\r' => {
                escaped.push_str(&"\\".repeat(pending_backslashes));
                pending_backslashes = 0;
                if characters.peek() == Some(&'\n') {
                    characters.next();
                }
                escaped.push_str("<br>");
            }
            '\n' => {
                escaped.push_str(&"\\".repeat(pending_backslashes));
                pending_backslashes = 0;
                escaped.push_str("<br>");
            }
            '|' => {
                escaped.push_str(&"\\".repeat(pending_backslashes.saturating_mul(2) + 1));
                escaped.push('|');
                pending_backslashes = 0;
            }
            character => {
                escaped.push_str(&"\\".repeat(pending_backslashes));
                pending_backslashes = 0;
                escaped.push(character);
            }
        }
    }
    escaped.push_str(&"\\".repeat(pending_backslashes));
    if pending_backslashes > 0 {
        escaped.push(' ');
    }
    escaped
}

/// Builds the CSV output path for one table.
pub(super) fn per_table_csv_path(
    base: &Path,
    table_name: &str,
    table_id: TableId,
    ordinal: Option<usize>,
) -> PathBuf {
    let stem = base.file_stem().map_or_else(
        || "export".to_owned(),
        |value| value.to_string_lossy().into_owned(),
    );
    let safe_name: String = table_name
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    let safe_name = if safe_name.trim_matches('_').is_empty() {
        format!("table-{table_id}")
    } else {
        safe_name
    };
    let ordinal = ordinal.map_or_else(String::new, |ordinal| format!("-{ordinal}"));
    base.with_file_name(format!("{stem}-{safe_name}{ordinal}.csv"))
}

/// Finds a unique path for one table CSV file.
pub(super) fn allocate_per_table_csv_path(
    base: &Path,
    table: &ExportTable,
    requested_suffix: Option<usize>,
    used_paths: &mut BTreeSet<String>,
) -> Option<(PathBuf, bool)> {
    let path = per_table_csv_path(base, &table.name, table.id, requested_suffix);
    if used_paths.insert(path_comparison_key(&path)) {
        return Some((path, false));
    }

    let mut suffix = match requested_suffix {
        Some(suffix) => suffix.checked_add(1)?,
        None => table.order.checked_add(1)?,
    };
    loop {
        let path = per_table_csv_path(base, &table.name, table.id, Some(suffix));
        if used_paths.insert(path_comparison_key(&path)) {
            return Some((path, true));
        }
        suffix = suffix.checked_add(1)?;
    }
}

/// Checks whether exported table names repeat.
pub(super) fn duplicate_table_names(tables: &[ExportTable]) -> BTreeSet<TableId> {
    let mut counts = BTreeMap::<String, usize>::new();
    let names = tables
        .iter()
        .map(|table| {
            let name: String = table
                .name
                .chars()
                .map(|character| {
                    if character.is_alphanumeric() || character == '-' || character == '_' {
                        character
                    } else {
                        '_'
                    }
                })
                .collect();
            if name.trim_matches('_').is_empty() {
                format!("table-{}", table.id)
            } else {
                name
            }
        })
        .collect::<Vec<_>>();
    for name in &names {
        *counts.entry(path_name_key(name)).or_default() += 1;
    }
    tables
        .iter()
        .zip(names)
        .filter_map(|(table, name)| (counts[&path_name_key(&name)] > 1).then_some(table.id))
        .collect()
}

/// Builds a normalized key for comparing output names.
pub(super) fn path_name_key(name: &str) -> String {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        name.to_lowercase()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        name.to_owned()
    }
}

/// Describes the export settings in a stable order.
pub(super) fn settings_summary(
    options: ExportOptions,
    filename: &str,
    selected_table_name: Option<&str>,
) -> crate::i18n::Message {
    crate::i18n::Message::new("report-settings-summary")
        .with_text("path", filename)
        .with_message(
            "scope",
            match options.scope {
                ExportScope::AllTables => crate::i18n::Message::new("report-scope-all"),
                ExportScope::Selected(_) => selected_table_name.map_or_else(
                    || crate::i18n::Message::new("report-scope-selected"),
                    |name| {
                        crate::i18n::Message::new("report-scope-selected-named")
                            .with_text("name", name)
                    },
                ),
            },
        )
        .with_message(
            "formulas",
            crate::i18n::Message::new(match options.formulas {
                FormulaOutput::Formula => "report-formula-source",
                FormulaOutput::LatestResult => "report-formula-latest",
            }),
        )
        .with_message(
            "modifiers",
            crate::i18n::Message::new(match options.modifiers {
                ModifierOutput::Convert => "report-modifiers-convert",
                ModifierOutput::TextOnly => "report-modifiers-text-only",
            }),
        )
        .with_message(
            "header",
            crate::i18n::Message::new(match options.markdown_header {
                MarkdownHeader::UseFirstRow => "report-markdown-header-first",
                MarkdownHeader::AddBlankHeader => "report-markdown-header-blank",
            }),
        )
}

/// Checks whether two paths name the same normalized destination.
pub(super) fn same_normalized_path(left: &Path, right: &Path) -> bool {
    path_comparison_key(left) == path_comparison_key(right)
}

/// Builds a comparison key for a path.
pub(super) fn path_comparison_key(path: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let normalized = std::fs::canonicalize(&absolute).unwrap_or_else(|_| normalize_path(&absolute));
    let text = normalized.to_string_lossy().into_owned();
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        text.to_lowercase()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        text
    }
}

/// Normalizes path components without requiring the file to exist.
pub(super) fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => (),
            std::path::Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}
