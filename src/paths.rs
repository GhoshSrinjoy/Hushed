//! File paths: which files are the user's code, and short names to show for them.

use std::path::{Component, Path};

/// Python's `os.path.normcase`: lower case with backslashes on Windows, unchanged elsewhere.
pub fn normcase(path: &str) -> String {
    if cfg!(windows) {
        path.replace('/', "\\").to_lowercase()
    } else {
        path.to_owned()
    }
}

/// `path` made absolute against the current directory, like `os.path.abspath`.
pub fn absolute(path: &str) -> String {
    std::path::absolute(path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_owned())
}

fn components(path: &str) -> Vec<String> {
    Path::new(path)
        .components()
        .filter(|c| !matches!(c, Component::CurDir))
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect()
}

fn as_prefix(dir: &str) -> String {
    let mut p = normcase(&absolute(dir));
    if !p.ends_with(std::path::MAIN_SEPARATOR) {
        p.push(std::path::MAIN_SEPARATOR);
    }
    p
}

/// The folders whose code is watched (`mine`) and the ones that never are (`skip`: the standard
/// library, installed packages and hushed itself).
#[derive(Clone)]
pub struct Roots {
    mine: Vec<String>,
    skip: Vec<String>,
}

/// A folder as a path prefix, under its own name and, if it differs, its real one. On Unix Python
/// resolves symlinks in the script's folder for `sys.path[0]`, so the modules a script imports can
/// carry a path the script itself doesn't (`/var/...` and `/private/var/...` on macOS).
fn as_prefixes(dir: &str) -> Vec<String> {
    let mut prefixes = vec![as_prefix(dir)];
    if cfg!(unix) {
        if let Ok(real) = std::fs::canonicalize(dir) {
            let real = as_prefix(&real.to_string_lossy());
            if !prefixes.contains(&real) {
                prefixes.push(real);
            }
        }
    }
    prefixes
}

impl Roots {
    pub fn new(mine: &[String], skip: &[String]) -> Self {
        Roots {
            mine: mine.iter().flat_map(|d| as_prefixes(d)).collect(),
            skip: skip.iter().flat_map(|d| as_prefixes(d)).collect(),
        }
    }

    pub fn is_mine(&self, filename: &str) -> bool {
        let path = normcase(filename);
        self.mine.iter().any(|r| path.starts_with(r.as_str()))
            && !self.skip.iter().any(|s| path.starts_with(s.as_str()))
            && !path.contains("site-packages")
            && !path.contains("dist-packages")
    }

    /// The path relative to the first watched or library folder that holds it, else the file name.
    pub fn display(&self, filename: &str) -> String {
        let parts = components(&absolute(filename));
        for root in self.mine.iter().chain(&self.skip) {
            let root = components(root);
            if parts.len() > root.len() && parts.iter().zip(&root).all(|(a, b)| normcase(a) == normcase(b)) {
                return parts[root.len()..].join("/");
            }
        }
        Path::new(filename)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| filename.to_owned())
    }
}
