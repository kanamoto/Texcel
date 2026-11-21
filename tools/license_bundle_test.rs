use super::*;
use serde_json::json;

// Checks that the walk keeps normal and build dependencies but skips Texcel and test-only crates.
#[test]
fn shipped_crates_skip_root_and_dev_only_dependencies() {
    let metadata = json!({
        "packages": [
            {"id": "root", "name": "texcel", "version": "0.1.0", "manifest_path": "/none/Cargo.toml"},
            {"id": "a", "name": "alpha", "version": "1.0.0", "license": "MIT", "authors": ["A <a@example.com>"], "manifest_path": "/none/a/Cargo.toml"},
            {"id": "b", "name": "beta", "version": "2.0.0", "license": "Apache-2.0", "manifest_path": "/none/b/Cargo.toml"},
            {"id": "t", "name": "testonly", "version": "0.3.0", "license": "MIT", "manifest_path": "/none/t/Cargo.toml"}
        ],
        "resolve": {
            "root": "root",
            "nodes": [
                {"id": "root", "deps": [
                    {"pkg": "a", "dep_kinds": [{"kind": null}]},
                    {"pkg": "t", "dep_kinds": [{"kind": "dev"}]}
                ]},
                {"id": "a", "deps": [{"pkg": "b", "dep_kinds": [{"kind": "build"}]}]},
                {"id": "b", "deps": []},
                {"id": "t", "deps": []}
            ]
        }
    });

    let crates = shipped_crates(&metadata).expect("metadata is valid");
    let names: Vec<_> = crates.iter().map(|item| item.name.as_str()).collect();
    assert_eq!(names, ["alpha", "beta"]);
    assert!(crates.iter().all(|item| item.files.is_empty()));
    assert_eq!(crates[0].authors, ["A <a@example.com>"]);
}

// Checks which file names are treated as license or notice files.
#[test]
fn license_names_match_common_spellings_only() {
    for name in [
        "LICENSE",
        "LICENSE-MIT",
        "license.txt",
        "COPYING",
        "NOTICE",
        "OFL.txt",
        "PATCHES.md",
    ] {
        assert!(is_license_name(Path::new(name)), "{name}");
    }
    for name in ["README.md", "Cargo.toml", "lib.rs"] {
        assert!(!is_license_name(Path::new(name)), "{name}");
    }
}

// Checks that files from a license sub-folder keep that folder in their bundled name.
#[test]
fn bundled_names_keep_license_subfolders_apart() {
    assert_eq!(bundled_name(Path::new("/c/fonts/OFL.txt")), "fonts-OFL.txt");
    assert_eq!(bundled_name(Path::new("/c/LICENSE-MIT")), "LICENSE-MIT");
}

// Checks that crates without license files are covered only when a bundled license is offered.
#[test]
fn common_license_detection_reads_old_and_new_expressions() {
    for expression in [
        "MIT",
        "MIT/Apache-2.0",
        "Zlib OR Apache-2.0 OR MIT",
        "(MIT OR Apache-2.0) AND BSD-3-Clause",
    ] {
        assert!(names_common_license(expression, false), "{expression}");
    }
    for expression in ["Zlib", "BSD-3-Clause", "MIT-0", "(not stated)", "BSL-1.0"] {
        assert!(!names_common_license(expression, false), "{expression}");
    }
    // BSL-1.0 counts only after a shipped crate supplied its standard text.
    assert!(names_common_license("BSL-1.0", true));
    assert!(!names_common_license("BSL-1.0-X", true));
}

// Checks that the BSL text is taken from its title and rejected when it is not the standard text.
#[test]
fn bsl_text_drops_a_copyright_header_and_requires_the_standard_body() {
    let standard = format!(
        "{BSL_TITLE}\n\nPermission is hereby granted, free of charge, to any person or organization\n\
         ...\nTHE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND\n"
    );
    let with_header = format!("Copyright (c) 2020 Someone\n\n{standard}");
    assert_eq!(standard_bsl_text(&with_header), Some(standard.clone()));
    assert_eq!(
        standard_bsl_text(&format!("{BSL_TITLE}\n\nCustom terms\n")),
        None
    );
    assert_eq!(standard_bsl_text("MIT License"), None);
}
