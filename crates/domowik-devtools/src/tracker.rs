//! Checks the issue files against the tracker index (ED-0078).
//!
//! Seven inconsistencies fail the check: a status line that disagrees with
//! `issues/README.md`, a duplicate `NNNN` prefix across `issues/`, `done/`,
//! `not-needed/` and `dropped/`, a missing status line, a status that is not
//! one of the spellings the workflow defines, a terminal status whose file is
//! not in that status's directory, an index row whose link does not name the
//! file's actual location, and an index row naming a file that does not exist.
//!
//! Note the asymmetry, which is deliberate and inherited: an index row with no
//! file is an ERROR, but a file with no index row is not. Adding an issue and
//! forgetting the index is caught by review and by the `Move completed/dropped
//! files and update this index in the same commit` rule, not here.
//!
//! The vocabulary check is deliberately EXACT rather than case- or
//! whitespace-insensitive. `Not needed` is a terminal status with a written
//! rationale behind it (ED-0102); `not needed` and `Not Needed` are typos, and
//! a tracker that silently accepts three spellings of one state is back to the
//! condition ED-0095 closed, where the status column stopped meaning anything.
//! Only the FILE status is validated: the index must already agree with it, so
//! a misspelling in a row surfaces as a mismatch against a checked value.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Statuses an issue may declare while work on it is still possible.
///
/// Spelled exactly as `issues/README.md` defines them.
const ACTIVE_STATUSES: [&str; 4] = [
    "Open",
    "In Progress",
    "In Progress (partial scope)",
    "Blocked",
];

/// Statuses that close an issue.
///
/// `Not needed` is the terminal status for scope that is obsolete or
/// superseded, and it requires a written rationale in the issue — the checker
/// cannot judge a rationale, but the status exists so the judgement has
/// somewhere to live other than a deleted acceptance criterion (ED-0102).
///
/// `Dropped` is RETAINED for historical files only. Nothing new should take
/// it; it stays accepted because rewriting closed history to match a newer
/// vocabulary would destroy the record of what was actually decided when.
const TERMINAL_STATUSES: [&str; 3] = ["Done", "Not needed", "Dropped"];

/// The directories an issue file may live in.
///
/// Order does not decide anything. A later directory's entry does overwrite an
/// earlier one in the `found` map, but an id appearing twice is rejected before
/// that map is used, so which entry survived is never observable.
const ISSUE_DIRS: [&str; 4] = [".", "done", "not-needed", "dropped"];

/// The directory a terminally-statused issue's file belongs in.
///
/// `None` for an active status, whose file belongs in the root.
fn terminal_dir(status: &str) -> Option<&'static str> {
    match status {
        "Done" => Some("done"),
        "Not needed" => Some("not-needed"),
        "Dropped" => Some("dropped"),
        _ => None,
    }
}

/// Whether `status` is one of the spellings the workflow defines.
fn is_known_status(status: &str) -> bool {
    ACTIVE_STATUSES.contains(&status) || TERMINAL_STATUSES.contains(&status)
}

/// Every valid spelling, in the order a diagnostic should list them.
fn known_statuses() -> Vec<&'static str> {
    ACTIVE_STATUSES
        .iter()
        .chain(TERMINAL_STATUSES.iter())
        .copied()
        .collect()
}

/// One issue file's identity and declared status.
#[derive(Debug, Clone)]
struct Issue {
    path: PathBuf,
    /// The same file as `path`, spelled the way an index row spells it:
    /// relative to `issues/`, forward slashes, no leading `./`.
    relative: String,
    status: String,
}

/// One `| [NNNN](path) | ... | Status |` row from the index.
#[derive(Debug, Clone)]
struct IndexRow {
    id: String,
    path: String,
    status: String,
    line: usize,
}

/// Every way the tracker can be inconsistent, as a printable report.
#[derive(Debug)]
pub struct TrackerError(String);

impl fmt::Display for TrackerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for TrackerError {}

/// Runs every consistency check over an `issues/` directory.
pub fn check_tracker(issues_dir: &Path) -> Result<(), TrackerError> {
    let issues = scan_issue_files(issues_dir)?;
    let rows = parse_index(&issues_dir.join("README.md"))?;
    check_status_agreement(&issues, &rows)
}

/// Reads the `- **Status:** ...` value from an issue file.
///
/// The FIRST match wins, as in the Go original: a status line appearing twice
/// is a malformed file rather than a second opinion, and taking the first
/// keeps the header authoritative over anything quoted further down.
fn extract_status(source: &str) -> Option<String> {
    for line in source.lines() {
        let rest = line.trim_start();
        let Some(rest) = rest.strip_prefix('-') else {
            continue;
        };
        if !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix("**Status:**") else {
            continue;
        };
        if !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let value = rest.trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

/// `NNNN` from a `NNNN-slug.md` filename, if it has that shape.
fn issue_id(file_name: &str) -> Option<String> {
    let stem = file_name.strip_suffix(".md")?;
    let (id, rest) = stem.split_at_checked(4)?;
    if !id.chars().all(|c| c.is_ascii_digit()) || !rest.starts_with('-') {
        return None;
    }
    Some(id.to_string())
}

fn scan_issue_files(root: &Path) -> Result<BTreeMap<String, Issue>, TrackerError> {
    let mut found: BTreeMap<String, Issue> = BTreeMap::new();
    let mut all_paths: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();

    for dir in ISSUE_DIRS {
        // `root.join(".")` would render as `issues/./0001-x.md`. Go's
        // filepath.Join normalises that away, and these paths are printed in
        // diagnostics, so the root is used directly for the current directory.
        let full = if dir == "." {
            root.to_path_buf()
        } else {
            root.join(dir)
        };
        let entries = match fs::read_dir(&full) {
            Ok(entries) => entries,
            // The terminal directories may not exist yet; a missing root is
            // the caller's problem and surfaces as an empty scan plus a README
            // read failure.
            Err(err) if err.kind() == std::io::ErrorKind::NotFound && dir != "." => continue,
            Err(err) => {
                return Err(TrackerError(format!("reading {}: {err}", full.display())));
            }
        };

        let mut names = Vec::new();
        for entry in entries {
            let entry =
                entry.map_err(|err| TrackerError(format!("reading {}: {err}", full.display())))?;
            names.push(entry.file_name());
        }
        names.sort();

        for name in names {
            let name = name.to_string_lossy().into_owned();
            if name == "README.md" || name == "TEMPLATE.md" || !name.ends_with(".md") {
                continue;
            }
            let Some(id) = issue_id(&name) else {
                continue;
            };
            let path = full.join(&name);
            if path.is_dir() {
                continue;
            }
            let source = fs::read_to_string(&path)
                .map_err(|err| TrackerError(format!("opening {}: {err}", path.display())))?;
            let Some(status) = extract_status(&source) else {
                return Err(TrackerError(format!(
                    "{}: missing Status line",
                    path.display()
                )));
            };
            if !is_known_status(&status) {
                return Err(TrackerError(format!(
                    "{}: unknown status {status:?}; expected one of {}",
                    path.display(),
                    known_statuses()
                        .iter()
                        .map(|s| format!("{s:?}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
            let relative = if dir == "." {
                name.clone()
            } else {
                format!("{dir}/{name}")
            };
            let expected = terminal_dir(&status).unwrap_or(".");
            if dir != expected {
                let (was, want) = (describe_dir(dir), describe_dir(expected));
                return Err(TrackerError(format!(
                    "{}: status {status:?} belongs in {want}, but the file is in {was}",
                    path.display()
                )));
            }
            all_paths.entry(id.clone()).or_default().push(path.clone());
            found.insert(
                id,
                Issue {
                    path,
                    relative,
                    status,
                },
            );
        }
    }

    let mut duplicates: Vec<String> = all_paths
        .iter()
        .filter(|(_, paths)| paths.len() > 1)
        .map(|(id, paths)| {
            let mut sorted: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
            sorted.sort();
            format!("  {id}: {}", sorted.join(", "))
        })
        .collect();
    if !duplicates.is_empty() {
        duplicates.sort();
        return Err(TrackerError(format!(
            "duplicate issue IDs:\n{}",
            duplicates.join("\n")
        )));
    }

    Ok(found)
}

/// Parses one index row, or `None` if the line is not one.
///
/// Whitespace around the id and status cells is REQUIRED, matching the Go
/// regex it replaces. Relaxing it would start matching rows the previous
/// checker ignored, turning prose that happens to contain pipes into orphan
/// errors.
fn parse_index_row(line: &str) -> Option<(String, String, String)> {
    if !line.starts_with('|') || !line.ends_with('|') || line.len() < 2 {
        return None;
    }
    let cells: Vec<&str> = line[1..line.len() - 1].split('|').collect();
    // The id cell, at least one middle cell, and the status cell: the Go
    // pattern needs a `|` between the link and the final field, so a two-cell
    // row never matched.
    if cells.len() < 3 {
        return None;
    }

    let id_cell = cells[0];
    if !starts_and_ends_with_space(id_cell) {
        return None;
    }
    let link = id_cell.trim();
    let rest = link.strip_prefix('[')?;
    let (id, rest) = rest.split_once(']')?;
    if id.len() != 4 || !id.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let target = rest.strip_prefix('(')?.strip_suffix(')')?;
    if target.contains(')') {
        return None;
    }

    let status_cell = cells[cells.len() - 1];
    if !starts_and_ends_with_space(status_cell) {
        return None;
    }
    let status = status_cell.trim();
    if status.is_empty() {
        return None;
    }

    Some((id.to_string(), target.to_string(), status.to_string()))
}

/// `issues/` for the root, `issues/<dir>/` otherwise — so a diagnostic reads
/// as a path rather than as a bare `.`.
fn describe_dir(dir: &str) -> String {
    if dir == "." {
        "issues/".to_string()
    } else {
        format!("issues/{dir}/")
    }
}

fn starts_and_ends_with_space(cell: &str) -> bool {
    let mut chars = cell.chars();
    let first = chars.next();
    let last = chars.next_back().or(first);
    matches!((first, last), (Some(f), Some(l)) if f.is_whitespace() && l.is_whitespace())
}

fn parse_index(path: &Path) -> Result<Vec<IndexRow>, TrackerError> {
    let source = fs::read_to_string(path)
        .map_err(|err| TrackerError(format!("opening {}: {err}", path.display())))?;
    Ok(source
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            parse_index_row(line).map(|(id, path, status)| IndexRow {
                id,
                path,
                status,
                line: index + 1,
            })
        })
        .collect())
}

fn check_status_agreement(
    issues: &BTreeMap<String, Issue>,
    rows: &[IndexRow],
) -> Result<(), TrackerError> {
    let mut errors = Vec::new();

    for row in rows {
        let Some(issue) = issues.get(&row.id) else {
            errors.push(format!(
                "issues/README.md:{}: orphan index row for {} ({}) — no corresponding file",
                row.line, row.id, row.path
            ));
            continue;
        };
        if issue.status != row.status {
            errors.push(format!(
                "status mismatch for {}:\n  file {}: {:?}\n  index issues/README.md:{}: {:?}",
                row.id,
                issue.path.display(),
                issue.status,
                row.line,
                row.status
            ));
        }
        // A row whose link no longer points at the file. The id still resolves,
        // so this is NOT an orphan and the old checker saw nothing — it is the
        // shape a closure takes when the file moves to `done/` and the index
        // keeps the root path, leaving a link that 404s on the forge.
        if issue.relative != row.path {
            errors.push(format!(
                "index issues/README.md:{} links {} to {:?}, but the file is at {:?}",
                row.line, row.id, row.path, issue.relative
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(TrackerError(format!(
            "tracker inconsistencies:\n{}",
            errors.join("\n")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn valid_tracker(root: &Path) {
        write(
            &root.join("0001-test.md"),
            "# 0001: Test\n\n- **Status:** Open\n",
        );
        write(
            &root.join("0002-other.md"),
            "# 0002: Other\n\n- **Status:** In Progress (partial scope)\n",
        );
        write(
            &root.join("README.md"),
            "# Tracker\n\n\
             | Issue | Title | Status |\n\
             |---|---|---|\n\
             | [0001](0001-test.md) | Test | Open |\n\
             | [0002](0002-other.md) | Other | In Progress (partial scope) |\n",
        );
    }

    #[test]
    fn a_consistent_tracker_passes() {
        let dir = tempfile::tempdir().unwrap();
        valid_tracker(dir.path());
        check_tracker(dir.path()).unwrap();
    }

    #[test]
    fn a_status_disagreement_fails() {
        let dir = tempfile::tempdir().unwrap();
        valid_tracker(dir.path());
        let readme = dir.path().join("README.md");
        let content = fs::read_to_string(&readme)
            .unwrap()
            .replace("| Test | Open |", "| Test | Done |");
        write(&readme, &content);

        let err = check_tracker(dir.path()).unwrap_err().to_string();
        assert!(err.contains("status mismatch"), "{err}");
    }

    #[test]
    fn a_duplicate_prefix_fails() {
        let dir = tempfile::tempdir().unwrap();
        valid_tracker(dir.path());
        write(
            &dir.path().join("done").join("0001-duplicate.md"),
            "# 0001: Duplicate\n\n- **Status:** Done\n",
        );

        let err = check_tracker(dir.path()).unwrap_err().to_string();
        assert!(err.contains("duplicate issue IDs"), "{err}");
    }

    #[test]
    fn a_missing_status_line_fails() {
        let dir = tempfile::tempdir().unwrap();
        valid_tracker(dir.path());
        write(&dir.path().join("0003-nostatus.md"), "# 0003: No status\n");

        let err = check_tracker(dir.path()).unwrap_err().to_string();
        assert!(err.contains("missing Status line"), "{err}");
    }

    #[test]
    fn an_orphan_index_row_fails() {
        let dir = tempfile::tempdir().unwrap();
        valid_tracker(dir.path());
        let readme = dir.path().join("README.md");
        let mut content = fs::read_to_string(&readme).unwrap();
        content.push_str("| [0099](0099-ghost.md) | Ghost | Open |\n");
        write(&readme, &content);

        let err = check_tracker(dir.path()).unwrap_err().to_string();
        assert!(err.contains("orphan index row"), "{err}");
    }

    #[test]
    fn issues_in_every_terminal_directory_are_found() {
        let dir = tempfile::tempdir().unwrap();
        valid_tracker(dir.path());
        write(
            &dir.path().join("done").join("0003-finished.md"),
            "# 0003\n\n- **Status:** Done\n",
        );
        write(
            &dir.path().join("not-needed").join("0005-superseded.md"),
            "# 0005\n\n- **Status:** Not needed\n",
        );
        write(
            &dir.path().join("dropped").join("0004-abandoned.md"),
            "# 0004\n\n- **Status:** Dropped\n",
        );
        let readme = dir.path().join("README.md");
        let mut content = fs::read_to_string(&readme).unwrap();
        content.push_str("| [0003](done/0003-finished.md) | Finished | Done |\n");
        content.push_str("| [0005](not-needed/0005-superseded.md) | Superseded | Not needed |\n");
        content.push_str("| [0004](dropped/0004-abandoned.md) | Abandoned | Dropped |\n");
        write(&readme, &content);

        check_tracker(dir.path()).unwrap();
    }

    /// A duplicate id must be caught in `not-needed/` too, or the new
    /// directory would be the one place an issue could be filed twice.
    #[test]
    fn a_duplicate_prefix_in_not_needed_fails() {
        let dir = tempfile::tempdir().unwrap();
        valid_tracker(dir.path());
        write(
            &dir.path().join("not-needed").join("0001-duplicate.md"),
            "# 0001: Duplicate\n\n- **Status:** Not needed\n",
        );

        let err = check_tracker(dir.path()).unwrap_err().to_string();
        assert!(err.contains("duplicate issue IDs"), "{err}");
    }

    /// `Not needed` is one spelling, not a family of them. Each of these
    /// passed the old checker, because it only compared file against index.
    #[test]
    fn a_near_miss_spelling_of_not_needed_fails() {
        for wrong in [
            "not needed",
            "Not Needed",
            "Not-needed",
            "NOT NEEDED",
            "Not needed.",
        ] {
            let dir = tempfile::tempdir().unwrap();
            valid_tracker(dir.path());
            write(
                &dir.path().join("not-needed").join("0005-superseded.md"),
                &format!("# 0005\n\n- **Status:** {wrong}\n"),
            );
            let readme = dir.path().join("README.md");
            let mut content = fs::read_to_string(&readme).unwrap();
            content.push_str(&format!(
                "| [0005](not-needed/0005-superseded.md) | Superseded | {wrong} |\n"
            ));
            write(&readme, &content);

            let err = check_tracker(dir.path()).unwrap_err().to_string();
            assert!(
                err.contains("unknown status"),
                "{wrong:?} was accepted: {err}"
            );
        }
    }

    #[test]
    fn every_workflow_status_is_accepted_in_its_own_directory() {
        for status in known_statuses() {
            let dir = tempfile::tempdir().unwrap();
            let relative = match terminal_dir(status) {
                Some(sub) => format!("{sub}/0001-test.md"),
                None => "0001-test.md".to_string(),
            };
            write(
                &dir.path().join(&relative),
                &format!("# 0001: Test\n\n- **Status:** {status}\n"),
            );
            write(
                &dir.path().join("README.md"),
                &format!("| [0001]({relative}) | Test | {status} |\n"),
            );
            check_tracker(dir.path()).expect(status);
        }
    }

    /// Issues 0015 and 0037 were `Done` and sat in `issues/` for eleven days.
    /// The old checker was satisfied: the file agreed with the index, and both
    /// said `Done`. Nothing compared either against where the file actually
    /// was.
    #[test]
    fn a_terminal_issue_left_in_the_root_fails() {
        for (status, want) in [("Done", "done"), ("Not needed", "not-needed")] {
            let dir = tempfile::tempdir().unwrap();
            valid_tracker(dir.path());
            write(
                &dir.path().join("0003-closed.md"),
                &format!("# 0003\n\n- **Status:** {status}\n"),
            );
            let readme = dir.path().join("README.md");
            let mut content = fs::read_to_string(&readme).unwrap();
            content.push_str(&format!("| [0003](0003-closed.md) | Closed | {status} |\n"));
            write(&readme, &content);

            let err = check_tracker(dir.path()).unwrap_err().to_string();
            assert!(err.contains("belongs in"), "{status}: {err}");
            assert!(err.contains(want), "{status}: {err}");
        }
    }

    /// The converse: an active issue filed away in a terminal directory.
    #[test]
    fn an_active_issue_in_a_terminal_directory_fails() {
        let dir = tempfile::tempdir().unwrap();
        valid_tracker(dir.path());
        write(
            &dir.path().join("done").join("0003-open.md"),
            "# 0003\n\n- **Status:** Blocked\n",
        );
        let readme = dir.path().join("README.md");
        let mut content = fs::read_to_string(&readme).unwrap();
        content.push_str("| [0003](done/0003-open.md) | Open one | Blocked |\n");
        write(&readme, &content);

        let err = check_tracker(dir.path()).unwrap_err().to_string();
        assert!(err.contains("belongs in issues/"), "{err}");
    }

    /// Moving a file and forgetting the index leaves a link that resolves to
    /// nothing on the forge. The id still matches, so this is not an orphan.
    #[test]
    fn an_index_row_naming_the_old_location_fails() {
        let dir = tempfile::tempdir().unwrap();
        valid_tracker(dir.path());
        write(
            &dir.path().join("done").join("0003-moved.md"),
            "# 0003\n\n- **Status:** Done\n",
        );
        let readme = dir.path().join("README.md");
        let mut content = fs::read_to_string(&readme).unwrap();
        content.push_str("| [0003](0003-moved.md) | Moved | Done |\n");
        write(&readme, &content);

        let err = check_tracker(dir.path()).unwrap_err().to_string();
        assert!(err.contains("links 0003"), "{err}");
        assert!(err.contains("done/0003-moved.md"), "{err}");
        assert!(
            !err.contains("orphan"),
            "reported as an orphan instead: {err}"
        );
    }

    /// An invented status fails even when the index agrees with it, which is
    /// the case the pre-vocabulary checker could not see.
    #[test]
    fn an_invented_status_fails_even_when_the_index_agrees() {
        let dir = tempfile::tempdir().unwrap();
        valid_tracker(dir.path());
        write(
            &dir.path().join("0003-invented.md"),
            "# 0003\n\n- **Status:** Mostly Done\n",
        );
        let readme = dir.path().join("README.md");
        let mut content = fs::read_to_string(&readme).unwrap();
        content.push_str("| [0003](0003-invented.md) | Invented | Mostly Done |\n");
        write(&readme, &content);

        let err = check_tracker(dir.path()).unwrap_err().to_string();
        assert!(err.contains("unknown status"), "{err}");
        assert!(
            err.contains("\"Not needed\""),
            "diagnostic omits the vocabulary: {err}"
        );
    }

    /// The parenthetical in `In Progress (partial scope)` must survive intact.
    /// A status regex that stopped at the first space would compare
    /// `In` against `In Progress (partial scope)` and fail every row.
    #[test]
    fn a_parenthetical_status_round_trips() {
        assert_eq!(
            extract_status("- **Status:** In Progress (partial scope)\n").unwrap(),
            "In Progress (partial scope)"
        );
    }

    #[test]
    fn every_mismatch_is_reported_not_just_the_first() {
        let dir = tempfile::tempdir().unwrap();
        valid_tracker(dir.path());
        let readme = dir.path().join("README.md");
        let content = fs::read_to_string(&readme)
            .unwrap()
            .replace("| Test | Open |", "| Test | Done |")
            .replace(
                "| Other | In Progress (partial scope) |",
                "| Other | Blocked |",
            );
        write(&readme, &content);

        let err = check_tracker(dir.path()).unwrap_err().to_string();
        assert_eq!(err.matches("status mismatch").count(), 2, "{err}");
    }

    #[test]
    fn a_four_column_row_takes_the_last_cell_as_status() {
        let row = parse_index_row("| [0024](0024-spike.md) | 2 | Bridge spike | Open |").unwrap();
        assert_eq!(row, ("0024".into(), "0024-spike.md".into(), "Open".into()));
    }

    /// Prose and separator lines must not be mistaken for rows.
    #[test]
    fn non_rows_are_ignored() {
        assert!(parse_index_row("|---|---|---|").is_none());
        assert!(parse_index_row("| Issue | Title | Status |").is_none());
        assert!(parse_index_row("some prose").is_none());
        assert!(parse_index_row("| [0001](a.md) | Open |").is_none());
    }

    #[test]
    fn a_filename_without_a_four_digit_prefix_is_not_an_issue() {
        assert_eq!(issue_id("0001-test.md").as_deref(), Some("0001"));
        assert!(issue_id("AGENTS.md").is_none());
        assert!(issue_id("012-short.md").is_none());
        assert!(issue_id("0001test.md").is_none());
    }
}
