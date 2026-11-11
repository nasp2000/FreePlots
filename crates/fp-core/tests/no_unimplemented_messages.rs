// SPDX-License-Identifier: MIT
//! Guards the messages a user can read against claiming that a feature is
//! missing.
//!
//! Every analysis either runs or refuses with an actionable reason; a report
//! note never says a capability does not exist. The phrases are assembled from
//! fragments so that this file does not contain them itself.

use std::fs;
use std::path::{Path, PathBuf};

/// Phrases that must not appear in the crate sources.
fn banned() -> Vec<String> {
    vec![
        ["not ", "implemented"].concat(),
        ["não ", "implementado"].concat(),
        ["un", "implemented"].concat(),
        ["todo", "!()"].concat(),
    ]
}

fn sources(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) == Some("target") {
                continue;
            }
            sources(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
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
    let mut files = Vec::new();
    sources(&crates, &mut files);
    assert!(files.len() > 20, "the crate sources were found");

    let banned = banned();
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
    }
    assert!(
        hits.is_empty(),
        "a user-facing message claims a feature is missing:\n{}",
        hits.join("\n")
    );
}
