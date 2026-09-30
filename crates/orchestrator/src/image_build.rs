//! Builds a Docker build-context tar in memory, respecting `.dockerignore`
//! at the context root. Deliberately reduced from the real `.dockerignore`
//! spec (no anchoring via a leading `/`, no negation with `!`) — this
//! repo's actual `.dockerignore` (`target/`, `.fastembed_cache/`, `.git/`,
//! `.github/`, `*.md`) never needs either, and a match-any-component
//! rule is honest about what it does rather than a partial reimplementation
//! of the full spec pretending to be complete.

use std::fs;
use std::io;
use std::path::Path;

/// A single path component matches a pattern either exactly, or — if the
/// pattern starts with `*` — by suffix (covers `*.md`, the only glob this
/// repo's `.dockerignore` actually uses).
fn component_matches(component: &str, pattern: &str) -> bool {
    match pattern.strip_prefix('*') {
        Some(suffix) => component.ends_with(suffix),
        None => component == pattern,
    }
}

/// True if any component of `rel_path` (a path relative to the build
/// context root) matches any pattern — matching at any depth, the same
/// behavior a pattern with no embedded `/` has in real `.dockerignore`.
fn is_ignored(rel_path: &Path, patterns: &[String]) -> bool {
    rel_path.components().any(|component| {
        let component = component.as_os_str().to_string_lossy();
        patterns
            .iter()
            .any(|p| component_matches(&component, p.strip_suffix('/').unwrap_or(p)))
    })
}

/// Reads `.dockerignore` at the context root — comments (`#`) and blank
/// lines dropped, everything else kept as a raw pattern for `is_ignored`.
/// No file at all (rather than an empty one) means "nothing excluded", not
/// an error — most build contexts don't have one.
fn read_dockerignore(context_dir: &Path) -> Vec<String> {
    let Ok(content) = fs::read_to_string(context_dir.join(".dockerignore")) else {
        return Vec::new();
    };
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

fn add_dir_to_tar<W: io::Write>(
    builder: &mut tar::Builder<W>,
    context_dir: &Path,
    current_dir: &Path,
    patterns: &[String],
) -> io::Result<()> {
    for entry in fs::read_dir(current_dir)? {
        let entry = entry?;
        let path = entry.path();
        let rel_path = path
            .strip_prefix(context_dir)
            .expect("walked entry must be under context_dir");

        if is_ignored(rel_path, patterns) {
            continue;
        }

        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            add_dir_to_tar(builder, context_dir, &path, patterns)?;
        } else if file_type.is_file() {
            let mut file = fs::File::open(&path)?;
            builder.append_file(rel_path, &mut file)?;
        }
        // Symlinks: skipped — none exist in the parts of this repo a build
        // context needs (crates/, docker/, Cargo.{toml,lock}, vendor/).
    }
    Ok(())
}

/// Builds the full tar Docker's `/build` endpoint expects, in memory —
/// proportionate for this workspace's context size, no temp file needed.
pub fn build_context_tar(context_dir: &Path) -> io::Result<Vec<u8>> {
    let patterns = read_dockerignore(context_dir);
    let mut builder = tar::Builder::new(Vec::new());
    add_dir_to_tar(&mut builder, context_dir, context_dir, &patterns)?;
    builder.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn patterns() -> Vec<String> {
        vec![
            "target/".to_string(),
            ".fastembed_cache/".to_string(),
            ".git/".to_string(),
            ".github/".to_string(),
            "*.md".to_string(),
        ]
    }

    #[test]
    fn ignores_target_directory_at_any_depth() {
        assert!(is_ignored(
            &PathBuf::from("target/debug/kernel"),
            &patterns()
        ));
    }

    #[test]
    fn ignores_markdown_files_anywhere() {
        assert!(is_ignored(
            &PathBuf::from("docs/interfaces/kernel-auth.md"),
            &patterns()
        ));
        assert!(is_ignored(&PathBuf::from("CLAUDE.md"), &patterns()));
    }

    #[test]
    fn ignores_files_below_an_ignored_directory() {
        assert!(is_ignored(
            &PathBuf::from(".github/workflows/ci.yml"),
            &patterns()
        ));
    }

    #[test]
    fn does_not_ignore_real_source_files() {
        assert!(!is_ignored(
            &PathBuf::from("crates/kernel/src/main.rs"),
            &patterns()
        ));
        assert!(!is_ignored(&PathBuf::from("Cargo.toml"), &patterns()));
        assert!(!is_ignored(
            &PathBuf::from("docker/kernel.Dockerfile"),
            &patterns()
        ));
    }

    #[test]
    fn a_file_merely_containing_md_is_not_mistaken_for_dot_md() {
        // "*.md" should match the suffix ".md", not the substring "md"
        // anywhere in the name.
        assert!(!is_ignored(&PathBuf::from("Cargo.mdx"), &patterns()));
    }
}
