use super::*;

/// Parses one HTML table into clipboard cells.
pub(super) fn parse_html_table(html: &str) -> Option<Vec<Vec<ClipboardCell>>> {
    let lower = html.to_ascii_lowercase();
    let table_start = lower.find("<table")?;
    let table_end = lower[table_start..].find("</table>")? + table_start;
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cursor = table_start;
    let mut cell_count = 0_usize;
    let mut open_cell: Option<(&str, usize, CellStyle)> = None;
    while cursor < table_end {
        let Some(relative_tag_start) = lower[cursor..table_end].find('<') else {
            break;
        };
        let tag_start = cursor + relative_tag_start;
        let Some(relative_tag_end) = lower[tag_start..table_end].find('>') else {
            break;
        };
        let tag_end = tag_start + relative_tag_end;
        let tag = &html[tag_start + 1..tag_end];
        let tag_lower = &lower[tag_start + 1..tag_end];
        if tag_lower.starts_with("!--") {
            let comment_end = lower[tag_start..table_end].find("-->")? + tag_start + 3;
            cursor = comment_end;
            continue;
        }
        let closing = tag_lower.starts_with('/');
        let name_start = usize::from(closing);
        let name_end = tag_lower[name_start..]
            .find(|character: char| !character.is_ascii_alphanumeric())
            .map_or(tag_lower.len(), |offset| name_start + offset);
        let name = &tag_lower[name_start..name_end];
        if let Some((cell_name, content_start, style)) = open_cell.as_ref()
            && closing
            && name == *cell_name
        {
            let cell_html = &html[*content_start..tag_start];
            let text = html_text(cell_html);
            let mut cell = text_cell(text);
            cell.style = style.clone();
            cell.rich_text = html_runs(cell_html);
            cell.hyperlink = html_outer_link(cell_html);
            cell_count = cell_count.saturating_add(1);
            row.push(cell);
            if cell_count > MAX_CLIPBOARD_CELLS || row.len() > MAX_CLIPBOARD_CELLS {
                return None;
            }
            open_cell = None;
        } else if open_cell.is_none() && !closing && name == "tr" {
            if !row.is_empty() {
                rows.push(std::mem::take(&mut row));
            }
        } else if open_cell.is_none() && !closing && matches!(name, "td" | "th") {
            open_cell = Some((name, tag_end + 1, parse_html_cell_style(tag)));
        }
        cursor = tag_end + 1;
    }
    if open_cell.is_some() {
        return None;
    }
    if !row.is_empty() {
        rows.push(row);
    }
    (rows.len() <= MAX_CLIPBOARD_CELLS
        && rows.iter().any(|row| !row.is_empty())
        && validate_matrix(&rows).is_ok())
    .then_some(rows)
}

/// Reads the supported inline style for one HTML cell.
pub(super) fn html_cell_style(cell: &ClipboardCell) -> String {
    let mut declarations = Vec::new();
    if cell.style.bold {
        declarations.push("font-weight:bold".to_owned());
    }
    if cell.style.italic {
        declarations.push("font-style:italic".to_owned());
    }
    if let Some(color) = &cell.style.font_color
        && let Some(color) = css_color(color)
    {
        declarations.push(format!("color:{color}"));
    }
    if let Some(color) = &cell.style.fill_color
        && let Some(color) = css_color(color)
    {
        declarations.push(format!("background-color:{color}"));
    }
    if let Some(size) = cell
        .style
        .font_size
        .filter(|size| size.is_finite() && *size > 0.0)
    {
        declarations.push(format!("font-size:{size}pt"));
    }
    declarations.join(";")
}

/// Parses a supported CSS color value.
pub(super) fn css_color(value: &str) -> Option<String> {
    let value = value.strip_prefix('#').unwrap_or(value);
    let value = if value.len() == 8 {
        value.get(2..)?
    } else {
        value
    };
    (value.len() == 6 && value.chars().all(|character| character.is_ascii_hexdigit()))
        .then(|| format!("#{value}"))
}

/// Reads supported style declarations from an HTML cell.
fn parse_html_cell_style(tag: &str) -> CellStyle {
    let mut style = CellStyle::default();
    let Some(css) = html_attribute(tag, "style") else {
        return style;
    };
    for declaration in css.split(';') {
        let Some((key, value)) = declaration.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        match key.as_str() {
            "font-weight"
                if value.eq_ignore_ascii_case("bold")
                    || value.parse::<u16>().is_ok_and(|weight| weight >= 600) =>
            {
                style.bold = true
            }
            "font-style" if value.eq_ignore_ascii_case("italic") => style.italic = true,
            "color" => style.font_color = parse_html_color(value),
            "background-color" | "background" => style.fill_color = parse_html_color(value),
            "font-size" => {
                style.font_size = value
                    .strip_suffix("pt")
                    .and_then(|size| size.trim().parse().ok())
            }
            "text-align" if matches!(value, "left" | "center" | "right") => {
                style.horizontal_alignment = Some(value.to_owned())
            }
            _ => (),
        }
    }
    style
}

/// Parses an HTML color value into the app color format.
fn parse_html_color(value: &str) -> Option<String> {
    let value = value.trim().strip_prefix('#').unwrap_or(value.trim());
    (value.len() == 6 && value.chars().all(|character| character.is_ascii_hexdigit()))
        .then(|| format!("FF{}", value.to_ascii_uppercase()))
}

/// Extracts plain and formatted text runs from an HTML cell.
fn html_runs(html: &str) -> Option<Vec<CellTextRun>> {
    let mut runs = Vec::new();
    let mut style = CellTextRun::default();
    let mut stack = Vec::<(String, CellTextRun)>::new();
    let mut rest = html;
    while let Some(tag_start) = rest.find('<') {
        let text = decode_html_entities(&rest[..tag_start]);
        push_run(&mut runs, &style, text);
        let tag_end = rest[tag_start..].find('>')? + tag_start;
        let tag = rest[tag_start + 1..tag_end].trim();
        let closing = tag.starts_with('/');
        let name = tag
            .trim_start_matches('/')
            .split_whitespace()
            .next()?
            .trim_end_matches('/')
            .to_ascii_lowercase();
        if matches!(
            name.as_str(),
            "b" | "strong" | "i" | "em" | "s" | "strike" | "del" | "span"
        ) {
            if closing {
                if let Some(index) = stack.iter().rposition(|(tag, _)| tag == &name) {
                    style = stack.remove(index).1;
                }
            } else {
                stack.push((name.clone(), style.clone()));
                match name.as_str() {
                    "b" | "strong" => style.bold = true,
                    "i" | "em" => style.italic = true,
                    "s" | "strike" | "del" => style.strike = true,
                    "span" => apply_run_css(&mut style, tag),
                    _ => (),
                }
            }
        } else if matches!(name.as_str(), "br" | "br/") {
            push_run(&mut runs, &style, "\n".to_owned());
        }
        rest = &rest[tag_end + 1..];
    }
    push_run(&mut runs, &style, decode_html_entities(rest));
    runs.iter()
        .any(|run| {
            run.bold
                || run.italic
                || run.strike
                || run.font_color.is_some()
                || run.font_size.is_some()
        })
        .then_some(runs)
}

// Reads a link only when it covers the full clipboard cell.
fn html_outer_link(html: &str) -> Option<crate::document::CellHyperlink> {
    let trimmed = html.trim();
    let lower = trimmed.to_ascii_lowercase();
    let opening_end = lower.find('>')?;
    let opening = &trimmed[..opening_end];
    if !opening
        .trim_start_matches('<')
        .get(..1)?
        .eq_ignore_ascii_case("a")
        || !opening
            .trim_start_matches('<')
            .get(1..)?
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
        || !lower.ends_with("</a>")
        || lower[opening_end + 1..lower.len() - 4].contains("<a")
    {
        return None;
    }
    let target = decode_html_entities(html_attribute(opening, "href")?);
    Some(crate::document::CellHyperlink {
        location: target.starts_with('#'),
        target,
        tooltip: html_attribute(opening, "title").map(decode_html_entities),
    })
}

/// Adds a text run when it is different from the preceding style.
fn push_run(runs: &mut Vec<CellTextRun>, style: &CellTextRun, text: String) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = runs.last_mut()
        && last.bold == style.bold
        && last.italic == style.italic
        && last.strike == style.strike
        && last.font_size == style.font_size
        && last.font_color == style.font_color
    {
        last.text.push_str(&text);
    } else {
        runs.push(CellTextRun {
            text,
            bold: style.bold,
            italic: style.italic,
            strike: style.strike,
            font_size: style.font_size,
            font_color: style.font_color.clone(),
            font_name: style.font_name.clone(),
            ..CellTextRun::default()
        });
    }
}

/// Applies supported CSS properties to one text run.
fn apply_run_css(style: &mut CellTextRun, tag: &str) {
    let Some(css) = html_attribute(tag, "style") else {
        return;
    };
    for declaration in css.split(';') {
        let Some((key, value)) = declaration.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        match key.as_str() {
            "font-weight"
                if value.eq_ignore_ascii_case("bold")
                    || value.parse::<u16>().is_ok_and(|weight| weight >= 600) =>
            {
                style.bold = true
            }
            "font-style" if value.eq_ignore_ascii_case("italic") => style.italic = true,
            "text-decoration" if value.contains("line-through") => style.strike = true,
            "color" => style.font_color = parse_html_color(value),
            "font-size" => {
                style.font_size = value
                    .strip_suffix("pt")
                    .and_then(|size| size.trim().parse().ok())
            }
            _ => (),
        }
    }
}

/// Collects visible text while ignoring markup tags.
fn html_text(html: &str) -> String {
    let mut output = String::new();
    let mut rest = html;
    while let Some(tag_start) = rest.find('<') {
        output.push_str(&decode_html_entities(&rest[..tag_start]));
        let Some(tag_end) = rest[tag_start..].find('>') else {
            return output;
        };
        let tag = rest[tag_start + 1..tag_start + tag_end]
            .trim()
            .to_ascii_lowercase();
        if tag.starts_with("br") || tag.starts_with("/div") || tag.starts_with("/p") {
            output.push('\n');
        }
        rest = &rest[tag_start + tag_end + 1..];
    }
    output.push_str(&decode_html_entities(rest));
    output.trim_matches('\n').to_owned()
}

/// Reads an HTML attribute value.
fn html_attribute<'a>(tag: &'a str, attribute: &str) -> Option<&'a str> {
    let rest = tag.split_once(char::is_whitespace)?.1;
    let lower = rest.to_ascii_lowercase();
    let key = format!("{attribute}=");
    let start = lower.find(&key)? + key.len();
    let quote = *rest.as_bytes().get(start)? as char;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let value_start = start + 1;
    let value_end = rest[value_start..].find(quote)? + value_start;
    Some(&rest[value_start..value_end])
}

/// Decodes supported HTML entities in clipboard text.
fn decode_html_entities(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        output.push_str(&rest[..start]);
        let Some(end) = rest[start..].find(';') else {
            output.push_str(&rest[start..]);
            return output;
        };
        let entity = &rest[start + 1..start + end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            value if value.starts_with("#x") || value.starts_with("#X") => {
                u32::from_str_radix(&value[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
            }
            value if value.starts_with('#') => {
                value[1..].parse::<u32>().ok().and_then(char::from_u32)
            }
            _ => None,
        };
        if let Some(decoded) = decoded {
            output.push(decoded);
        } else {
            output.push_str(&rest[start..start + end + 1]);
        }
        rest = &rest[start + end + 1..];
    }
    output.push_str(rest);
    output
}

/// Escapes text before writing it into HTML markup.
pub(super) fn escape_html_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Escapes text before writing it into an HTML attribute.
pub(super) fn escape_html_attribute(value: &str) -> String {
    escape_html_text(value).replace('\'', "&#39;")
}
