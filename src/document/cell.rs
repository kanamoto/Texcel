//!  Cell input, formatting, rich text, and typed result display.

use serde::{Deserialize, Serialize};

// Cell input stored separately from formula results and display text.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum CellInput {
    Empty,
    Text(String),
    Number(f64),
    Boolean(bool),
    Formula(String),
    Error(String),
}

impl CellInput {
    // Converts typed input into source text for display in an input field.
    pub fn source_text(&self) -> String {
        match self {
            Self::Empty => String::new(),
            Self::Text(text) => text.clone(),
            Self::Number(number) => number.to_string(),
            Self::Boolean(value) => if *value { "TRUE" } else { "FALSE" }.to_owned(),
            Self::Formula(formula) => formula.clone(),
            Self::Error(error) => error.clone(),
        }
    }

    // Keeps a value typed after editing when possible, and stores unrecognized values as text.
    pub fn edited_source(&self, source: String) -> Self {
        match self {
            Self::Empty if source.is_empty() => Self::Empty,
            Self::Empty => Self::Text(source),
            Self::Text(_) => Self::Text(source),
            Self::Formula(_) if source.is_empty() => Self::Empty,
            Self::Formula(_) if source.starts_with('=') => Self::Formula(source),
            Self::Formula(_) => Self::Text(source),
            Self::Error(_) if source.is_empty() => Self::Empty,
            Self::Error(_) => Self::Text(source),
            Self::Number(_) if source.is_empty() => Self::Empty,
            Self::Number(_) => source
                .parse::<f64>()
                .map(Self::Number)
                .unwrap_or(Self::Text(source)),
            Self::Boolean(_) if source.is_empty() => Self::Empty,
            Self::Boolean(_) if source.eq_ignore_ascii_case("true") => Self::Boolean(true),
            Self::Boolean(_) if source.eq_ignore_ascii_case("false") => Self::Boolean(false),
            Self::Boolean(_) => Self::Text(source),
        }
    }
}

/// Returns a literal text value after removing its persisted formula escape marker.
pub fn literal_text_value(source: &str) -> &str {
    let mut rest = source;
    while let Some(value) = rest.strip_prefix('\'') {
        rest = value;
    }
    if rest.starts_with('=') {
        source.strip_prefix('\'').unwrap_or(source)
    } else {
        source
    }
}

/// Adds one persisted escape marker to raw text that could be read as a formula.
pub fn encode_literal_text(value: &str) -> String {
    let formula_candidate = value.trim_start_matches('\'').starts_with('=');
    if formula_candidate {
        format!("'{value}")
    } else {
        value.to_owned()
    }
}

// A calculated or imported calculation result.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum CalcValue {
    Text(String),
    Number(f64),
    Boolean(bool),
    Error(String),
}

// Shows the state of a calculation result in the current document.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CalcStatus {
    Fresh,
    Pending,
    Imported,
    Unsupported,
    Stale,
    Error,
}

// A result tied to an input version. Do not display stale results.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CalcResult {
    pub revision: u64,
    pub status: CalcStatus,
    pub value: Option<CalcValue>,
}

// A cell with source input, imported formatting, and an optional calculation result.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Cell {
    pub input: CellInput,
    /// Cache read from XLSX, retained independently of runtime calculation results.
    /// Editing replaces the Cell and clears this field; Undo restores the original cache.
    pub imported_result: Option<CalcValue>,
    pub calculation: Option<CalcResult>,
}

/// A text run with the character attributes that Texcel currently preserves.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CellTextRun {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    #[serde(default)]
    pub code: bool,
    #[serde(default)]
    pub hyperlink: Option<CellHyperlink>,
    pub font_name: Option<String>,
    pub font_size: Option<f64>,
    pub font_color: Option<String>,
}

/// Cell hyperlink metadata. `location` distinguishes a sheet-local target from a URL.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CellHyperlink {
    pub target: String,
    pub tooltip: Option<String>,
    pub location: bool,
}

/// Formatting read from an imported XLSX or HTML file, or from pasted HTML.
/// Only import conversion and paste-to-Markdown read it; editing cells never carry it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CellFormat {
    pub style: CellStyle,
    pub hyperlink: Option<CellHyperlink>,
    pub rich_text: Option<Vec<CellTextRun>>,
}

impl CellFormat {
    /// Returns whether this format carries no style, link, or rich text.
    pub fn is_empty(&self) -> bool {
        self.style == CellStyle::default() && self.hyperlink.is_none() && self.rich_text.is_none()
    }
}

/// A single supported border edge.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CellBorder {
    pub style: String,
    pub color: Option<String>,
}

/// Explicit XLSX cell formatting retained in the document and in Undo history.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CellStyle {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub font_name: Option<String>,
    pub font_size: Option<f64>,
    pub font_color: Option<String>,
    pub fill_color: Option<String>,
    pub number_format: Option<String>,
    pub horizontal_alignment: Option<String>,
    pub vertical_alignment: Option<String>,
    pub wrap_text: bool,
    pub top_border: Option<CellBorder>,
    pub bottom_border: Option<CellBorder>,
    pub left_border: Option<CellBorder>,
    pub right_border: Option<CellBorder>,
}

impl Cell {
    // Creates a text cell.
    pub fn text(input: impl Into<String>) -> Self {
        Self {
            input: CellInput::Text(input.into()),
            imported_result: None,
            calculation: None,
        }
    }

    /// Creates an explicitly empty cell, used when formatting a blank range.
    pub fn empty() -> Self {
        Self {
            input: CellInput::Empty,
            imported_result: None,
            calculation: None,
        }
    }

    /// Keeps explicitly literal text from becoming a formula in text-file output.
    pub fn source_text_for_text_file(&self, string_cell: bool) -> String {
        match (&self.input, string_cell) {
            (CellInput::Text(value), true) if literal_text_value(value) == value => {
                encode_literal_text(value)
            }
            _ => self.input.source_text(),
        }
    }

    /// Displays a runtime result selected from the document's immutable calculation root.
    pub fn display_text_with_calculation(&self, result: Option<&CalcResult>) -> String {
        if let Some(result) = result
            && let Some(value) = &result.value
        {
            return match value {
                CalcValue::Text(text) | CalcValue::Error(text) => text.clone(),
                CalcValue::Number(number) => number.to_string(),
                CalcValue::Boolean(value) => if *value { "TRUE" } else { "FALSE" }.to_owned(),
            };
        }
        self.input.source_text()
    }

    /// Displays the result of the given calculation version, or the source text.
    pub fn display_text(&self, calculation_revision: u64) -> String {
        self.display_text_with_calculation(
            self.calculation
                .as_ref()
                .filter(|result| result.revision == calculation_revision),
        )
    }
}

#[cfg(test)]
impl Cell {
    // Creates a formula cell and ties an available result to the initial document version.
    pub fn formula(source: impl Into<String>, result: Option<CalcValue>) -> Self {
        Self {
            input: CellInput::Formula(source.into()),
            imported_result: None,
            calculation: result.map(|value| CalcResult {
                revision: 0,
                status: CalcStatus::Fresh,
                value: Some(value),
            }),
        }
    }
    // Creates a number cell.
    pub fn number(value: f64) -> Self {
        Self {
            input: CellInput::Number(value),
            imported_result: None,
            calculation: None,
        }
    }
}
