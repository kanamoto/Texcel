use super::*;

pub(super) struct WorksheetCellScan {
    pub(super) reference: String,
    pub(super) is_shared_string: bool,
    pub(super) value_text: String,
    pub(super) inside_value: bool,
}

pub(super) struct WorksheetScanMetadata<'a> {
    pub(super) part: &'a str,
    pub(super) sheet_name: &'a str,
    pub(super) sheet_order: usize,
    pub(super) limits: XlsxPackageLimits,
}

// Scan one worksheet for dimensions, cells, and unsupported features.
pub(super) fn scan_worksheet(
    bytes: &[u8],
    metadata: WorksheetScanMetadata<'_>,
    total_before: usize,
    shared_string_lengths: &[u64],
    effective_expanded_bytes: &mut u64,
) -> Result<SheetScanResult, XlsxScanError> {
    let WorksheetScanMetadata {
        part,
        sheet_name,
        sheet_order,
        limits,
    } = metadata;
    let known_elements = [
        "worksheet",
        "dimension",
        "sheetViews",
        "sheetView",
        "pane",
        "selection",
        "sheetFormatPr",
        "cols",
        "col",
        "sheetData",
        "row",
        "c",
        "f",
        "v",
        "is",
        "t",
        "r",
        "rPr",
        "rFont",
        "b",
        "i",
        "strike",
        "color",
        "u",
        "sz",
        "font",
        "alignment",
        "mergeCells",
        "mergeCell",
        "autoFilter",
        "filterColumn",
        "filters",
        "filter",
        "customFilters",
        "customFilter",
        "conditionalFormatting",
        "cfRule",
        "formula",
        "dxf",
        "fill",
        "patternFill",
        "fgColor",
        "bgColor",
        "dataValidations",
        "dataValidation",
        "formula1",
        "formula2",
        "hyperlinks",
        "hyperlink",
        "sheetProtection",
        "pageMargins",
        "pageSetup",
        "printOptions",
        "headerFooter",
        "legacyDrawing",
        "legacyDrawingHF",
        "drawing",
        "picture",
        "tableParts",
        "tablePart",
        "extLst",
        "ext",
        "ignoredErrors",
        "ignoredError",
        "rowBreaks",
        "colBreaks",
        "brk",
        "oddHeader",
        "oddFooter",
        "evenHeader",
        "evenFooter",
        "firstHeader",
        "firstFooter",
        "customSheetViews",
        "customSheetView",
        "controls",
        "control",
        "oleObjects",
        "oleObject",
        "webPublishItems",
        "phoneticPr",
        "phoneticRun",
        "rPh",
        "protectedRanges",
        "protectedRange",
        "smartTags",
        "smartTag",
        "smartTagPr",
        "smartTagType",
        "scenarios",
        "scenario",
        "sheetCalcPr",
        "cellWatches",
        "cellWatch",
        "pivotTableParts",
        "pivotTablePart",
        "extLst",
    ];
    let unsupported_elements = [
        "sheetProtection",
        "pageMargins",
        "pageSetup",
        "printOptions",
        "headerFooter",
        "legacyDrawing",
        "legacyDrawingHF",
        "drawing",
        "picture",
        "tableParts",
        "tablePart",
        "extLst",
        "ignoredErrors",
        "rowBreaks",
        "colBreaks",
        "brk",
        "oddHeader",
        "oddFooter",
        "evenHeader",
        "evenFooter",
        "firstHeader",
        "firstFooter",
        "customSheetViews",
        "controls",
        "oleObjects",
        "webPublishItems",
        "protectedRanges",
        "scenarios",
        "cellWatches",
        "pivotTableParts",
    ];
    let mut reader = Reader::from_reader(bytes);
    let mut element_path = Vec::new();
    let mut entries = Vec::new();
    let mut cell_count = 0_usize;
    let mut current_cell: Option<WorksheetCellScan> = None;
    loop {
        match reader.read_event() {
            Ok(event) if matches!(event, Event::Start(_) | Event::Empty(_)) => {
                let is_empty = matches!(event, Event::Empty(_));
                let element = match event {
                    Event::Start(element) | Event::Empty(element) => element,
                    _ => continue,
                };
                let name = local_name(element.name().as_ref());
                let parent = element_path.last().map(String::as_str);
                let attributes = element
                    .attributes()
                    .map(|attribute| {
                        let attribute = attribute.map_err(|error| XlsxScanError::InvalidXml {
                            part: part.to_owned(),
                            reason: error.to_string(),
                        })?;
                        let key = String::from_utf8_lossy(attribute.key.as_ref()).into_owned();
                        let value = attribute
                            .decoded_and_normalized_value(XmlVersion::Implicit1_0, reader.decoder())
                            .map_err(|error| XlsxScanError::InvalidXml {
                                part: part.to_owned(),
                                reason: error.to_string(),
                            })?
                            .into_owned();
                        Ok((key, value))
                    })
                    .collect::<Result<BTreeMap<_, _>, XlsxScanError>>()?;
                if name == "dimension" {
                    if let Some(reference) = attributes.get("ref") {
                        validate_range(reference, limits)?;
                    }
                } else if name == "row" {
                    if let Some(value) = attributes.get("r") {
                        let row = value.parse::<usize>().map_err(|_| {
                            XlsxScanError::InvalidPackage(
                                crate::i18n::Message::new("status-xlsx-package-row-index-invalid")
                                    .with_text("part", part)
                                    .with_text("value", value),
                            )
                        })?;
                        if row == 0 || row > limits.max_rows {
                            return Err(limit_error(
                                XlsxResourceLimit::Rows,
                                limits.max_rows as u64,
                            ));
                        }
                    }
                } else if name == "col" {
                    let minimum = attributes
                        .get("min")
                        .map(|value| value.parse::<usize>())
                        .transpose()
                        .map_err(|_| {
                            XlsxScanError::InvalidPackage(
                                crate::i18n::Message::new(
                                    "status-xlsx-package-column-index-invalid",
                                )
                                .with_text("part", part),
                            )
                        })?;
                    let maximum = attributes
                        .get("max")
                        .map(|value| value.parse::<usize>())
                        .transpose()
                        .map_err(|_| {
                            XlsxScanError::InvalidPackage(
                                crate::i18n::Message::new(
                                    "status-xlsx-package-column-index-invalid",
                                )
                                .with_text("part", part),
                            )
                        })?;
                    for column in [minimum, maximum].into_iter().flatten() {
                        if column == 0 || column > limits.max_columns {
                            return Err(limit_error(
                                XlsxResourceLimit::Columns,
                                limits.max_columns as u64,
                            ));
                        }
                    }
                    if minimum
                        .zip(maximum)
                        .is_some_and(|(minimum, maximum)| minimum > maximum)
                    {
                        return Err(XlsxScanError::InvalidPackage(
                            crate::i18n::Message::new("status-xlsx-package-column-range-invalid")
                                .with_text("part", part),
                        ));
                    }
                } else if name == "c" {
                    let reference = attributes.get("r").ok_or_else(|| {
                        XlsxScanError::InvalidPackage(
                            crate::i18n::Message::new("status-xlsx-package-cell-reference-missing")
                                .with_text("part", part),
                        )
                    })?;
                    validate_range(reference, limits)?;
                    cell_count = cell_count.saturating_add(1);
                    if total_before.saturating_add(cell_count) > limits.max_cells {
                        return Err(limit_error(
                            XlsxResourceLimit::Cells,
                            limits.max_cells as u64,
                        ));
                    }
                    current_cell = Some(WorksheetCellScan {
                        reference: reference.clone(),
                        is_shared_string: attributes.get("t").is_some_and(|value| value == "s"),
                        value_text: String::new(),
                        inside_value: false,
                    });
                } else if name == "v"
                    && current_cell
                        .as_ref()
                        .is_some_and(|cell| cell.is_shared_string)
                    && let Some(cell) = &mut current_cell
                {
                    cell.value_text.clear();
                    cell.inside_value = true;
                }
                let range = attributes
                    .get("ref")
                    .or_else(|| attributes.get("sqref"))
                    .cloned()
                    .or_else(|| current_cell.as_ref().map(|cell| cell.reference.clone()));
                if !known_elements.contains(&name.as_str()) {
                    push_report_entry(
                        &mut entries,
                        ConversionEntry::new(
                            ConversionClass::Omitted,
                            Some(sheet_order),
                            Some(sheet_name.to_owned()),
                            range,
                            format!("未分類の非対応XML要素: {name}"),
                        )
                        .with_message(
                            Message::new("report-reason-xlsx-unknown-element")
                                .with_text("part", part)
                                .with_text("element", &name),
                        ),
                        limits,
                    )?;
                } else if unsupported_elements.contains(&name.as_str()) {
                    let entry = ConversionEntry::new(
                        ConversionClass::Omitted,
                        Some(sheet_order),
                        Some(sheet_name.to_owned()),
                        range,
                        unsupported_element_reason(&name),
                    )
                    .with_message(unsupported_element_message(&name).with_text("part", part));
                    let entry = if is_print_formatting_element(&name, parent) {
                        entry.formatting_only()
                    } else {
                        entry
                    };
                    push_report_entry(&mut entries, entry, limits)?;
                }
                if is_empty && name == "c" {
                    current_cell = None;
                } else if is_empty
                    && name == "v"
                    && let Some(cell) = &mut current_cell
                {
                    cell.inside_value = false;
                }
                if !is_empty {
                    element_path.push(name);
                }
            }
            Ok(Event::Text(text))
                if current_cell.as_ref().is_some_and(|cell| cell.inside_value) =>
            {
                let decoded = text.decode().map_err(|error| XlsxScanError::InvalidXml {
                    part: part.to_owned(),
                    reason: error.to_string(),
                })?;
                let Some(cell) = &mut current_cell else {
                    continue;
                };
                cell.value_text.push_str(&decoded);
                if cell.value_text.len() > 32 {
                    return Err(XlsxScanError::InvalidPackage(
                        crate::i18n::Message::new("status-xlsx-shared-string-reference-invalid")
                            .with_text("cell", &cell.reference),
                    ));
                }
            }
            Ok(Event::End(element)) => {
                let name = local_name(element.name().as_ref());
                if name == "v"
                    && let Some(cell) = &mut current_cell
                    && cell.is_shared_string
                    && cell.inside_value
                {
                    let index = cell.value_text.parse::<usize>().map_err(|_| {
                        XlsxScanError::InvalidPackage(
                            crate::i18n::Message::new("status-xlsx-shared-string-index-invalid")
                                .with_text("cell", &cell.reference),
                        )
                    })?;
                    let length = shared_string_lengths.get(index).ok_or_else(|| {
                        XlsxScanError::InvalidPackage(
                            crate::i18n::Message::new(
                                "status-xlsx-shared-string-index-out-of-range",
                            )
                            .with_text("cell", &cell.reference),
                        )
                    })?;
                    *effective_expanded_bytes = effective_expanded_bytes
                        .checked_add(*length)
                        .ok_or_else(|| {
                            limit_error(
                                XlsxResourceLimit::SharedStringExpandedBytes,
                                limits.max_expanded_bytes,
                            )
                        })?;
                    if *effective_expanded_bytes > limits.max_expanded_bytes {
                        return Err(limit_error(
                            XlsxResourceLimit::SharedStringExpandedBytes,
                            limits.max_expanded_bytes,
                        ));
                    }
                    cell.inside_value = false;
                }
                if name == "c" {
                    current_cell = None;
                }
                element_path.pop();
            }
            Ok(Event::Eof) => break,
            Ok(_) => (),
            Err(error) => {
                return Err(XlsxScanError::InvalidXml {
                    part: part.to_owned(),
                    reason: error.to_string(),
                });
            }
        }
    }
    Ok(SheetScanResult {
        sheet_cell_count: cell_count,
        total_cells: total_before.saturating_add(cell_count),
        entries,
    })
}

/// Marks worksheet print layout features as formatting-only losses.
fn is_print_formatting_element(name: &str, parent: Option<&str>) -> bool {
    matches!(
        name,
        "pageMargins" | "pageSetup" | "printOptions" | "headerFooter" | "rowBreaks" | "colBreaks"
    ) || (name == "brk" && matches!(parent, Some("rowBreaks" | "colBreaks")))
        || (matches!(
            name,
            "oddHeader" | "oddFooter" | "evenHeader" | "evenFooter" | "firstHeader" | "firstFooter"
        ) && parent == Some("headerFooter"))
}
