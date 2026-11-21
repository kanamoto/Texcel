use super::*;

// Reject XML input that exceeds its part or element limits.
pub(super) fn validate_xml_limits(
    bytes: &[u8],
    part: &str,
    limits: XlsxPackageLimits,
) -> Result<(), XlsxScanError> {
    let mut reader = Reader::from_reader(bytes);
    let mut event_count = 0_usize;
    loop {
        match reader.read_event() {
            Ok(Event::Eof) => return Ok(()),
            Ok(Event::Start(element)) | Ok(Event::Empty(element)) => {
                event_count += 1;
                if element.name().as_ref().len() > limits.max_xml_name_bytes {
                    return Err(limit_error(
                        XlsxResourceLimit::XmlNameBytes,
                        limits.max_xml_name_bytes as u64,
                    ));
                }
                let mut attribute_count = 0;
                let mut element_attribute_bytes = 0_usize;
                for attribute in element.attributes() {
                    let attribute = attribute.map_err(|error| XlsxScanError::InvalidXml {
                        part: part.to_owned(),
                        reason: error.to_string(),
                    })?;
                    attribute_count += 1;
                    if attribute_count > limits.max_xml_attributes_per_element {
                        return Err(limit_error(
                            XlsxResourceLimit::XmlAttributeCount,
                            limits.max_xml_attributes_per_element as u64,
                        ));
                    }
                    if attribute.value.len() > limits.max_xml_attribute_bytes {
                        return Err(limit_error(
                            XlsxResourceLimit::XmlAttributeBytes,
                            limits.max_xml_attribute_bytes as u64,
                        ));
                    }
                    element_attribute_bytes = element_attribute_bytes
                        .saturating_add(attribute.key.as_ref().len())
                        .saturating_add(attribute.value.len());
                    if element_attribute_bytes > limits.max_xml_element_attribute_bytes {
                        return Err(limit_error(
                            XlsxResourceLimit::XmlElementAttributeBytes,
                            limits.max_xml_element_attribute_bytes as u64,
                        ));
                    }
                }
            }
            Ok(Event::Text(text)) => {
                event_count += 1;
                if text.len() > limits.max_xml_text_bytes {
                    return Err(limit_error(
                        XlsxResourceLimit::XmlTextBytes,
                        limits.max_xml_text_bytes as u64,
                    ));
                }
            }
            Ok(Event::CData(text)) => {
                event_count += 1;
                if text.len() > limits.max_xml_text_bytes {
                    return Err(limit_error(
                        XlsxResourceLimit::XmlTextBytes,
                        limits.max_xml_text_bytes as u64,
                    ));
                }
            }
            Ok(Event::GeneralRef(reference)) => {
                event_count += 1;
                if reference.len() > limits.max_xml_text_bytes {
                    return Err(limit_error(
                        XlsxResourceLimit::XmlReferenceBytes,
                        limits.max_xml_text_bytes as u64,
                    ));
                }
            }
            Ok(_) => event_count += 1,
            Err(error) => {
                return Err(XlsxScanError::InvalidXml {
                    part: part.to_owned(),
                    reason: error.to_string(),
                });
            }
        }
        if event_count > limits.max_xml_events {
            return Err(limit_error(
                XlsxResourceLimit::XmlEventCount,
                limits.max_xml_events as u64,
            ));
        }
    }
}

// Check whether a package part name can refer to a worksheet.
pub(super) fn is_worksheet_candidate(name: &str) -> bool {
    name.starts_with("xl/worksheets/") && name.ends_with(".xml")
}

// Read an XML element name without its namespace prefix.
pub(super) fn local_name(name: &[u8]) -> String {
    let name = String::from_utf8_lossy(name);
    name.rsplit(':').next().unwrap_or(&name).to_owned()
}

pub(super) type ParsedXmlElement = (String, BTreeMap<String, String>);

// Visit XML elements while enforcing safe nesting and size limits.
pub(super) fn parse_xml_elements(
    bytes: &[u8],
    part: &str,
    limits: XlsxPackageLimits,
) -> Result<Vec<ParsedXmlElement>, XlsxScanError> {
    let mut reader = Reader::from_reader(bytes);
    let mut elements = Vec::new();
    let mut event_count = 0_usize;
    let mut metadata_bytes = 0_usize;
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) | Ok(Event::Empty(element)) => {
                event_count += 1;
                let name_bytes = element.name().as_ref().len();
                if name_bytes > limits.max_xml_name_bytes {
                    return Err(limit_error(
                        XlsxResourceLimit::XmlNameBytes,
                        limits.max_xml_name_bytes as u64,
                    ));
                }
                if elements.len() >= limits.max_xml_metadata_elements {
                    return Err(limit_error(
                        XlsxResourceLimit::XmlMetadataElementCount,
                        limits.max_xml_metadata_elements as u64,
                    ));
                }
                let name = local_name(element.name().as_ref());
                let mut attribute_count = 0_usize;
                let mut element_attribute_bytes = 0_usize;
                let mut element_metadata_bytes = 64_usize.saturating_add(name_bytes);
                let attributes = element
                    .attributes()
                    .map(|attribute| {
                        let attribute = attribute.map_err(|error| XlsxScanError::InvalidXml {
                            part: part.to_owned(),
                            reason: error.to_string(),
                        })?;
                        if attribute.value.len() > limits.max_xml_attribute_bytes {
                            return Err(limit_error(
                                XlsxResourceLimit::XmlAttributeBytes,
                                limits.max_xml_attribute_bytes as u64,
                            ));
                        }
                        attribute_count += 1;
                        if attribute_count > limits.max_xml_attributes_per_element {
                            return Err(limit_error(
                                XlsxResourceLimit::XmlAttributeCount,
                                limits.max_xml_attributes_per_element as u64,
                            ));
                        }
                        element_attribute_bytes = element_attribute_bytes
                            .saturating_add(attribute.key.as_ref().len())
                            .saturating_add(attribute.value.len());
                        if element_attribute_bytes > limits.max_xml_element_attribute_bytes {
                            return Err(limit_error(
                                XlsxResourceLimit::XmlElementAttributeBytes,
                                limits.max_xml_element_attribute_bytes as u64,
                            ));
                        }
                        // Map nodes and owned strings exceed source XML size; reserve a conservative budget.
                        element_metadata_bytes = element_metadata_bytes
                            .saturating_add(128)
                            .saturating_add(attribute.key.as_ref().len())
                            .saturating_add(attribute.value.len());
                        if metadata_bytes.saturating_add(element_metadata_bytes)
                            > limits.max_xml_metadata_bytes
                        {
                            return Err(limit_error(
                                XlsxResourceLimit::XmlMetadataBytes,
                                limits.max_xml_metadata_bytes as u64,
                            ));
                        }
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
                metadata_bytes = metadata_bytes.saturating_add(element_metadata_bytes);
                elements.push((name, attributes));
            }
            Ok(Event::Text(text)) => {
                event_count += 1;
                if text.len() > limits.max_xml_text_bytes {
                    return Err(limit_error(
                        XlsxResourceLimit::XmlTextBytes,
                        limits.max_xml_text_bytes as u64,
                    ));
                }
            }
            Ok(Event::CData(text)) => {
                event_count += 1;
                if text.len() > limits.max_xml_text_bytes {
                    return Err(limit_error(
                        XlsxResourceLimit::XmlTextBytes,
                        limits.max_xml_text_bytes as u64,
                    ));
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => event_count += 1,
            Err(error) => {
                return Err(XlsxScanError::InvalidXml {
                    part: part.to_owned(),
                    reason: error.to_string(),
                });
            }
        }
        if event_count > limits.max_xml_events {
            return Err(limit_error(
                XlsxResourceLimit::XmlEventCount,
                limits.max_xml_events as u64,
            ));
        }
    }
    Ok(elements)
}
