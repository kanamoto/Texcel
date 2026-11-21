//!  Worker-side operations for search helpers.

use super::*;
use crate::i18n::Message;

// Build the search expression from literal or regular expression options.
pub(crate) fn search_pattern(request: &SearchRequest) -> Result<regex::Regex, Message> {
    let pattern = if request.regular_expression {
        request.query.clone()
    } else {
        regex::escape(&request.query)
    };
    let pattern = if request.whole_cell {
        format!("^(?:{pattern})$")
    } else {
        pattern
    };
    regex::RegexBuilder::new(&pattern)
        .case_insensitive(!request.case_sensitive)
        .size_limit(10 * 1024 * 1024)
        .dfa_size_limit(10 * 1024 * 1024)
        .nest_limit(128)
        .build()
        .map_err(|error| Message::new("status-regex-invalid").with_text("detail", error))
}

// Record a skipped search candidate and enforce the hit limit.
pub(crate) fn continue_search_skip(
    hits: &mut Vec<SearchHit>,
    table_id: TableId,
    address: CellAddress,
    source_text: String,
    state: SearchResultState,
    hits_truncated: &mut bool,
) {
    if hits.len() < 10_000 {
        hits.push(SearchHit {
            table_id,
            address,
            source_text,
            searched_text: String::new(),
            replacement_text: None,
            result_state: Some(state),
        });
    } else {
        *hits_truncated = true;
    }
}
