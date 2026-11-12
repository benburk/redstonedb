//! The crate's modules below the web are layers, each using only those
//! before it: the file format at the bottom, then a build's inputs and
//! outputs, stacking, the simulator, the categories with their tests, and
//! recognizing a build; then what a stored build is, the database, and at
//! the top the path that takes a file in. A module's own unit tests may
//! reach higher, as for a fixture an upload prepares.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

/// The layers, lowest first, each a folder of `src` or a file.
const LAYERS: [&str; 18] = [
    "schem",
    "interface",
    "stack",
    "simulate",
    "category",
    "standard",
    "recognize",
    "works_from",
    "compare",
    "thumbnail",
    "build",
    "account",
    "search",
    "leaderboard",
    "external",
    "store",
    "players",
    "ingest",
];

/// Every Rust file under `dir`.
fn files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs")
            && path.file_name().is_some_and(|name| name != "tests.rs")
        {
            found.push(path);
        }
    }
}

/// The parts of a use tree at its top level, split at its commas.
fn parts(tree: &str) -> Vec<&str> {
    let (mut depth, mut start, mut parts) = (0_usize, 0, Vec::new());
    for (at, c) in tree.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&tree[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    parts.push(&tree[start..]);
    parts
}

/// The modules of the crate `source` names, as `crate::name` or in a
/// `crate::{name, ...}` group, its comments and unit tests left out.
fn named(source: &str) -> BTreeSet<String> {
    let code: String = source
        .split("#[cfg(test)]")
        .next()
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let word = |text: &str| -> String {
        text.trim()
            .chars()
            .take_while(|c| c.is_ascii_lowercase() || *c == '_')
            .collect()
    };
    let mut names = BTreeSet::new();
    for (at, _) in code.match_indices("crate::") {
        let rest = &code[at + "crate::".len()..];
        if let Some(group) = rest.strip_prefix('{') {
            let end = group.find(';').unwrap_or(group.len());
            let tree = group[..end].trim_end().trim_end_matches('}');
            names.extend(parts(tree).into_iter().map(word));
        } else {
            names.insert(word(rest));
        }
    }
    names.remove("");
    names
}

#[test]
fn each_layer_uses_only_the_layers_below_it() {
    let mut faults = Vec::new();
    for (place, layer) in LAYERS.iter().enumerate() {
        let allowed = &LAYERS[..=place];
        let mut found = Vec::new();
        let folder = Path::new("src").join(layer);
        if folder.is_dir() {
            files(&folder, &mut found);
        } else {
            found.push(folder.with_extension("rs"));
        }
        assert!(!found.is_empty(), "{layer} has no files");
        for path in found {
            let source = fs::read_to_string(&path).unwrap();
            for name in named(&source) {
                if !allowed.contains(&name.as_str()) {
                    faults.push(format!("{} uses {name}", path.display()));
                }
            }
        }
    }
    assert!(faults.is_empty(), "{}", faults.join("\n"));
}
