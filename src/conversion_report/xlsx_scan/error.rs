use std::fmt;

// Why XLSX ZIP checks failed. A failed check is not reported as successful conversion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum XlsxScanError {
    InvalidArchive(String),
    InvalidPackage(crate::i18n::Message),
    InvalidXml {
        part: String,
        reason: String,
    },
    LimitExceeded {
        limit: XlsxResourceLimit,
        maximum: u64,
    },
}

/// Names a package resource limit without storing display text in the error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum XlsxResourceLimit {
    CompressedBytes,
    ZipParts,
    PartNameBytes,
    PartExpandedBytes,
    ExpandedBytes,
    ActualPartExpandedBytes,
    RelationshipCount,
    SheetCount,
    Rows,
    Columns,
    Cells,
    SharedStringExpandedBytes,
    SharedStringBytes,
    SharedStringCount,
    ReportEntryCount,
    ReportTextBytes,
    XmlNameBytes,
    XmlAttributeCount,
    XmlAttributeBytes,
    XmlElementAttributeBytes,
    XmlTextBytes,
    XmlReferenceBytes,
    XmlEventCount,
    XmlMetadataElementCount,
    XmlMetadataBytes,
}

impl XlsxResourceLimit {
    /// Returns a localized limit name that stays language neutral until display.
    pub fn message(self) -> crate::i18n::Message {
        crate::i18n::Message::new(match self {
            Self::CompressedBytes => "status-xlsx-limit-compressed-bytes",
            Self::ZipParts => "status-xlsx-limit-zip-parts",
            Self::PartNameBytes => "status-xlsx-limit-part-name-bytes",
            Self::PartExpandedBytes => "status-xlsx-limit-part-expanded-bytes",
            Self::ExpandedBytes => "status-xlsx-limit-expanded-bytes",
            Self::ActualPartExpandedBytes => "status-xlsx-limit-actual-part-expanded-bytes",
            Self::RelationshipCount => "status-xlsx-limit-relationship-count",
            Self::SheetCount => "status-xlsx-limit-sheet-count",
            Self::Rows => "status-xlsx-limit-rows",
            Self::Columns => "status-xlsx-limit-columns",
            Self::Cells => "status-xlsx-limit-cells",
            Self::SharedStringExpandedBytes => "status-xlsx-limit-shared-string-expanded-bytes",
            Self::SharedStringBytes => "status-xlsx-limit-shared-string-bytes",
            Self::SharedStringCount => "status-xlsx-limit-shared-string-count",
            Self::ReportEntryCount => "status-xlsx-limit-report-entry-count",
            Self::ReportTextBytes => "status-xlsx-limit-report-text-bytes",
            Self::XmlNameBytes => "status-xlsx-limit-xml-name-bytes",
            Self::XmlAttributeCount => "status-xlsx-limit-xml-attribute-count",
            Self::XmlAttributeBytes => "status-xlsx-limit-xml-attribute-bytes",
            Self::XmlElementAttributeBytes => "status-xlsx-limit-xml-element-attribute-bytes",
            Self::XmlTextBytes => "status-xlsx-limit-xml-text-bytes",
            Self::XmlReferenceBytes => "status-xlsx-limit-xml-reference-bytes",
            Self::XmlEventCount => "status-xlsx-limit-xml-event-count",
            Self::XmlMetadataElementCount => "status-xlsx-limit-xml-metadata-element-count",
            Self::XmlMetadataBytes => "status-xlsx-limit-xml-metadata-bytes",
        })
    }
}

impl std::fmt::Display for XlsxScanError {
    // Show a short English message to callers without a language setting.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidArchive(reason) => formatter.write_str(reason),
            Self::InvalidPackage(message) => {
                formatter.write_str(&message.resolve(crate::i18n::Language::English))
            }
            Self::InvalidXml { part, reason } => {
                write!(formatter, "{part} XML could not be read: {reason}")
            }
            Self::LimitExceeded { limit, maximum } => write!(
                formatter,
                "{}: {maximum}",
                limit.message().resolve(crate::i18n::Language::English)
            ),
        }
    }
}

impl std::error::Error for XlsxScanError {}
