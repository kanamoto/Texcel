use super::*;

/// Builds a document border from XLSX border data.
pub(super) fn border_from(
    border: &umya_spreadsheet::Border,
    theme: &umya_spreadsheet::structs::drawing::Theme,
) -> Option<XlsxBorder> {
    if border.border_style().is_empty() || border.border_style() == "none" {
        return None;
    }
    Some(XlsxBorder {
        style: border.border_style().to_owned(),
        color: border
            .color()
            .map(|color| color.argb_with_theme(theme).into_owned())
            .filter(|color| !color.is_empty() && color != "00000000"),
    })
}

/// Converts a document border to XLSX data.
pub(super) fn document_border(border: &Option<XlsxBorder>) -> Option<CellBorder> {
    border.as_ref().map(|border| CellBorder {
        style: border.style.clone(),
        color: border.color.clone(),
    })
}

/// Builds an XLSX border side from document style.
pub(super) fn xlsx_border(border: &Option<CellBorder>) -> Option<XlsxBorder> {
    border.as_ref().map(|border| XlsxBorder {
        style: border.style.clone(),
        color: border.color.clone(),
    })
}

/// Converts an XLSX style to supported document style fields.
pub(super) fn document_style(style: &XlsxStyle) -> CellStyle {
    CellStyle {
        bold: style.bold,
        italic: style.italic,
        strike: style.strike,
        font_name: style.font_name.clone(),
        font_size: style.font_size,
        font_color: style.font_color.clone(),
        fill_color: style.fill_color.clone(),
        number_format: style.number_format.clone(),
        horizontal_alignment: style.horizontal_alignment.clone(),
        vertical_alignment: style.vertical_alignment.clone(),
        wrap_text: style.wrap_text,
        top_border: document_border(&style.top_border),
        bottom_border: document_border(&style.bottom_border),
        left_border: document_border(&style.left_border),
        right_border: document_border(&style.right_border),
    }
}

/// Builds an XLSX style from supported document fields.
pub(crate) fn xlsx_style(style: &CellStyle) -> XlsxStyle {
    XlsxStyle {
        bold: style.bold,
        italic: style.italic,
        strike: style.strike,
        font_name: style.font_name.clone(),
        font_size: style.font_size,
        font_color: style.font_color.clone(),
        fill_color: style.fill_color.clone(),
        number_format: style.number_format.clone(),
        wrap_text: style.wrap_text,
        horizontal_alignment: style.horizontal_alignment.clone(),
        vertical_alignment: style.vertical_alignment.clone(),
        top_border: xlsx_border(&style.top_border),
        bottom_border: xlsx_border(&style.bottom_border),
        left_border: xlsx_border(&style.left_border),
        right_border: xlsx_border(&style.right_border),
    }
}

/// umya's unset font color reports transparent black; keep it distinct from an explicit color.
pub(super) fn font_color(
    font: &Font,
    theme: &umya_spreadsheet::structs::drawing::Theme,
) -> Option<String> {
    if font.color() == &umya_spreadsheet::Color::default() {
        return None;
    }
    let color = font.color().argb_with_theme(theme).into_owned();
    (!color.is_empty() && color != "00000000").then_some(color)
}
