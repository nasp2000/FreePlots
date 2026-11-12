// SPDX-License-Identifier: MIT
//! Guards the messages a user can read against claiming that a feature is
//! missing, and against placeholder markers left in the sources.
//!
//! Every analysis either runs or refuses with an actionable reason; a report
//! note never says a capability does not exist and no user-facing text carries
//! a to-do marker. The phrases are assembled from fragments so that this file
//! does not contain them itself.

use std::fs;
use std::path::{Path, PathBuf};

/// Upper-case marker words that must not appear in the sources: a note left
/// for later is a placeholder the user can run into. They are matched as whole
/// words and in upper case, so Portuguese words such as "método" or "todos"
/// never match. They are assembled from fragments like the phrases above, so
/// this file does not contain them itself.
fn markers() -> Vec<String> {
    vec![["TO", "DO"].concat(), ["FIX", "ME"].concat()]
}

/// Whether `line` carries `marker` as a whole upper-case word.
fn has_marker(line: &str, marker: &str) -> bool {
    line.split(|character: char| !character.is_alphanumeric() && character != '_')
        .any(|token| token == marker)
}

/// Phrases that must not appear in the sources or the documentation.
fn banned() -> Vec<String> {
    vec![
        ["not ", "implemented"].concat(),
        ["não ", "implementado"].concat(),
        ["un", "implemented"].concat(),
        ["todo", "!()"].concat(),
    ]
}

/// Collects the files of `root` with one of `extensions`.
fn sources(root: &Path, extensions: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) == Some("target") {
                continue;
            }
            sources(&path, extensions, out);
        } else if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| extensions.contains(&ext))
        {
            out.push(path);
        }
    }
}

#[test]
fn no_message_claims_a_feature_is_missing() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crate lives under crates/")
        .to_path_buf();
    let root = crates.parent().expect("crates/ lives in the repository");

    let mut files = Vec::new();
    sources(&crates, &["rs"], &mut files);
    assert!(files.len() > 40, "the crate sources were found");

    // The documentation and the plan make the same promise, so a stale claim
    // is caught there too. A checkout that does not carry them (the published
    // tree keeps the sources only) still checks the sources.
    let mut documentation = Vec::new();
    sources(&root.join("docs"), &["md"], &mut documentation);
    let plan = root.join("PLAN.md");
    if plan.is_file() {
        documentation.push(plan);
    }
    files.extend(documentation);

    let banned = banned();
    let markers = markers();
    let mut hits = Vec::new();
    for path in files {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let lowered = text.to_lowercase();
        for phrase in &banned {
            if let Some(index) = lowered.find(phrase) {
                let line = lowered[..index].matches('\n').count() + 1;
                hits.push(format!("{}:{line}: {phrase}", path.display()));
            }
        }
        for (number, line) in text.lines().enumerate() {
            for marker in &markers {
                if has_marker(line, marker) {
                    hits.push(format!("{}:{}: {marker}", path.display(), number + 1));
                }
            }
        }
    }
    assert!(
        hits.is_empty(),
        "a user-facing message claims a feature is missing or leaves a placeholder:\n{}",
        hits.join("\n")
    );
}
