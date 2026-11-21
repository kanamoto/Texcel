use super::*;

use std::borrow::Cow;

/// Inline text and basic emphasis resolved by pulldown while the full document is parsed.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MarkdownCellDisplay {
    pub differs_from_source: bool,
    pub literal_marker_consumed: bool,
    pub literal_marker_range: Option<Range<usize>>,
    pub plain_text: String,
    pub rendered_bytes: usize,
    pub rendered_lines: usize,
    pub first_link_target: Option<String>,
    pub first_link_tooltip: Option<String>,
    pub runs: Vec<MarkdownDisplayRun>,
}

impl MarkdownCellDisplay {
    /// Parses one edited cell fragment when it differs from the worker-parsed source snapshot.
    pub fn parse_fragment(source: &str) -> Self {
        Self::parse_fragment_with_limit(source, Some(4 * 1024))
    }

    /// Adds a literal marker to formatted text when the visible value needs one.
    pub(crate) fn persist_literal_marker(
        source: &str,
        visible_text: &str,
    ) -> Option<(String, Option<usize>)> {
        let marker_needed = crate::document::encode_literal_text(visible_text) != visible_text;
        if !marker_needed {
            return Some((source.to_owned(), None));
        }
        let display = Self::parse_fragment(source);
        if display.literal_marker_consumed && display.plain_text == visible_text {
            return Some((source.to_owned(), None));
        }
        if display.runs.first().is_some_and(|run| run.code) {
            return (display.plain_text == visible_text).then(|| (source.to_owned(), None));
        }

        let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_GFM;
        let mut insertion = None;
        for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
            match event {
                Event::Text(text) if !text.is_empty() => {
                    insertion = Some(range.start);
                    break;
                }
                Event::Code(text) if !text.is_empty() => {
                    return (display.plain_text == visible_text).then(|| (source.to_owned(), None));
                }
                _ => (),
            }
        }
        let insertion = insertion?;

        let mut encoded = String::with_capacity(source.len().saturating_add(1));
        encoded.push_str(source.get(..insertion)?);
        encoded.push('\'');
        encoded.push_str(source.get(insertion..)?);
        let reparsed = Self::parse_fragment(&encoded);
        (reparsed.literal_marker_consumed && reparsed.plain_text == visible_text)
            .then_some((encoded, Some(insertion)))
    }

    // Parse inline Markdown for display and stop at the requested byte limit.
    fn parse_fragment_with_limit(source: &str, max_bytes: Option<usize>) -> Self {
        let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_GFM;
        let mut display = InlineDisplayBuilder::default();
        let mut style = InlineStyleState::default();
        for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
            match event {
                Event::Start(Tag::Strong) => style.bold += 1,
                Event::Start(Tag::Emphasis) => style.italic += 1,
                Event::Start(Tag::Strikethrough) => style.strike += 1,
                Event::Start(Tag::Link {
                    dest_url, title, ..
                }) => {
                    let link = crate::document::CellHyperlink {
                        target: dest_url.to_string(),
                        tooltip: (!title.is_empty()).then(|| title.to_string()),
                        location: false,
                    };
                    if display.first_link_target.is_none() {
                        display.first_link_target = Some(link.target.clone());
                        display.first_link_tooltip = link.tooltip.clone();
                    }
                    style.links.push(link);
                }
                Event::End(TagEnd::Strong) => style.bold = style.bold.saturating_sub(1),
                Event::End(TagEnd::Emphasis) => style.italic = style.italic.saturating_sub(1),
                Event::End(TagEnd::Strikethrough) => {
                    style.strike = style.strike.saturating_sub(1);
                }
                Event::End(TagEnd::Link) => {
                    style.links.pop();
                }
                Event::Text(text) => {
                    display.push(&text, style.run(false), Some(range), Some(source), false);
                }
                Event::Code(text) => {
                    display.push(&text, style.run(true), Some(range), Some(source), false);
                }
                Event::SoftBreak => display.push(" ", style.run(false), None, None, false),
                Event::HardBreak => display.push("\n", style.run(false), None, None, false),
                Event::InlineHtml(html) | Event::Html(html) if is_markdown_break(&html) => {
                    display.push("\n", InlineDisplayStyle::default(), None, None, false);
                }
                Event::End(TagEnd::Image) => {
                    display.push_image();
                }
                Event::TaskListMarker(checked) => display.push(
                    if checked { "☑ " } else { "☐ " },
                    InlineDisplayStyle::default(),
                    None,
                    None,
                    false,
                ),
                _ => (),
            }
        }
        let mut display = display.finish(source);
        if let Some(max_bytes) = max_bytes {
            display.limit_and_own_runs(None, max_bytes);
        }
        display
    }

    /// Stores only the grid preview text, resolving worker source ranges before releasing source.
    pub(crate) fn limit_and_own_runs(
        &mut self,
        source: Option<&MarkdownDocument>,
        max_bytes: usize,
    ) {
        let mut remaining = max_bytes;
        let mut keep = 0;
        for run in &mut self.runs {
            if remaining == 0 {
                break;
            }
            let Some(text) = run.text.clone().or_else(|| {
                source
                    .and_then(|source| source.display_run_text(run))
                    .map(str::to_owned)
            }) else {
                if run.localized_image {
                    if remaining < 9 {
                        break;
                    }
                    remaining = remaining.saturating_sub(9);
                    keep += 1;
                }
                continue;
            };
            let mut end = remaining.min(text.len());
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            run.text = Some(text[..end].to_owned());
            run.source_range = None;
            remaining = remaining.saturating_sub(end);
            keep += 1;
        }
        self.runs.truncate(keep);
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MarkdownDisplayRun {
    pub source_range: Option<Range<usize>>,
    pub text: Option<String>,
    pub localized_image: bool,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: bool,
    pub link_target: Option<String>,
    pub link_tooltip: Option<String>,
}

impl MarkdownDisplayRun {
    /// Resolves an image marker in the app language and keeps normal text borrowed.
    pub fn text_for_language<'a>(
        &'a self,
        source: Option<&'a MarkdownDocument>,
        language: crate::i18n::Language,
    ) -> Option<Cow<'a, str>> {
        if self.localized_image {
            return Some(Cow::Owned(
                crate::i18n::Message::new("markdown-image-placeholder").resolve(language),
            ));
        }
        self.text
            .as_deref()
            .or_else(|| source.and_then(|source| source.display_run_text(self)))
            .map(Cow::Borrowed)
    }
}

#[derive(Default)]
pub(super) struct InlineStyleState {
    pub(super) bold: usize,
    pub(super) italic: usize,
    pub(super) strike: usize,
    pub(super) links: Vec<crate::document::CellHyperlink>,
}

impl InlineStyleState {
    // Return the inline style that applies to the next displayed text.
    pub(super) fn run(&self, code: bool) -> InlineDisplayStyle {
        InlineDisplayStyle {
            bold: self.bold > 0,
            italic: self.italic > 0,
            strike: self.strike > 0,
            code,
            link: !self.links.is_empty(),
            link_target: self.links.last().map(|link| link.target.clone()),
            link_tooltip: self.links.last().and_then(|link| link.tooltip.clone()),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct InlineDisplayStyle {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    link: bool,
    link_target: Option<String>,
    link_tooltip: Option<String>,
}

#[derive(Default)]
pub(super) struct InlineDisplayBuilder {
    pub(super) source_range: Range<usize>,
    pub(super) plain: String,
    // Whether a real leading apostrophe may start the marker.
    pub(super) literal_marker_candidate: bool,
    // Whether visible text confirmed the marker before any code run.
    pub(super) literal_marker_complete: bool,
    // Counts apostrophes at the start without rescanning earlier text runs.
    pub(super) literal_marker_apostrophes: usize,
    // Stores the source byte for the marker found in the first text event.
    pub(super) literal_marker_source_range: Option<Range<usize>>,
    pub(super) first_link_target: Option<String>,
    pub(super) first_link_tooltip: Option<String>,
    pub(super) runs: Vec<MarkdownDisplayRun>,
}

impl InlineDisplayBuilder {
    // Adds styled text and borrows source offsets only for a shared document.
    pub(super) fn push(
        &mut self,
        text: &str,
        style: InlineDisplayStyle,
        source_range: Option<Range<usize>>,
        source: Option<&str>,
        borrow_source: bool,
    ) {
        if self.plain.is_empty()
            && !style.code
            && text.starts_with('\'')
            && source_range
                .as_ref()
                .is_some_and(|range| is_unescaped_apostrophe(range, source))
        {
            self.literal_marker_candidate = true;
            self.literal_marker_source_range = source_range
                .as_ref()
                .map(|range| range.start..range.start.saturating_add(1));
        }
        if self.literal_marker_candidate && !self.literal_marker_complete {
            if style.code {
                self.literal_marker_candidate = false;
            } else {
                let apostrophes = text.bytes().take_while(|byte| *byte == b'\'').count();
                self.literal_marker_apostrophes =
                    self.literal_marker_apostrophes.saturating_add(apostrophes);
                match text.as_bytes().get(apostrophes) {
                    None => (),
                    Some(&b'=')
                        if self.literal_marker_apostrophes > 0
                            && source_range.as_ref().is_some_and(|range| {
                                equals_matches_text(text, apostrophes, range, source)
                            }) =>
                    {
                        self.literal_marker_complete = true;
                    }
                    Some(_) => self.literal_marker_candidate = false,
                }
            }
        }
        self.plain.push_str(text);
        let source_range = source_range
            .filter(|_| borrow_source)
            .filter(|range| source.and_then(|source| source.get(range.clone())) == Some(text));
        if let Some(last) = self.runs.last_mut()
            && last.bold == style.bold
            && last.italic == style.italic
            && last.strike == style.strike
            && last.code == style.code
            && last.link == style.link
            && last.link_target == style.link_target
            && last.link_tooltip == style.link_tooltip
            && last.source_range.is_some() == source_range.is_some()
            && !last.localized_image
        {
            if let (Some(last_range), Some(next_range)) =
                (&mut last.source_range, source_range.as_ref())
            {
                if last_range.end == next_range.start {
                    last_range.end = next_range.end;
                    return;
                }
            } else if let Some(last_text) = &mut last.text {
                last_text.push_str(text);
                return;
            }
        }
        let owned_text = source_range.is_none().then(|| text.to_owned());
        self.runs.push(MarkdownDisplayRun {
            source_range,
            text: owned_text,
            localized_image: false,
            bold: style.bold,
            italic: style.italic,
            strike: style.strike,
            code: style.code,
            link: style.link,
            link_target: style.link_target,
            link_tooltip: style.link_tooltip,
        });
    }

    /// Adds a typed image marker that can change with the selected language.
    pub(super) fn push_image(&mut self) {
        self.plain.push_str(" [Image]");
        self.runs.push(MarkdownDisplayRun {
            localized_image: true,
            ..MarkdownDisplayRun::default()
        });
    }

    // Build the final cell display and attach links from the full source.
    pub(super) fn finish(mut self, source: &str) -> MarkdownCellDisplay {
        let literal_marker_consumed = self.literal_marker_candidate && self.literal_marker_complete;
        let literal_marker_range = literal_marker_consumed
            .then(|| self.literal_marker_source_range.clone())
            .flatten();
        if literal_marker_consumed {
            self.plain.remove(0);
            if let Some(run) = self
                .runs
                .iter_mut()
                .find(|run| !run.localized_image && !run.code)
            {
                if let Some(range) = &mut run.source_range {
                    range.start = range.start.saturating_add(1);
                } else if let Some(text) = &mut run.text
                    && text.starts_with('\'')
                {
                    text.remove(0);
                }
            }
        }
        let rendered_bytes = self.plain.len();
        let rendered_lines = self.plain.split('\n').count().max(1);
        MarkdownCellDisplay {
            differs_from_source: self.plain != source,
            literal_marker_consumed,
            literal_marker_range,
            plain_text: self.plain,
            rendered_bytes,
            rendered_lines,
            first_link_target: self.first_link_target,
            first_link_tooltip: self.first_link_tooltip,
            runs: self.runs,
        }
    }
}

// Checks whether an event starts at a real apostrophe rather than an escaped one.
fn is_unescaped_apostrophe(range: &Range<usize>, source: Option<&str>) -> bool {
    let Some(source) = source else {
        return false;
    };
    let Some(source_prefix) = source.get(..range.start) else {
        return false;
    };
    let Some(event_source) = source.get(range.clone()) else {
        return false;
    };
    let backslashes = source_prefix
        .bytes()
        .rev()
        .take_while(|byte| *byte == b'\\')
        .count();
    event_source.starts_with('\'') && backslashes % 2 == 0
}

// Matches the first displayed equals sign to the current Markdown text event.
fn equals_matches_text(
    text: &str,
    apostrophes: usize,
    range: &Range<usize>,
    source: Option<&str>,
) -> bool {
    let Some(source) = source else {
        return false;
    };
    let Some(event_source) = source.get(range.clone()) else {
        return false;
    };
    if !source_has_unescaped_start(range.start, source) {
        return false;
    }
    if event_source == text {
        return true;
    }
    if apostrophes != 0 || !text.starts_with('=') || !event_source.starts_with('=') {
        return false;
    }
    true
}

// Checks whether a source character is preceded by an even number of backslashes.
fn source_has_unescaped_start(position: usize, source: &str) -> bool {
    let backslashes = source
        .as_bytes()
        .get(..position)
        .unwrap_or_default()
        .iter()
        .rev()
        .take_while(|byte| **byte == b'\\')
        .count();
    backslashes % 2 == 0
}

// Check whether an HTML fragment represents a Markdown line break.
pub(super) fn is_markdown_break(html: &str) -> bool {
    let Some(inner) = html
        .trim()
        .strip_prefix('<')
        .and_then(|value| value.strip_suffix('>'))
    else {
        return false;
    };
    inner
        .trim()
        .trim_end_matches('/')
        .trim()
        .eq_ignore_ascii_case("br")
}
