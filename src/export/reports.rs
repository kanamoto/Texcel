use super::*;

// Record when Markdown text formatting cannot be kept in the target format.
pub(super) fn report_markdown_text_conversion(
    entries: &mut Vec<ConversionEntry>,
    table: &ExportTable,
    address: CellAddress,
    content: &MarkdownCellContent,
) {
    if content.has_link {
        add_conversion_with_message(
            entries,
            table,
            address,
            "リンク先URLを保持できないため表示文字列に変換しました",
            crate::i18n::Message::new("report-reason-link-url-text"),
        );
    }
    if content.has_reference_link {
        add_omission_with_message(
            entries,
            table,
            address,
            "表外の参照リンク定義を出力へ引き継げないため、リンク先を表示文字列へ変換しました",
            crate::i18n::Message::new("report-reason-reference-link-text"),
        );
    }
    if content.has_image {
        add_conversion_with_message(
            entries,
            table,
            address,
            "画像URLを保持できないため代替テキストに変換しました",
            crate::i18n::Message::new("report-reason-image-alt-text"),
        );
    }
    if content.has_code {
        add_conversion_with_message(
            entries,
            table,
            address,
            "インラインコードの書体を通常文字へ変換しました",
            crate::i18n::Message::new("report-reason-inline-code-style"),
        );
    }
    if content.has_html {
        add_conversion_with_message(
            entries,
            table,
            address,
            "任意HTMLを実行せず文字列として出力しました",
            crate::i18n::Message::new("report-reason-html-text"),
        );
    }
}
