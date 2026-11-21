use super::*;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[test]
// Keeps the imported source path as an origin until a Markdown destination is saved.
fn imported_html_uses_markdown_source_and_requires_a_matching_save_path() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is after the epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("texcel-origin-{unique}.html"));
    let input = "<table><tr><th>Item</th></tr><tr><td>Value</td></tr></table>";
    std::fs::write(&path, input).expect("write HTML source");
    let context = egui::Context::default();
    let mut app = new_app(&context);
    let document_id = app
        .start_path_load(path.clone())
        .expect("HTML import is queued");
    let token = app.pending_html_loads[&document_id];
    let deadline = Instant::now() + Duration::from_secs(5);
    let result = loop {
        let _ = app.file_worker.poll_load_progress();
        if let Some(result) = app.file_worker.poll().into_iter().next() {
            break result;
        }
        assert!(
            Instant::now() < deadline,
            "HTML import worker completes; stopped={}, status={:?}, token={:?}",
            app.file_worker.is_stopped(),
            app.status,
            token
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(result.token, token);
    assert!(!app.dispatch_file_result(result));

    let document = app
        .workspace
        .documents
        .get(&document_id)
        .expect("imported document is adopted");
    assert!(matches!(
        document.source.as_deref(),
        Some(SourceDocument::Markdown(_))
    ));
    assert!(app.document_requires_save(document_id));
    assert!(app.save_as_path_supported(document_id, std::path::Path::new("converted.md")));
    assert!(!app.save_as_path_supported(document_id, std::path::Path::new("source.html")));
    assert_eq!(
        app.file_backings[&document_id].path,
        std::fs::canonicalize(&path).expect("resolve the origin"),
        "the origin path remains attached for conflict checks"
    );
    assert_eq!(
        std::fs::read(&path).expect("read origin again"),
        input.as_bytes()
    );
    std::fs::remove_file(path).expect("remove HTML source");
}
