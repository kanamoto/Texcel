use super::HtmlError;
use super::tokenizer::{Token, TokenKind};

// Reject declarations that could make browsers decode saved edits as a different encoding.
pub(super) fn validate_charset_declarations(tokens: &[Token]) -> Result<(), HtmlError> {
    let mut template_depth = 0_usize;
    for token in tokens {
        if token.foreign || token.ignored {
            continue;
        }
        match &token.kind {
            TokenKind::Start { name, .. } if name == "template" => {
                template_depth = template_depth.saturating_add(1);
                continue;
            }
            TokenKind::End { name, .. } if name == "template" => {
                template_depth = template_depth.saturating_sub(1);
                continue;
            }
            _ if template_depth > 0 => continue,
            TokenKind::Start { name, attrs, .. } if name == "meta" => {
                if let Some(label) = declared_charset(attrs)
                    && encoding_rs::Encoding::for_label(label.as_bytes())
                        .is_none_or(|encoding| encoding.name() != "UTF-8")
                {
                    return Err(HtmlError::UnsupportedEncoding(label.to_owned()));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

// Reads either a direct charset attribute or the older content-type form.
fn declared_charset(attrs: &std::collections::BTreeMap<String, String>) -> Option<&str> {
    if let Some(label) = attrs.get("charset") {
        return Some(label.trim());
    }
    if !attrs
        .get("http-equiv")
        .is_some_and(|value| value.eq_ignore_ascii_case("content-type"))
    {
        return None;
    }
    let content = attrs.get("content")?;
    let content_lower = content.to_ascii_lowercase();
    let (_, label) = content_lower.split_once("charset=")?;
    let label_start = content.len() - label.len();
    let label = content[label_start..]
        .split(';')
        .next()?
        .trim()
        .trim_matches(['\'', '"']);
    Some(label)
}
