//! Traceability gate for behavior specs (ADR-001, workflow rule 4).
//!
//! Reads every spec under `docs/specs/domain/` and `docs/specs/application/` and checks that
//!
//! 1. rule IDs are well-formed (`LC-01`) and unique across all specs;
//! 2. every rule has a known status (`Draft`, `Approved`, `Deprecated`);
//! 3. every **Approved** rule is mentioned by at least one test in `domain/tests/` or
//!    `application/tests/` (as an `"LC-01"` literal or in a test name such as `lc_01_...`);
//! 4. every rule ID used by a test (literal or test name) exists in a spec.
//!
//! This only proves that a rule is *referenced*. Whether the test is *right* is a human review
//! (see `docs/specs/GUIDELINES.md`, part B).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

const STATUSES: [&str; 3] = ["Draft", "Approved", "Deprecated"];

struct Rule {
    status: String,
    file: PathBuf,
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("domain crate has a parent directory")
        .to_path_buf()
}

fn files_with_extension(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_with_extension(&path, ext, out);
        } else if path.extension().is_some_and(|e| e == ext) {
            out.push(path);
        }
    }
}

/// Parses rules from one spec file. Two layouts are supported:
/// - heading rule:  `### LC-01 — title` followed by a `- **Status**: Approved` line;
/// - table rule:    `| LC-01 | Approved | ... |` (status is the second column).
fn parse_spec(path: &Path, errors: &mut Vec<String>) -> Vec<(String, Rule)> {
    let text = fs::read_to_string(path).expect("spec is readable");
    let heading = Regex::new(r"^#{3,4}\s+`?([A-Z]{2,4}-\d{2,3})`?\b").unwrap();
    let status_line = Regex::new(r"^-\s+\*\*Status\*\*:\s*(\S+)").unwrap();
    let table_row = Regex::new(r"^\|\s*`?([A-Z]{2,4}-\d{2,3})`?\s*\|\s*`?(\S+?)`?\s*\|").unwrap();
    let loose_id = Regex::new(r"^#{3,4}\s+`?([A-Za-z]{1,6}-\d+)`?\b").unwrap();

    let mut rules: Vec<(String, Rule)> = Vec::new();
    let mut pending_heading: Option<String> = None;

    let flush = |pending: &mut Option<String>, errors: &mut Vec<String>| {
        if let Some(id) = pending.take() {
            errors.push(format!("{}: rule {id} has no `- **Status**:` line", path.display()));
        }
    };

    for line in text.lines() {
        if let Some(c) = heading.captures(line) {
            flush(&mut pending_heading, errors);
            pending_heading = Some(c[1].to_string());
        } else if let Some(c) = loose_id.captures(line) {
            errors.push(format!(
                "{}: malformed rule ID `{}` (expected e.g. `LC-01`)",
                path.display(),
                &c[1]
            ));
        } else if let Some(c) = status_line.captures(line) {
            if let Some(id) = pending_heading.take() {
                rules.push((
                    id,
                    Rule {
                        status: c[1].trim_matches('`').to_string(),
                        file: path.to_path_buf(),
                    },
                ));
            }
        } else if let Some(c) = table_row.captures(line) {
            rules.push((
                c[1].to_string(),
                Rule {
                    status: c[2].trim_matches('`').to_string(),
                    file: path.to_path_buf(),
                },
            ));
        }
    }
    flush(&mut pending_heading, errors);
    rules
}

fn normalise(id: &str) -> String {
    id.to_ascii_uppercase().replace('_', "-")
}

#[test]
fn specs_and_tests_are_consistent() {
    let root = workspace_root();
    let mut errors: Vec<String> = Vec::new();

    // --- collect rules -------------------------------------------------------------------
    let mut spec_files = Vec::new();
    for layer in ["domain", "application"] {
        files_with_extension(&root.join("docs/specs").join(layer), "md", &mut spec_files);
    }
    let mut rules: BTreeMap<String, Rule> = BTreeMap::new();
    for file in &spec_files {
        for (id, rule) in parse_spec(file, &mut errors) {
            if !STATUSES.contains(&rule.status.as_str()) {
                errors.push(format!(
                    "{}: rule {id} has unknown status `{}` (allowed: {STATUSES:?})",
                    rule.file.display(),
                    rule.status
                ));
            }
            if let Some(prev) = rules.get(&id) {
                errors.push(format!(
                    "duplicate rule ID {id}: {} and {}",
                    prev.file.display(),
                    rule.file.display()
                ));
            } else {
                rules.insert(id, rule);
            }
        }
    }

    // --- collect test references --------------------------------------------------------
    let mut test_files = Vec::new();
    for krate in ["domain", "application"] {
        files_with_extension(&root.join(krate).join("tests"), "rs", &mut test_files);
    }
    test_files.retain(|p| p.file_name().is_some_and(|n| n != "spec_traceability.rs"));

    let literal = Regex::new(r#""([A-Z]{2,4}-\d{2,3})""#).unwrap();
    let test_name = Regex::new(r"fn\s+([a-z]{2,4})_(\d{2,3})(?:_|\b)").unwrap();
    let mentioned_anywhere = |id: &str, text: &str| -> bool {
        let (prefix, num) = id.split_once('-').expect("id has a dash");
        Regex::new(&format!(r"(?i){prefix}[-_]{num}(?:[^0-9]|$)"))
            .unwrap()
            .is_match(text)
    };

    let mut sources = String::new();
    let mut referenced: BTreeSet<String> = BTreeSet::new();
    for file in &test_files {
        let text = fs::read_to_string(file).expect("test file is readable");
        for c in literal.captures_iter(&text) {
            referenced.insert(c[1].to_string());
        }
        for c in test_name.captures_iter(&text) {
            referenced.insert(normalise(&format!("{}-{}", &c[1], &c[2])));
        }
        sources.push_str(&text);
        sources.push('\n');
    }

    // --- checks 3 and 4 ---------------------------------------------------------------
    for (id, rule) in &rules {
        if rule.status == "Approved" && !mentioned_anywhere(id, &sources) {
            errors.push(format!(
                "Approved rule {id} ({}) has no test: mention it as \"{id}\" or in a test name",
                rule.file.display()
            ));
        }
    }
    for id in &referenced {
        if !rules.contains_key(id) {
            errors.push(format!("tests reference rule {id}, which does not exist in any spec"));
        }
    }

    assert!(
        errors.is_empty(),
        "spec/test traceability violations:\n  - {}",
        errors.join("\n  - ")
    );
}
