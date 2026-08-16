use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const MAX_COMMIT_SUBJECT_CHARS: usize = 72;
pub const MAX_COMMIT_MESSAGE_BYTES: usize = 4_096;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlanEntry {
    pub message: String,
    pub files: Vec<String>,
}

fn strip_fence(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with("```") && trimmed.ends_with("```") {
        let mut lines = trimmed.lines();
        lines.next();
        let mut body: Vec<&str> = lines.collect();
        body.pop();
        body.join("\n")
    } else {
        trimmed.to_owned()
    }
}

fn valid_commit_type(kind: &str) -> bool {
    matches!(
        kind,
        "feat"
            | "fix"
            | "docs"
            | "style"
            | "refactor"
            | "perf"
            | "test"
            | "build"
            | "ci"
            | "chore"
            | "revert"
    )
}

fn valid_scope(scope: &str) -> bool {
    !scope.is_empty()
        && scope.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | '/')
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitMessageError {
    Empty,
    MessageTooLong,
    UnsafeCharacter,
    TrailingWhitespace,
    SubjectTooLong,
    MissingSubjectSeparator,
    InvalidSummary,
    InvalidType,
    InvalidScopeSyntax,
    InvalidScope,
    MissingBodySeparator,
    EmptyBody,
    TrailerLikeBody,
}

impl fmt::Display for CommitMessageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "message is empty",
            Self::MessageTooLong => "message exceeds the 4096-byte limit",
            Self::UnsafeCharacter => {
                "message contains a control, bidirectional, or zero-width character"
            }
            Self::TrailingWhitespace => "message contains trailing whitespace",
            Self::SubjectTooLong => "subject exceeds 72 characters",
            Self::MissingSubjectSeparator => {
                "subject must match `type(scope): summary` or `type: summary`"
            }
            Self::InvalidSummary => {
                "summary must be non-empty and have no surrounding whitespace"
            }
            Self::InvalidType => {
                "type must be one of feat, fix, docs, style, refactor, perf, test, build, ci, chore, or revert"
            }
            Self::InvalidScopeSyntax => "scope must be closed with `)` before `: `",
            Self::InvalidScope => {
                "scope may contain only ASCII letters, digits, `-`, `_`, `.`, or `/`"
            }
            Self::MissingBodySeparator => {
                "body must be separated from the subject by exactly one blank line"
            }
            Self::EmptyBody => {
                "body must start immediately after exactly one blank line"
            }
            Self::TrailerLikeBody => {
                "body contains a trailer-like `Token: value` or `Token=value` line"
            }
        })
    }
}

fn validate_conventional_subject(
    subject: &str,
) -> std::result::Result<(), CommitMessageError> {
    let Some((prefix, summary)) = subject.split_once(": ") else {
        return Err(CommitMessageError::MissingSubjectSeparator);
    };
    if summary.is_empty() || summary.trim() != summary {
        return Err(CommitMessageError::InvalidSummary);
    }
    let prefix = prefix.strip_suffix('!').unwrap_or(prefix);
    if let Some((kind, scope)) = prefix.split_once('(') {
        if !valid_commit_type(kind) {
            return Err(CommitMessageError::InvalidType);
        }
        let Some(scope) = scope.strip_suffix(')') else {
            return Err(CommitMessageError::InvalidScopeSyntax);
        };
        if !valid_scope(scope) {
            return Err(CommitMessageError::InvalidScope);
        }
        Ok(())
    } else if valid_commit_type(prefix) {
        Ok(())
    } else {
        Err(CommitMessageError::InvalidType)
    }
}

fn unsafe_message_character(character: char) -> bool {
    (character.is_control() && character != '\n')
        || matches!(
            character,
            '\u{061c}'
                | '\u{200b}'
                | '\u{200c}'
                | '\u{200d}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{202a}'
                | '\u{202b}'
                | '\u{202c}'
                | '\u{202d}'
                | '\u{202e}'
                | '\u{2060}'
                | '\u{2066}'
                | '\u{2067}'
                | '\u{2068}'
                | '\u{2069}'
                | '\u{feff}'
        )
}

pub fn terminal_safe_path(path: &str) -> String {
    let mut rendered = String::with_capacity(path.len());
    for character in path.chars() {
        if character == '\n' || unsafe_message_character(character) {
            rendered.extend(character.escape_default());
        } else {
            rendered.push(character);
        }
    }
    rendered
}

fn terminal_safe_paths(paths: &[&str]) -> String {
    paths
        .iter()
        .map(|path| terminal_safe_path(path))
        .collect::<Vec<_>>()
        .join(", ")
}

fn trailer_like_line(line: &str) -> bool {
    let line = line.trim_start();
    let separator = line
        .char_indices()
        .find_map(|(index, character)| matches!(character, ':' | '=').then_some(index));
    let Some(separator) = separator else {
        return false;
    };
    let token = &line[..separator];
    let value = &line[separator + 1..];
    !token.is_empty()
        && !value.trim().is_empty()
        && token.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || character.is_ascii_whitespace()
                || matches!(character, '-' | '_' | '.')
        })
}

pub fn validate_conventional_message(
    message: &str,
) -> std::result::Result<(), CommitMessageError> {
    let message = message.trim();
    if message.is_empty() {
        return Err(CommitMessageError::Empty);
    }
    if message.len() > MAX_COMMIT_MESSAGE_BYTES {
        return Err(CommitMessageError::MessageTooLong);
    }
    if message.chars().any(unsafe_message_character) {
        return Err(CommitMessageError::UnsafeCharacter);
    }
    if message.lines().any(|line| line.trim_end() != line) {
        return Err(CommitMessageError::TrailingWhitespace);
    }

    let mut lines = message.lines();
    let subject = lines.next().unwrap_or_default();
    if subject.chars().count() > MAX_COMMIT_SUBJECT_CHARS {
        return Err(CommitMessageError::SubjectTooLong);
    }
    validate_conventional_subject(subject)?;

    let body: Vec<&str> = lines.collect();
    if body.is_empty() {
        return Ok(());
    }
    if !body[0].is_empty() {
        return Err(CommitMessageError::MissingBodySeparator);
    }
    if body.len() < 2 || body[1].is_empty() {
        return Err(CommitMessageError::EmptyBody);
    }
    if body[1..].iter().any(|line| trailer_like_line(line)) {
        return Err(CommitMessageError::TrailerLikeBody);
    }
    Ok(())
}

pub fn parse_plan(raw: &str, staged: &[String], max_commits: usize) -> Result<Vec<PlanEntry>> {
    let plan: Vec<PlanEntry> = serde_json::from_str(&strip_fence(raw))
        .context("local AI did not return a JSON commit plan")?;
    if plan.is_empty() {
        bail!("local AI returned an empty commit plan");
    }
    if plan.len() > max_commits {
        bail!("commit plan exceeds the {max_commits}-commit limit");
    }
    let expected: BTreeSet<&str> = staged.iter().map(String::as_str).collect();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (index, entry) in plan.iter().enumerate() {
        if let Err(error) = validate_conventional_message(entry.message.trim()) {
            bail!(
                "commit plan entry {} has an invalid Conventional Commit message: {error}",
                index + 1
            );
        }
        if entry.files.is_empty() {
            bail!("commit plan entry {} has no files", index + 1);
        }
        for file in &entry.files {
            *counts.entry(file.as_str()).or_default() += 1;
        }
    }
    let actual: BTreeSet<&str> = counts.keys().copied().collect();
    let duplicates: Vec<&str> = counts
        .iter()
        .filter_map(|(path, count)| (*count > 1).then_some(*path))
        .collect();
    if !duplicates.is_empty() {
        bail!(
            "commit plan duplicates paths: {}",
            terminal_safe_paths(&duplicates)
        );
    }
    let unknown: Vec<&str> = actual.difference(&expected).copied().collect();
    if !unknown.is_empty() {
        bail!(
            "commit plan invents paths: {}",
            terminal_safe_paths(&unknown)
        );
    }
    let missing: Vec<&str> = expected.difference(&actual).copied().collect();
    if !missing.is_empty() {
        bail!(
            "commit plan omits paths: {}",
            terminal_safe_paths(&missing)
        );
    }
    Ok(plan)
}

pub fn validate_requested_plan(
    raw: &str,
    staged: &[String],
    max_commits: usize,
    single_commit: bool,
) -> Result<Vec<PlanEntry>> {
    let plan = parse_plan(raw, staged, max_commits)?;
    if single_commit && plan.len() != 1 {
        bail!("local AI ignored single-commit mode");
    }
    Ok(plan)
}
