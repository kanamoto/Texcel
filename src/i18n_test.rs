use super::*;
use fluent_syntax::ast::{Entry, Expression, InlineExpression, Pattern, PatternElement, Resource};
use std::collections::{BTreeMap, BTreeSet};

/// Describes the variables used by one Fluent message and its attributes.
#[derive(Debug, Eq, PartialEq)]
struct MessageShape {
    value: Option<BTreeSet<String>>,
    attributes: BTreeMap<String, BTreeSet<String>>,
}

/// Collects variables from a Fluent pattern using the official resource AST.
fn pattern_variables(pattern: &Pattern<&str>) -> BTreeSet<String> {
    let mut variables = BTreeSet::new();
    for element in &pattern.elements {
        if let PatternElement::Placeable { expression } = element {
            expression_variables(expression, &mut variables);
        }
    }
    variables
}

/// Collects variables from both direct values and plural branches.
fn expression_variables(expression: &Expression<&str>, variables: &mut BTreeSet<String>) {
    match expression {
        Expression::Inline(inline) => inline_variables(inline, variables),
        Expression::Select { selector, variants } => {
            inline_variables(selector, variables);
            for variant in variants {
                variables.extend(pattern_variables(&variant.value));
            }
        }
    }
}

/// Collects variable references from an inline Fluent expression.
fn inline_variables(expression: &InlineExpression<&str>, variables: &mut BTreeSet<String>) {
    match expression {
        InlineExpression::VariableReference { id } => {
            variables.insert(id.name.to_owned());
        }
        InlineExpression::Placeable { expression } => expression_variables(expression, variables),
        InlineExpression::FunctionReference { arguments, .. } => {
            for argument in &arguments.positional {
                inline_variables(argument, variables);
            }
            for argument in &arguments.named {
                inline_variables(&argument.value, variables);
            }
        }
        InlineExpression::TermReference { arguments, .. } => {
            if let Some(arguments) = arguments {
                for argument in &arguments.positional {
                    inline_variables(argument, variables);
                }
                for argument in &arguments.named {
                    inline_variables(&argument.value, variables);
                }
            }
        }
        InlineExpression::StringLiteral { .. }
        | InlineExpression::NumberLiteral { .. }
        | InlineExpression::MessageReference { .. } => {}
    }
}

/// Merges one Fluent resource and rejects duplicate message identifiers.
fn merge_resource(
    source: &str,
    catalog: &mut BTreeMap<String, MessageShape>,
    duplicates: &mut BTreeSet<String>,
) {
    let resource: Resource<&str> =
        fluent_syntax::parser::parse(source).expect("valid Fluent resource");
    for entry in resource.body {
        let (key, value, attributes) = match entry {
            Entry::Message(message) => (
                message.id.name.to_owned(),
                message.value,
                message.attributes,
            ),
            Entry::Term(term) => (
                format!("-{}", term.id.name),
                Some(term.value),
                term.attributes,
            ),
            Entry::Junk { content } => panic!("invalid Fluent resource block: {content}"),
            Entry::Comment(_) | Entry::GroupComment(_) | Entry::ResourceComment(_) => continue,
        };
        let shape = MessageShape {
            value: value.as_ref().map(pattern_variables),
            attributes: attributes
                .into_iter()
                .map(|attribute| {
                    (
                        attribute.id.name.to_owned(),
                        pattern_variables(&attribute.value),
                    )
                })
                .collect(),
        };
        if catalog.insert(key.clone(), shape).is_some() {
            duplicates.insert(key);
        }
    }
}

/// Uses Japanese for Japanese tags and English for all unsupported tags.
#[test]
fn system_language_has_an_explicit_english_fallback() {
    assert_eq!(resolve_system_tag(Some("ja-JP")), Language::Japanese);
    assert_eq!(resolve_system_tag(Some("JA")), Language::Japanese);
    assert_eq!(resolve_system_tag(Some("fr-FR")), Language::English);
    assert_eq!(resolve_system_tag(None), Language::English);
}

/// Resolves translated values and named arguments from the packaged FTL files.
#[test]
fn fluent_messages_support_both_languages_and_arguments() {
    let message = Message::new("dialog-save-conflict-help").with_text("name", "Rin.md");
    assert!(
        message
            .resolve(Language::English)
            .contains("Another app changed")
    );
    assert!(message.resolve(Language::English).contains("Rin.md"));
    assert!(
        message
            .resolve(Language::Japanese)
            .contains("ほかのアプリで変更")
    );
    assert!(message.resolve(Language::Japanese).contains("Rin.md"));
}

/// Leaves authored directional marks intact without adding marks around Fluent arguments.
#[test]
fn fluent_arguments_keep_only_directional_marks_the_user_supplied() {
    let name = "Quarterly \u{2068}source\u{2069}";
    let resolved = Message::new("top-imported-document-name")
        .with_text("name", name)
        .resolve(Language::English);
    assert_eq!(resolved, format!("{name} (imported)"));
}

/// Resolves one retained status again after the active language changes.
#[test]
fn retained_status_message_tracks_the_current_language() {
    let status = Message::new("status-export-cancelled");
    assert_eq!(status.resolve(Language::English), "Export cancelled.");
    assert_eq!(
        status.resolve(Language::Japanese),
        "エクスポートをキャンセルしました。"
    );
}

/// Reuses one partial-export message with paths joined in the active language.
#[test]
fn retained_export_paths_follow_the_current_language() {
    let status = Message::new("status-export-partial")
        .with_number("written", 2)
        .with_message("remaining", Message::new("dialog-empty-tail"))
        .with_message(
            "paths",
            Message::new("status-export-successful-paths")
                .with_text_list("paths", vec!["one.csv".to_owned(), "two.csv".to_owned()]),
        )
        .with_text("failed_path", "three.csv")
        .with_message("detail", Message::new("status-file-operation-failed"))
        .with_message("omitted", Message::new("dialog-empty-tail"));
    let english = status.resolve(Language::English);
    let japanese = status.resolve(Language::Japanese);
    assert!(!english.contains('\u{2068}') && !english.contains('\u{2069}'));
    assert!(!japanese.contains('\u{2068}') && !japanese.contains('\u{2069}'));
    assert!(english.contains("one.csv, two.csv"), "{english}");
    assert!(english.contains("File operation failed."));
    assert!(!english.contains("permission denied"));
    assert!(japanese.contains("one.csv、two.csv"), "{japanese}");
    assert!(japanese.contains("ファイルを処理できませんでした。"));
    assert!(!japanese.contains("permission denied"));
    assert!(english.contains("three.csv"));
    assert!(japanese.contains("three.csv"));
}

/// Checks every Fluent file for duplicate keys and matching translated variables.
#[test]
fn all_fluent_resources_have_matching_keys_and_variables() {
    let mut english = BTreeMap::new();
    let mut english_duplicates = BTreeSet::new();
    merge_resource(
        include_str!("../locales/en-US/main.ftl"),
        &mut english,
        &mut english_duplicates,
    );
    merge_resource(
        include_str!("../locales/en-US/status.ftl"),
        &mut english,
        &mut english_duplicates,
    );
    let mut japanese = BTreeMap::new();
    let mut japanese_duplicates = BTreeSet::new();
    merge_resource(
        include_str!("../locales/ja-JP/main.ftl"),
        &mut japanese,
        &mut japanese_duplicates,
    );
    merge_resource(
        include_str!("../locales/ja-JP/status.ftl"),
        &mut japanese,
        &mut japanese_duplicates,
    );

    assert!(
        english_duplicates.is_empty(),
        "English duplicate keys: {english_duplicates:?}"
    );
    assert!(
        japanese_duplicates.is_empty(),
        "Japanese duplicate keys: {japanese_duplicates:?}"
    );
    assert_eq!(
        english.keys().collect::<Vec<_>>(),
        japanese.keys().collect::<Vec<_>>()
    );
    for (key, english_shape) in &english {
        assert_eq!(
            Some(english_shape),
            japanese.get(key),
            "resource mismatch: {key}"
        );
    }
}

/// Keys built at run time instead of written as one string literal in the source.
const DYNAMIC_KEYS: &[&str] = &[];

/// Reads every message key from one language's FTL files.
fn locale_keys(language: &str) -> std::collections::BTreeSet<String> {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("locales")
        .join(language);
    let mut keys = std::collections::BTreeSet::new();
    for entry in std::fs::read_dir(folder).expect("read locale folder") {
        let text = std::fs::read_to_string(entry.expect("read locale entry").path())
            .expect("read FTL file");
        for line in text.lines() {
            if let Some((key, _)) = line.split_once('=')
                && line.starts_with(|first: char| first.is_ascii_lowercase())
            {
                keys.insert(key.trim().to_owned());
            }
        }
    }
    keys
}

/// Collects the app's source text, leaving out test files.
fn app_source_text(folder: &std::path::Path, text: &mut String) {
    for entry in std::fs::read_dir(folder).expect("read source folder") {
        let path = entry.expect("read source entry").path();
        if path.is_dir() {
            app_source_text(&path, text);
        } else if path.extension().is_some_and(|extension| extension == "rs")
            && !path.to_string_lossy().ends_with("_test.rs")
        {
            text.push_str(&std::fs::read_to_string(&path).expect("read source file"));
        }
    }
}

/// Checks that both languages define the same keys, the app uses every key, and every key used has a translation.
#[test]
fn every_translation_key_exists_in_both_languages_and_is_used() {
    let english = locale_keys("en-US");
    let japanese = locale_keys("ja-JP");
    assert_eq!(
        english, japanese,
        "English and Japanese define different keys"
    );

    let mut source = String::new();
    app_source_text(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut source,
    );
    let unused = english
        .iter()
        .filter(|key| !DYNAMIC_KEYS.contains(&key.as_str()))
        .filter(|key| !source.contains(&format!("\"{key}\"")))
        .collect::<Vec<_>>();
    assert!(unused.is_empty(), "unused translation keys: {unused:?}");

    // Every key-like string literal in the app must have a translation.
    let prefixes = english
        .iter()
        .filter_map(|key| key.split_once('-').map(|(prefix, _)| prefix))
        .collect::<std::collections::BTreeSet<_>>();
    // Look at every quoted word, so escaped quotes elsewhere cannot shift the pairing.
    let missing = source
        .match_indices('"')
        .filter_map(|(start, _)| {
            let rest = &source[start + 1..];
            let end = rest.find(|character: char| {
                !(character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-')
            })?;
            (rest[end..].starts_with('"')).then_some(&rest[..end])
        })
        .filter(|literal| {
            literal
                .split_once('-')
                .is_some_and(|(prefix, rest)| prefixes.contains(prefix) && !rest.is_empty())
        })
        .filter(|literal| !english.contains(*literal))
        .collect::<std::collections::BTreeSet<_>>();
    assert!(missing.is_empty(), "keys without translations: {missing:?}");
}

#[test]
/// Names the import report after the format that was read.
fn import_report_title_names_the_input_format() {
    let title = |language| {
        Message::new("dialog-report-title-import")
            .with_text("format", "HTML")
            .resolve(language)
    };
    assert_eq!(title(Language::Japanese), "HTML読み込みレポート");
    assert_eq!(title(Language::English), "HTML Import Report");
}
