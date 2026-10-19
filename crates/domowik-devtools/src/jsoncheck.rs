//! Syntax-validates every fenced ```json block in committed Markdown.
//!
//! Scope, deliberately narrow: PARSING ONLY. Parse success establishes
//! syntactic validity and nothing else; it implies NOTHING about schema
//! conformance of the documented examples. Schema and conformance checking
//! belong to issues 0028 (corpus) and 0029 (enforcement).

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// A fence that did not parse, or a directory that could not be walked.
#[derive(Debug)]
pub struct CheckError {
    /// The file the problem was found in.
    pub path: PathBuf,
    /// 1-based line of the fence's opening ```json, or 0 for a whole-file
    /// problem such as an unreadable directory.
    pub line: usize,
    pub message: String,
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.path.display(), self.line, self.message)
    }
}

/// Counts and validates the ```json fences in one Markdown source.
///
/// Returns the number of fences that parsed, or the first failure. Counting
/// stops at the first failure on purpose: the count is only ever used to prove
/// the checker found SOMETHING to check, and a partial count from a failing
/// run would be a misleading number to print beside an error.
pub fn check_markdown(path: &Path, source: &str) -> Result<usize, CheckError> {
    let mut count = 0usize;
    let mut fence: Option<(usize, Vec<&str>)> = None;

    for (index, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        match fence.as_mut() {
            None if trimmed == "```json" => fence = Some((index + 1, Vec::new())),
            None => {}
            Some((start, buffer)) if trimmed == "```" => {
                let start = *start;
                let text = buffer.join("\n");
                serde_json::from_str::<serde_json::Value>(&text).map_err(|err| CheckError {
                    path: path.to_path_buf(),
                    line: start,
                    message: format!("invalid JSON: {err}"),
                })?;
                count += 1;
                fence = None;
            }
            Some((_, buffer)) => buffer.push(line),
        }
    }

    match fence {
        Some((start, _)) => Err(CheckError {
            path: path.to_path_buf(),
            line: start,
            message: "unterminated json fence".to_string(),
        }),
        None => Ok(count),
    }
}

/// Walks `root` and checks every `.md` file beneath it.
///
/// Directory entries are visited in sorted order so a failure is reported at
/// the same file on every host; `read_dir` order is filesystem-defined and
/// differs between macOS and Linux.
pub fn check_tree(root: &Path) -> Result<usize, CheckError> {
    let mut total = 0usize;
    let mut stack = vec![root.to_path_buf()];

    while let Some(current) = stack.pop() {
        let metadata = fs::metadata(&current).map_err(|err| CheckError {
            path: current.clone(),
            line: 0,
            message: err.to_string(),
        })?;

        if metadata.is_dir() {
            let mut children = Vec::new();
            let entries = fs::read_dir(&current).map_err(|err| CheckError {
                path: current.clone(),
                line: 0,
                message: err.to_string(),
            })?;
            for entry in entries {
                let entry = entry.map_err(|err| CheckError {
                    path: current.clone(),
                    line: 0,
                    message: err.to_string(),
                })?;
                children.push(entry.path());
            }
            // Reversed, because the stack pops from the end.
            children.sort();
            children.reverse();
            stack.extend(children);
            continue;
        }

        if current.extension().is_some_and(|ext| ext == "md") {
            let source = fs::read_to_string(&current).map_err(|err| CheckError {
                path: current.clone(),
                line: 0,
                message: err.to_string(),
            })?;
            total += check_markdown(&current, &source)?;
        }
    }

    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path() -> PathBuf {
        PathBuf::from("doc.md")
    }

    #[test]
    fn a_valid_fence_counts() {
        let source = "text\n```json\n{\"a\": 1}\n```\nmore\n";
        assert_eq!(check_markdown(&path(), source).unwrap(), 1);
    }

    #[test]
    fn several_fences_all_count() {
        let source = "```json\n[]\n```\n\n```json\n{}\n```\n";
        assert_eq!(check_markdown(&path(), source).unwrap(), 2);
    }

    #[test]
    fn a_file_with_no_fences_is_not_an_error() {
        assert_eq!(check_markdown(&path(), "# Title\n\nprose\n").unwrap(), 0);
    }

    /// A fence in another language must not be parsed as JSON. Without this,
    /// ```` ```rust ```` blocks would fail the check on their first brace.
    #[test]
    fn a_non_json_fence_is_ignored() {
        let source = "```rust\nfn main() {}\n```\n";
        assert_eq!(check_markdown(&path(), source).unwrap(), 0);
    }

    /// The reported line is the opening ```json, NOT the first content line.
    /// `serde_json`'s own message carries a line number relative to the fence
    /// body, so the two compose into a real location; shifting this one by a
    /// line would make them disagree by one for every fence.
    #[test]
    fn invalid_json_reports_the_opening_fence_line() {
        let source = "line one\n```json\n{oops}\n```\n";
        let err = check_markdown(&path(), source).unwrap_err();
        assert_eq!(err.line, 2);
        assert!(err.message.contains("invalid JSON"), "{}", err.message);
    }

    #[test]
    fn an_unterminated_fence_is_an_error() {
        let source = "```json\n{\"a\": 1}\n";
        let err = check_markdown(&path(), source).unwrap_err();
        assert!(err.message.contains("unterminated"), "{}", err.message);
    }

    /// An indented fence still opens and closes. The Go original trimmed each
    /// line before comparing, and dropping that would silently stop checking
    /// every fence nested inside a list item.
    #[test]
    fn an_indented_fence_is_still_checked() {
        let source = "- item\n  ```json\n  {\"a\": 1}\n  ```\n";
        assert_eq!(check_markdown(&path(), source).unwrap(), 1);
    }

    /// A bare scalar is valid JSON and must be accepted; `serde_json` parses
    /// one only because the value type is `Value` rather than a map.
    #[test]
    fn a_scalar_document_is_valid() {
        assert_eq!(check_markdown(&path(), "```json\n42\n```\n").unwrap(), 1);
    }

    #[test]
    fn a_tree_walk_finds_nested_markdown() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a").join("b");
        fs::create_dir_all(&nested).unwrap();
        fs::write(dir.path().join("top.md"), "```json\n1\n```\n").unwrap();
        fs::write(nested.join("deep.md"), "```json\n[2]\n```\n").unwrap();
        fs::write(
            nested.join("ignored.txt"),
            "```json\nnot json at all\n```\n",
        )
        .unwrap();

        assert_eq!(check_tree(dir.path()).unwrap(), 2);
    }
}
