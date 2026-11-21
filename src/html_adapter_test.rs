use super::*;

// Checks that only real document tables are imported from the HTML token stream.
#[test]
fn ignores_comments_scripts_styles_and_templates() {
    let source = concat!(
        "<!-- <table><tr><td>comment</td></tr></table> -->",
        "<script>const x = '<table>';</script>",
        "<style>.x::after { content: '<table>'; }</style>",
        "<template><table><tr><td>template</td></tr></table></template>",
        "<table><tr><th>Name</th><td>A &amp; B</td></tr></table>"
    );
    let document = HtmlDocument::parse(source.as_bytes().to_vec()).unwrap();

    assert_eq!(document.tables.len(), 1);
    assert_eq!(document.tables[0].rows.len(), 1);
    assert_eq!(document.tables[0].rows[0][1].value, "A &amp; B");
    assert_eq!(document.tables[0].rows[0][1].display_value, "A & B");
}

// Verify generated HTML table names use the selected language without changing captions.
#[test]
fn generated_html_table_names_are_localized_and_captions_stay_authored() {
    let original = concat!(
        "<table><tr><td>plain</td></tr></table>",
        "<table><caption>売上表</caption><tr><td>named</td></tr></table>"
    );
    let source = HtmlDocument::parse(original.as_bytes().to_vec()).unwrap();
    let english =
        source.to_document_with_language(1, "english.html", crate::i18n::Language::English);
    let japanese =
        source.to_document_with_language(2, "japanese.html", crate::i18n::Language::Japanese);

    assert_eq!(english.tables[0].name, "Table1");
    assert_eq!(japanese.tables[0].name, "表1");
    assert_eq!(english.tables[1].name, "売上表");
    assert_eq!(japanese.tables[1].name, "売上表");
}

// Checks that an omitted cell and row close cannot consume the next row group.
#[test]
fn closes_cells_before_tbody_boundaries() {
    let source = "<table><tbody id='a'><tr><td>A</tbody><tbody id='b'><tr><td>B</table>";
    let document = HtmlDocument::parse(source.as_bytes().to_vec()).unwrap();

    assert_eq!(document.tables[0].rows[0][0].value, "A");
    assert_eq!(document.tables[0].rows[1][0].value, "B");
}

// Checks that empty physical rows retain later rows' original table coordinates.
#[test]
fn retains_empty_rows_in_logical_coordinates() {
    let source = "<table><tr><td>A</td></tr><tr></tr><tr><td>B</td></tr></table>";
    let document = HtmlDocument::parse(source.as_bytes().to_vec()).unwrap();

    assert_eq!(document.tables[0].rows.len(), 3);
    assert!(document.tables[0].rows[1].is_empty());
    assert_eq!(document.tables[0].rows[2][0].value, "B");
}

// Checks rowspan and colspan expansion and installs the same merge in the grid model.
#[test]
fn expands_spans_into_grid_merges() {
    let source = "<table><tbody><tr><td rowspan='2' colspan='2'>A</td><td>B</td></tr><tr><td>C</td></tr></tbody></table>";
    let document = HtmlDocument::parse(source.as_bytes().to_vec()).unwrap();
    let table = &document.tables[0];

    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0].len(), 3);
    assert_eq!(table.rows[0][0].value, "A");
    assert!(table.rows[0][1].placeholder);
    assert_eq!(table.rows[0][2].value, "B");
    assert!(table.rows[1][0].placeholder);
    assert!(table.rows[1][1].placeholder);
    assert_eq!(table.rows[1][2].value, "C");
    assert_eq!(table.merges.len(), 1);

    let model = document.to_document(41, "HTML");
    let merge = table.merges[0];
    assert_eq!(
        model.tables[0]
            .layout
            .value(crate::document::LayoutTarget::Merge(merge)),
        crate::document::LayoutValue::Merge(true)
    );
}

// Checks that growing a row keeps a rowspan placeholder reserved in a non-leading column.
#[test]
fn preserves_nonleading_rowspan_coordinates() {
    let source =
        "<table><tr><td>A</td><td rowspan='2'>B</td></tr><tr><td>C</td><td>D</td></tr></table>";
    let document = HtmlDocument::parse(source.as_bytes().to_vec()).unwrap();
    let rows = &document.tables[0].rows;

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0][0].value, "A");
    assert_eq!(rows[0][1].value, "B");
    assert_eq!(rows[1][0].value, "C");
    assert!(rows[1][1].placeholder);
    assert_eq!(rows[1][2].value, "D");
}

// Checks that hidden text and foreign CDATA cannot create false tables or hide later HTML.
#[test]
fn tokenizer_context_keeps_noframes_and_svg_opaque() {
    let source = concat!(
        "<noframes><table><tr><td>frame fake</td></tr></table></noframes>",
        "<svg><![CDATA[</svg><table><tr><td>cdata fake</td></tr></table>]]>",
        "<script/><path/></svg>",
        "<svg><script/></svg>",
        "<table><tr><td>real</td></tr></table>"
    );
    let document = HtmlDocument::parse(source.as_bytes().to_vec()).unwrap();

    assert_eq!(document.tables.len(), 1);
    assert_eq!(document.tables[0].rows[0][0].value, "real");
}

// Checks that foreignObject uses HTML parsing rules without exposing script text as a table.
#[test]
fn parses_foreign_object_html_but_keeps_script_text_opaque() {
    let source = concat!(
        "<svg><foreignObject>",
        "<script>const s = '</svg><table><tr><td>fake</td></tr></table>';</script>",
        "<table id='inside'><tr><td>inside</td></tr></table>",
        "</foreignObject></svg>",
        "<table id='outside'><tr><td>real</td></tr></table>"
    );
    let document = HtmlDocument::parse(source.as_bytes().to_vec()).unwrap();

    assert_eq!(document.tables.len(), 2);
    assert_eq!(document.tables[0].rows[0][0].value, "inside");
    assert_eq!(document.tables[1].rows[0][0].value, "real");
}

// Checks that table-like tags ignored by the HTML tree builder do not become editable grids.
#[test]
fn ignores_table_tokens_rejected_inside_select_elements() {
    let source = "<select><table><tr><td>fake</td></tr></table></select><table><tr><td>real</td></tr></table>";
    let document = HtmlDocument::parse(source.as_bytes().to_vec()).unwrap();

    assert_eq!(document.tables.len(), 1);
    assert_eq!(document.tables[0].rows[0][0].display_value, "real");
}

// Checks that individually valid tables share one document-wide logical-cell budget.
#[test]
fn enforces_logical_cell_budget_across_tables() {
    let source = "<table><tr><td>A</td></tr></table><table><tr><td>B</td></tr></table><table><tr><td>C</td></tr></table>";
    let tokens = tokenizer::tokenize(source).unwrap();

    assert_eq!(
        parser::parse_tables_with_cell_limit(&tokens, source, 2),
        Err(HtmlError::ResourceLimit)
    );
}

// Checks that the callback limit returns an error instead of truncated token success.
#[test]
fn token_limit_is_reported_without_partial_parse() {
    assert!(matches!(
        tokenizer::tokenize_with_token_limit(
            "<table><tr><td>one</td></tr></table><table><tr><td>two</td></tr></table>",
            4,
        ),
        Err(HtmlError::ResourceLimit)
    ));
}

// Checks that dirty raw cell markup becomes plain text without rendering scripts.
#[test]
fn derives_bounded_plain_text_for_dirty_cells() {
    assert_eq!(
        HtmlDocument::display_fragment("<b>word</b>&amp;<br>next<script>hidden()</script>")
            .map(|display| display.display),
        Some("word&\nnext".to_owned())
    );
    assert_eq!(
        HtmlDocument::display_fragment(&"x".repeat(MAX_HTML_DISPLAY_BYTES + 1)),
        None
    );
}

// Keeps a whole-cell title and rejects two separate anchors as one link.
#[test]
fn derives_current_link_shape_and_title_from_markup() {
    let whole =
        HtmlDocument::display_fragment("<a href='/target' title='Keep this'><b>日本語</b></a>")
            .expect("bounded anchor parses");
    assert_eq!(
        whole.hyperlink.as_ref().unwrap().tooltip.as_deref(),
        Some("Keep this")
    );
    assert!(!whole.has_partial_link);
    let split = HtmlDocument::display_fragment("<a href='/same'>A</a><a href='/same'>B</a>")
        .expect("split links parse");
    assert!(split.has_partial_link);
    assert!(split.hyperlink.is_none());
}

// Checks that explicit UTF-8 declarations and UTF-8 BOMs are accepted.
#[test]
fn accepts_utf8_html_charset_declarations() {
    let source = b"\xEF\xBB\xBF<meta charset='UTF-8'><table><tr><td>ok</td></tr></table>";
    assert!(HtmlDocument::parse(source.to_vec()).is_ok());
    let legacy = b"<meta http-equiv='Content-Type' content='text/html; charset=utf-8'><table><tr><td>ok</td></tr></table>";
    assert!(HtmlDocument::parse(legacy.to_vec()).is_ok());
}

// Checks that legacy charset declarations cannot be kept while writing UTF-8 edits.
#[test]
fn rejects_non_utf8_html_charset_declarations() {
    let direct = b"<meta charset='shift_jis'><table><tr><td>ascii</td></tr></table>";
    assert_eq!(
        HtmlDocument::parse(direct.to_vec()),
        Err(HtmlError::UnsupportedEncoding("shift_jis".to_owned()))
    );
    let legacy = b"<meta http-equiv='Content-Type' content='text/html; charset=windows-1252'><table><tr><td>ascii</td></tr></table>";
    assert_eq!(
        HtmlDocument::parse(legacy.to_vec()),
        Err(HtmlError::UnsupportedEncoding("windows-1252".to_owned()))
    );
}

// Checks that charset-like text in comments, scripts, and inert templates is ignored.
#[test]
fn ignores_inert_html_charset_text() {
    let source = concat!(
        "<!-- <meta charset='shift_jis'> -->",
        "<script>const x = \"<meta charset='shift_jis'>\";</script>",
        "<template><meta charset='shift_jis'></template>",
        "<table><tr><td>ok</td></tr></table>"
    );
    assert!(HtmlDocument::parse(source.as_bytes().to_vec()).is_ok());
}

// Checks that supported inline HTML styles and whole-cell links enter the shared model.
#[test]
fn imports_html_inline_styles_and_links_into_cell_metadata() {
    let source = "<table><tr><td><a href='/target'><b>Bold</b> <i>italic</i> <del>gone</del> <strong>linked</strong></a></td></tr></table>";
    let html = HtmlDocument::parse(source.as_bytes().to_vec()).unwrap();
    let document = html.to_document(1, "HTML");
    let format = document.tables[0]
        .format(CellAddress { row: 0, column: 0 })
        .expect("styled cell formatting is loaded");
    let runs = format.rich_text.as_ref().expect("styled runs are loaded");
    assert!(runs.iter().any(|run| run.text == "Bold" && run.bold));
    assert!(runs.iter().any(|run| run.text == "italic" && run.italic));
    assert!(runs.iter().any(|run| run.text == "gone" && run.strike));
    assert!(runs.iter().any(|run| run.text == "linked" && run.bold));
    assert_eq!(
        format.hyperlink.as_ref().map(|link| link.target.as_str()),
        Some("/target")
    );
}
