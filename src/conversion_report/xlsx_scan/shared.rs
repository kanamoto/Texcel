use super::*;

pub(super) struct SharedStringScanResult {
    pub(super) lengths: Vec<u64>,
    pub(super) entries: Vec<ConversionEntry>,
}

/// Shared strings are measured without retaining text; the XLSX reader materializes each value
/// and its rich-text run data for every referencing cell, so that cost is included in the budget.
pub(super) fn scan_shared_strings(
    bytes: &[u8],
    part: &str,
    limits: XlsxPackageLimits,
) -> Result<SharedStringScanResult, XlsxScanError> {
    let known_elements = [
        "sst",
        "si",
        "t",
        "r",
        "rPr",
        "rFont",
        "b",
        "i",
        "strike",
        "color",
        "sz",
        "vertAlign",
        "u",
        "charset",
        "family",
        "scheme",
        "phoneticPr",
        "phoneticRun",
        "rPh",
        "extLst",
    ];
    let mut reader = Reader::from_reader(bytes);
    let mut lengths = Vec::new();
    let mut entries = Vec::new();
    let mut reported = BTreeSet::new();
    let mut current_length = None::<u64>;
    let mut inside_text = false;
    let mut phonetic_depth = 0_usize;
    let mut inside_run = false;

    loop {
        match reader.read_event() {
            Ok(event) if matches!(event, Event::Start(_) | Event::Empty(_)) => {
                let is_empty = matches!(event, Event::Empty(_));
                let element = match event {
                    Event::Start(element) | Event::Empty(element) => element,
                    _ => continue,
                };
                let name = local_name(element.name().as_ref());
                if name == "si" && current_length.is_none() {
                    current_length = Some(SHARED_STRING_VALUE_OVERHEAD);
                }
                if name == "r" && current_length.is_some() && phonetic_depth == 0 {
                    account_shared_string_bytes(
                        &mut current_length,
                        RICH_TEXT_RUN_OVERHEAD,
                        limits,
                    )?;
                    inside_run = true;
                }
                if inside_run && name == "rPr" {
                    account_shared_string_bytes(
                        &mut current_length,
                        RICH_TEXT_PROPERTIES_OVERHEAD,
                        limits,
                    )?;
                }
                if inside_run {
                    let attribute_bytes =
                        element.attributes().try_fold(0_u64, |total, attribute| {
                            let attribute =
                                attribute.map_err(|error| XlsxScanError::InvalidXml {
                                    part: part.to_owned(),
                                    reason: error.to_string(),
                                })?;
                            Ok::<_, XlsxScanError>(
                                total
                                    .saturating_add(attribute.key.as_ref().len() as u64)
                                    .saturating_add(attribute.value.len() as u64),
                            )
                        })?;
                    account_shared_string_bytes(&mut current_length, attribute_bytes, limits)?;
                }
                if !known_elements.contains(&name.as_str()) || name == "extLst" {
                    let reason = if name == "extLst" {
                        format!("拡張XML要素は出力へ引き継がれません: {part}:{name}")
                    } else {
                        format!("未分類の非対応XML要素: {part}:{name}")
                    };
                    if reported.insert(reason.clone()) {
                        push_report_entry(
                            &mut entries,
                            ConversionEntry::new(
                                ConversionClass::Omitted,
                                None,
                                None,
                                None,
                                reason,
                            )
                            .with_message(
                                Message::new(if name == "extLst" {
                                    "report-reason-xlsx-extension-element"
                                } else {
                                    "report-reason-xlsx-unknown-element"
                                })
                                .with_text("part", part)
                                .with_text("element", &name),
                            ),
                            limits,
                        )?;
                    }
                }
                match name.as_str() {
                    "rPh" => phonetic_depth = phonetic_depth.saturating_add(1),
                    "t" => inside_text = current_length.is_some() && phonetic_depth == 0,
                    _ => (),
                }
                if is_empty {
                    if name == "si" {
                        let length = current_length
                            .take()
                            .unwrap_or(SHARED_STRING_VALUE_OVERHEAD);
                        push_shared_string_length(&mut lengths, length, limits)?;
                    } else if name == "r" {
                        inside_run = false;
                    } else if name == "t" {
                        inside_text = false;
                    } else if name == "rPh" {
                        phonetic_depth = phonetic_depth.saturating_sub(1);
                    }
                }
            }
            Ok(Event::Text(text)) if inside_text => {
                account_shared_string_bytes(&mut current_length, text.len() as u64, limits)?;
            }
            Ok(Event::CData(text)) if inside_text => {
                account_shared_string_bytes(&mut current_length, text.len() as u64, limits)?;
            }
            Ok(Event::GeneralRef(reference)) if inside_text => {
                let expanded_bytes = if let Some(character) =
                    reference
                        .resolve_char_ref()
                        .map_err(|error| XlsxScanError::InvalidXml {
                            part: part.to_owned(),
                            reason: error.to_string(),
                        })? {
                    character.len_utf8() as u64
                } else {
                    let entity = reference
                        .decode()
                        .map_err(|error| XlsxScanError::InvalidXml {
                            part: part.to_owned(),
                            reason: error.to_string(),
                        })?;
                    quick_xml::escape::resolve_xml_entity(&entity)
                        .ok_or_else(|| {
                            XlsxScanError::InvalidPackage(
                                crate::i18n::Message::new(
                                    "status-xlsx-shared-string-entity-unresolved",
                                )
                                .with_text("entity", entity),
                            )
                        })?
                        .len() as u64
                };
                account_shared_string_bytes(&mut current_length, expanded_bytes, limits)?;
            }
            Ok(Event::End(element)) => match local_name(element.name().as_ref()).as_str() {
                "t" => inside_text = false,
                "rPh" => phonetic_depth = phonetic_depth.saturating_sub(1),
                "r" => inside_run = false,
                "si" => {
                    let length = current_length.take().ok_or_else(|| {
                        XlsxScanError::InvalidPackage(crate::i18n::Message::new(
                            "status-xlsx-shared-string-element-mismatch",
                        ))
                    })?;
                    push_shared_string_length(&mut lengths, length, limits)?;
                }
                _ => (),
            },
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
    Ok(SharedStringScanResult { lengths, entries })
}

// Track expanded shared string data against the package limits.
pub(super) fn account_shared_string_bytes(
    current_length: &mut Option<u64>,
    bytes: u64,
    limits: XlsxPackageLimits,
) -> Result<(), XlsxScanError> {
    let length = current_length.as_mut().ok_or_else(|| {
        XlsxScanError::InvalidPackage(crate::i18n::Message::new(
            "status-xlsx-shared-string-data-outside-item",
        ))
    })?;
    *length = length.saturating_add(bytes);
    if *length > limits.max_shared_string_bytes {
        return Err(limit_error(
            XlsxResourceLimit::SharedStringBytes,
            limits.max_shared_string_bytes,
        ));
    }
    Ok(())
}

// Record one shared string length and enforce its size limit.
pub(super) fn push_shared_string_length(
    lengths: &mut Vec<u64>,
    length: u64,
    limits: XlsxPackageLimits,
) -> Result<(), XlsxScanError> {
    if length > limits.max_shared_string_bytes {
        return Err(limit_error(
            XlsxResourceLimit::SharedStringBytes,
            limits.max_shared_string_bytes,
        ));
    }
    if lengths.len() >= limits.max_cells {
        return Err(limit_error(
            XlsxResourceLimit::SharedStringCount,
            limits.max_cells as u64,
        ));
    }
    lengths.push(length);
    Ok(())
}
