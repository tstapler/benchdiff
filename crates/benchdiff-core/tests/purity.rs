//! Requirement C-1: core is I/O-free, so it must not use `std::fs`, `std::net`, `std::process` or
//! `std::env` outside `#[cfg(test)]` items. I/O lives in the binary and `index.js`.
//!
//! The scanner strips each `#[cfg(test)]` item by brace matching, then searches. Strings and comments
//! are not parsed, so a brace inside a string literal in a test module would desync the matcher
//! (known limit). `crates/benchdiff-core/tests/**` is not scanned.

use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN: [&str; 4] = ["std::fs", "std::net", "std::process", "std::env"];

/// Replaces every `#[cfg(test)]` item with blank text, keeping newlines so line numbers survive.
fn strip_cfg_test(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(pos) = rest.find("#[cfg(test)]") {
        out.push_str(&rest[..pos]);
        let item = &rest[pos..];
        let end = item_end(item);
        out.extend(item[..end].chars().map(|c| if c == '\n' { '\n' } else { ' ' }));
        rest = &item[end..];
    }
    out.push_str(rest);
    out
}

/// Byte length of the item starting at `#[cfg(test)]`: through the matching `}` of its first brace
/// block, or through the first `;` when the item has no block (e.g. `use` or a `const`).
fn item_end(item: &str) -> usize {
    let mut depth = 0usize;
    for (i, c) in item.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return i + 1;
                }
            }
            ';' if depth == 0 => return i + 1,
            _ => {}
        }
    }
    item.len()
}

/// `(line, pattern)` for each forbidden use in production code.
fn violations(src: &str) -> Vec<(usize, &'static str)> {
    let mut found = Vec::new();
    for (idx, line) in strip_cfg_test(src).lines().enumerate() {
        for pat in FORBIDDEN {
            if line.contains(pat) {
                found.push((idx + 1, pat));
            }
        }
    }
    found
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read_dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn core_should_not_use_fs_net_process_env_when_sources_scanned() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    assert!(!files.is_empty(), "no sources found under {}", src.display());
    let mut report = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file).expect("read source");
        for (line, pat) in violations(&text) {
            report.push(format!("{}:{line}: {pat}", file.display()));
        }
    }
    assert!(report.is_empty(), "I/O in core production code:\n{}", report.join("\n"));
}

#[test]
fn core_purity_scan_should_ignore_std_fs_inside_cfg_test_modules_and_fail_outside_them() {
    // The two real files that read fixtures inside #[cfg(test)] modules must pass as they are.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/import");
    for name in ["mod.rs", "json_formats.rs"] {
        let text = fs::read_to_string(root.join(name)).expect("read source");
        assert!(text.contains("std::fs"), "{name} no longer uses std::fs in tests; update this test");
        assert_eq!(violations(&text), vec![], "{name}");
    }

    // The same call inside a test module passes; outside it, the scan reports file-relative line.
    let in_test = "fn a() {}\n#[cfg(test)]\nmod tests {\n    fn t() { std::fs::read_to_string(\"x\"); }\n}\n";
    assert_eq!(violations(in_test), vec![]);
    let outside = "#[cfg(test)]\nmod tests {\n    fn t() {}\n}\nfn b() {\n    let _ = std::fs::read_to_string(\"x\");\n}\n";
    assert_eq!(violations(outside), vec![(6, "std::fs")]);
    for pat in ["std::net", "std::process", "std::env"] {
        let planted = format!("fn c() {{ let _ = {pat}::x(); }}\n");
        assert_eq!(violations(&planted).len(), 1, "{pat}");
    }
}
