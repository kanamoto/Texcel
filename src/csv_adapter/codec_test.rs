use super::*;

#[test]
// Selects a consistent tab table when its numeric fields contain comma groups.
fn delimiter_choice_prioritizes_consistency_over_spurious_width() {
    let bytes = b"name\tamount\nx\t1,000,000\ny\t2,000,000\n";

    let analysis = analyze_delimiters(bytes, EncodingChoice::Auto).expect("UTF-8 is valid");
    let comma = analysis
        .candidates
        .iter()
        .find(|candidate| candidate.delimiter == ',')
        .expect("comma candidate exists");
    let tab = analysis
        .candidates
        .iter()
        .find(|candidate| candidate.delimiter == '\t')
        .expect("tab candidate exists");

    assert_eq!(comma.most_common_width, 3);
    assert_eq!(tab.most_common_width, 2);
    assert_eq!(analysis.suggested, Some('\t'));
}

#[test]
// Prompts when two plausible multi-column delimiters have equal consistency.
fn tied_multi_column_candidates_keep_the_choice_with_the_user() {
    let analysis =
        analyze_delimiters(b"a,b;c\nd,e;f\n", EncodingChoice::Auto).expect("UTF-8 is valid");

    assert_eq!(analysis.suggested, None);
}

#[test]
// Keeps a unique ragged multi-column candidate eligible for normal loading.
fn ragged_csv_keeps_its_unique_multi_column_delimiter() {
    let analysis =
        analyze_delimiters(b"a,b\nc,d,e\n", EncodingChoice::Auto).expect("UTF-8 is valid");

    assert_eq!(analysis.suggested, Some(','));
}
