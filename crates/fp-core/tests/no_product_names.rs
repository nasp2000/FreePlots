//! Guards the repository against naming the product used for comparison.
//!
//! The code, the tests, the validation pack and the documentation describe the
//! captured behaviour in neutral terms ("the captured run", "the fixtures");
//! they never name or version the product that produced them.

use std::fs;
use std::path::{Path, PathBuf};

/// Terms that must not appear in the repository.
///
/// They are assembled from fragments so that this file does not contain them
/// itself.
fn banned() -> Vec<String> {
    vec![
        ["Mini", "tab"].concat(),
        ["reference", " product"].concat(),
        ["reference", " application"].concat(),
        ["captured", " reference"].concat(),
        ["reference", "-output"].concat(),
        ["22", ".4.0.0"].concat(),
    ]
}

fn files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some("target") {
                continue;
            }
            files(&path, out);
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("rs" | "md" | "py" | "sh" | "toml" | "txt" | "yml")
        ) {
            out.push(path);
        }
    }
}

#[test]
fn no_comparison_product_names() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repository root")
        .to_path_buf();
    let terms = banned();
    let mut paths = Vec::new();
    for directory in [
        "crates",
        "validation",
        "docs",
        "scripts",
        "licenses",
        ".github",
    ] {
        files(&root.join(directory), &mut paths);
    }
    for file in [
        "report.md",
        "README.md",
        "PLAN.md",
        "AGENTS.md",
        "THIRD-PARTY.md",
    ] {
        let path = root.join(file);
        if path.exists() {
            paths.push(path);
        }
    }
    let mut hits = Vec::new();
    for path in paths {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        for term in &terms {
            if text.to_lowercase().contains(&term.to_lowercase()) {
                hits.push(format!("{}: {term}", path.display()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "the repository names the comparison product: {hits:#?}"
    );
}
