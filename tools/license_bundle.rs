use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// One crate that is built into Texcel, with the license files found in its package.
#[derive(Debug, PartialEq, Eq)]
struct CrateLicense {
    name: String,
    version: String,
    license: String,
    authors: Vec<String>,
    repository: String,
    files: Vec<PathBuf>,
}

/// File name starts that mark license, notice, and patch files inside a crate package.
const LICENSE_FILE_PREFIXES: [&str; 8] = [
    "LICENSE",
    "LICENCE",
    "COPYING",
    "COPYRIGHT",
    "NOTICE",
    "UNLICENSE",
    "THIRD-PARTY",
    "PATCHES",
];

/// The first line of the standard Boost Software License text.
const BSL_TITLE: &str = "Boost Software License - Version 1.0 - August 17th, 2003";

/// Folders inside a crate package that may hold license files for bundled data such as fonts.
const LICENSE_SUBFOLDERS: [&str; 3] = ["fonts", "licenses", "LICENSES"];

// Copy the license text of Texcel and of every crate in the release build into one folder.
fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut output = root.join("target/package/licenses");
    let mut target = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--target" => target = args.next(),
            _ if !arg.starts_with('-') => output = PathBuf::from(arg),
            _ => {
                eprintln!("usage: texcel-licenses [output-dir] [--target <triple>]");
                std::process::exit(2);
            }
        }
    }

    if let Err(error) = run(&root, &output, target) {
        eprintln!("license bundle failed: {error}");
        std::process::exit(1);
    }
    println!("created {}", output.display());
}

/// Reads Cargo metadata for one target and writes the license folder.
fn run(root: &Path, output: &Path, target: Option<String>) -> Result<(), String> {
    let target = match target {
        Some(target) => target,
        None => host_target()?,
    };
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let metadata = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--locked"])
        .args(["--filter-platform", &target])
        .arg("--manifest-path")
        .arg(root.join("Cargo.toml"))
        .output()
        .map_err(|error| format!("cannot run cargo metadata: {error}"))?;
    if !metadata.status.success() {
        return Err(String::from_utf8_lossy(&metadata.stderr).into_owned());
    }
    let metadata: Value = serde_json::from_slice(&metadata.stdout)
        .map_err(|error| format!("cannot read cargo metadata: {error}"))?;
    let crates = shipped_crates(&metadata)?;
    write_bundle(root, output, &crates)
}

/// Asks rustc which target it builds for by default.
fn host_target() -> Result<String, String> {
    let output = Command::new("rustc")
        .arg("-vV")
        .output()
        .map_err(|error| format!("cannot run rustc: {error}"))?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("host: ").map(str::to_owned))
        .ok_or_else(|| "rustc did not report a host target".to_owned())
}

/// Lists every crate reachable from Texcel through normal and build dependencies.
fn shipped_crates(metadata: &Value) -> Result<Vec<CrateLicense>, String> {
    let resolve = &metadata["resolve"];
    let root = resolve["root"]
        .as_str()
        .ok_or("cargo metadata has no root package")?;

    // Index the resolved graph so the walk can follow each crate's dependencies.
    let mut edges: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for node in resolve["nodes"].as_array().into_iter().flatten() {
        let id = node["id"].as_str().unwrap_or_default();
        let deps = node["deps"].as_array().into_iter().flatten();
        let shipped = deps.filter(|dep| !is_dev_only(dep));
        edges.insert(id, shipped.filter_map(|dep| dep["pkg"].as_str()).collect());
    }

    // Walk from the Texcel package; tests-only crates are never reached.
    let mut reached = BTreeSet::new();
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        if reached.insert(id) {
            pending.extend(edges.get(id).into_iter().flatten().copied());
        }
    }

    // Turn each reached package except Texcel itself into a license entry.
    let mut crates = Vec::new();
    for package in metadata["packages"].as_array().into_iter().flatten() {
        let id = package["id"].as_str().unwrap_or_default();
        if id == root || !reached.contains(id) {
            continue;
        }
        let manifest = PathBuf::from(package["manifest_path"].as_str().unwrap_or_default());
        let folder = manifest.parent().unwrap_or(Path::new("."));
        let mut files = license_files(folder);
        if let Some(extra) = package["license_file"].as_str() {
            let extra = folder.join(extra);
            if extra.is_file() && !files.contains(&extra) {
                files.push(extra);
            }
        }
        crates.push(CrateLicense {
            name: package["name"].as_str().unwrap_or_default().to_owned(),
            version: package["version"].as_str().unwrap_or_default().to_owned(),
            license: package["license"]
                .as_str()
                .unwrap_or("(not stated)")
                .to_owned(),
            authors: package["authors"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|author| author.as_str().map(str::to_owned))
                .collect(),
            repository: package["repository"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            files,
        });
    }
    crates.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
    Ok(crates)
}

/// Tells whether a dependency edge is used only by tests and examples.
fn is_dev_only(dep: &Value) -> bool {
    let kinds = dep["dep_kinds"].as_array();
    kinds.is_some_and(|kinds| {
        !kinds.is_empty()
            && kinds
                .iter()
                .all(|kind| kind["kind"].as_str() == Some("dev"))
    })
}

/// Finds license files at the top of a crate package and in its known license folders.
fn license_files(folder: &Path) -> Vec<PathBuf> {
    let mut files = matching_files(folder);
    for sub in LICENSE_SUBFOLDERS {
        files.extend(matching_files(&folder.join(sub)));
    }
    files
}

/// Returns the files in one folder whose names look like license or notice files.
fn matching_files(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_license_name(path))
        .collect();
    files.sort();
    files
}

/// Matches names such as LICENSE-MIT, COPYING.txt, NOTICE, or OFL.txt.
fn is_license_name(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_uppercase())
        .unwrap_or_default();
    LICENSE_FILE_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
        || name.starts_with("OFL")
        || name.starts_with("UFL")
}

/// Writes Texcel's own notices and the license files of each crate.
fn write_bundle(root: &Path, output: &Path, crates: &[CrateLicense]) -> Result<(), String> {
    if output.exists() {
        fs::remove_dir_all(output).map_err(|error| io_error(output, error))?;
    }
    copy(&root.join("LICENSE"), &output.join("LICENSE"))?;
    copy(
        &root.join("assets/fonts/LICENSE-Noto-Sans-CJK.txt"),
        &output.join("fonts/LICENSE-Noto-Sans-CJK.txt"),
    )?;

    // Copy each crate's files into its own folder, keeping sub-folder names apart.
    let mut missing = Vec::new();
    for item in crates {
        let folder = output
            .join("crates")
            .join(format!("{}-{}", item.name, item.version));
        for file in &item.files {
            copy(file, &folder.join(bundled_name(file)))?;
        }
        if item.files.is_empty() {
            write_text(&folder.join("LICENSE-NOTE.txt"), &missing_note(item))?;
            missing.push(item);
        }
    }

    // Crates without license files point to the standard texts in the common folder.
    let bsl_bundled = write_common_texts(root, output, crates)?;
    let uncovered: Vec<_> = missing
        .iter()
        .filter(|item| !names_common_license(&item.license, bsl_bundled))
        .map(|item| format!("{} {} ({})", item.name, item.version, item.license))
        .collect();
    if !uncovered.is_empty() {
        return Err(format!(
            "no license text for crate(s) without license files: {}",
            uncovered.join(", ")
        ));
    }
    Ok(())
}

/// Describes a crate whose package has no license file, using its Cargo.toml fields.
fn missing_note(item: &CrateLicense) -> String {
    let authors = if item.authors.is_empty() {
        "(none listed)".to_owned()
    } else {
        item.authors.join(", ")
    };
    format!(
        "The published {} {} package does not include a license file.\n\
         License stated in its Cargo.toml: {}\n\
         Authors stated in its Cargo.toml: {authors}\n\
         Repository: {}\n\n\
         The standard text of the license named above is in ../../common/.\n",
        item.name, item.version, item.license, item.repository
    )
}

/// Tells whether a license expression offers a license whose standard text is bundled.
///
/// MIT and Apache-2.0 are always bundled; BSL-1.0 only when a shipped crate carries its text.
fn names_common_license(expression: &str, bsl_bundled: bool) -> bool {
    expression
        .split(|c: char| c == '/' || c == '(' || c == ')' || c.is_whitespace())
        .any(|name| name == "MIT" || name == "Apache-2.0" || bsl_bundled && name == "BSL-1.0")
}

/// Writes the standard license texts without any one project's copyright line.
///
/// Returns whether the BSL-1.0 text was found in a shipped crate and written.
fn write_common_texts(root: &Path, output: &Path, crates: &[CrateLicense]) -> Result<bool, String> {
    // Texcel's own MIT file supplies the permission text after its copyright line.
    let texcel = fs::read_to_string(root.join("LICENSE")).map_err(|error| io_error(root, error))?;
    let permission = texcel
        .split_once("Permission is hereby granted")
        .map(|(_, rest)| format!("Permission is hereby granted{rest}"))
        .ok_or("LICENSE is not the MIT license text")?;
    write_text(
        &output.join("common/MIT.txt"),
        &format!("MIT License\n\n{permission}"),
    )?;

    // The Apache text comes from a crate that kept the standard placeholder appendix.
    let apache = crates
        .iter()
        .flat_map(|item| &item.files)
        .filter(|file| bundled_name(file).to_ascii_uppercase().contains("APACHE"))
        .filter_map(|file| fs::read_to_string(file).ok())
        .find(|text| is_standard_apache_text(text))
        .ok_or("no crate package has the standard Apache-2.0 license text")?;
    write_text(&output.join("common/Apache-2.0.txt"), &apache)?;

    // The BSL text, when present, comes from a shipped crate's own license file.
    let bsl = crates
        .iter()
        .flat_map(|item| &item.files)
        .filter_map(|file| fs::read_to_string(file).ok())
        .find_map(|text| standard_bsl_text(&text));
    if let Some(bsl) = &bsl {
        write_text(&output.join("common/BSL-1.0.txt"), bsl)?;
    }
    Ok(bsl.is_some())
}

/// Returns the standard BSL-1.0 text from its title onward, without any copyright line above it.
fn standard_bsl_text(text: &str) -> Option<String> {
    let start = text.find(BSL_TITLE)?;
    let body = &text[start..];
    (body.contains("Permission is hereby granted, free of charge, to any person or organization")
        && body.contains("THE SOFTWARE IS PROVIDED \"AS IS\""))
    .then(|| body.to_owned())
}

/// Accepts only the unmodified Apache-2.0 text whose appendix still has placeholders.
fn is_standard_apache_text(text: &str) -> bool {
    text.contains("Apache License")
        && text.contains("Version 2.0, January 2004")
        && text.contains("[yyyy] [name of copyright owner]")
}

/// Writes one text file and creates its folder first.
fn write_text(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
    }
    fs::write(path, text).map_err(|error| io_error(path, error))
}

/// Keeps a license file's sub-folder in its name, for example fonts-OFL.txt.
fn bundled_name(file: &Path) -> String {
    let name = file.file_name().unwrap_or_default().to_string_lossy();
    let parent = file
        .parent()
        .and_then(Path::file_name)
        .map(|parent| parent.to_string_lossy());
    match parent {
        Some(parent) if LICENSE_SUBFOLDERS.contains(&parent.as_ref()) => {
            format!("{parent}-{name}")
        }
        _ => name.into_owned(),
    }
}

/// Copies one file and creates its folder first.
fn copy(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
    }
    fs::copy(from, to)
        .map(|_| ())
        .map_err(|error| io_error(from, error))
}

/// Formats a file error together with the path that caused it.
fn io_error(path: &Path, error: std::io::Error) -> String {
    format!("{}: {error}", path.display())
}

#[cfg(test)]
#[path = "license_bundle_test.rs"]
mod tests;
