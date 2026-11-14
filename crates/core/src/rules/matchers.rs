//! Validated building blocks of custom rules.

use glob::{MatchOptions, Pattern};
use regex::Regex;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::process::ProcessInfo;

/// A custom rule that cannot be constructed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InvalidRule {
    /// The rule text is empty or whitespace.
    #[error("rule text is empty")]
    Empty,
    /// An executable name contains a path separator.
    #[error("executable name {0:?} must not contain a path separator")]
    NameWithSeparator(String),
    /// The path glob does not parse.
    #[error("invalid path glob {pattern:?}: {reason}")]
    Glob {
        /// The glob as given.
        pattern: String,
        /// Why it does not parse.
        reason: String,
    },
    /// The regular expression does not parse.
    #[error("invalid regular expression {pattern:?}: {reason}")]
    Regex {
        /// The expression as given.
        pattern: String,
        /// Why it does not parse.
        reason: String,
    },
}

/// Text that is not empty or whitespace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct NonEmptyText(String);

impl NonEmptyText {
    /// The text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for NonEmptyText {
    type Error = InvalidRule;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.trim().is_empty() {
            return Err(InvalidRule::Empty);
        }
        Ok(Self(value))
    }
}

impl From<NonEmptyText> for String {
    fn from(value: NonEmptyText) -> Self {
        value.0
    }
}

/// File name of an executable, such as `java.exe`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ExecutableName(String);

impl ExecutableName {
    /// The file name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether the process runs an executable with this file name.
    ///
    /// Falls back to the first command-line argument when the executable
    /// path is unknown.
    #[must_use]
    pub fn matches(&self, process: &ProcessInfo) -> bool {
        let path = match &process.executable {
            Some(path) => path.to_string_lossy(),
            None => match process.command_line.first() {
                Some(argument) => argument.into(),
                None => return false,
            },
        };
        // Split on both separators so the result does not depend on the
        // OS the code runs on.
        let name = path.rsplit(['/', '\\']).next().unwrap_or_default();
        name.eq_ignore_ascii_case(&self.0)
    }
}

impl TryFrom<String> for ExecutableName {
    type Error = InvalidRule;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let value = value.trim().to_owned();
        if value.is_empty() {
            return Err(InvalidRule::Empty);
        }
        if value.contains(['/', '\\']) {
            return Err(InvalidRule::NameWithSeparator(value));
        }
        Ok(Self(value))
    }
}

impl From<ExecutableName> for String {
    fn from(value: ExecutableName) -> Self {
        value.0
    }
}

/// A glob over the executable path: `*` stays within one directory,
/// `**` spans several. Matching ignores case.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PathPattern(Pattern);

impl PathPattern {
    const OPTIONS: MatchOptions = MatchOptions {
        case_sensitive: false,
        require_literal_separator: true,
        require_literal_leading_dot: false,
    };

    /// The glob as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Whether the process executable matches the glob.
    #[must_use]
    pub fn matches(&self, process: &ProcessInfo) -> bool {
        process
            .executable
            .as_deref()
            .is_some_and(|path| self.0.matches_path_with(path, Self::OPTIONS))
    }
}

impl PartialEq for PathPattern {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

impl Eq for PathPattern {}

impl TryFrom<String> for PathPattern {
    type Error = InvalidRule;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.trim().is_empty() {
            return Err(InvalidRule::Empty);
        }
        Pattern::new(&value)
            .map(Self)
            .map_err(|error| InvalidRule::Glob {
                reason: error.msg.to_owned(),
                pattern: value,
            })
    }
}

impl From<PathPattern> for String {
    fn from(value: PathPattern) -> Self {
        value.0.as_str().to_owned()
    }
}

/// A regular expression over the command line.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CommandLinePattern(Regex);

impl CommandLinePattern {
    /// The expression as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Whether the expression matches anywhere in `command_line`.
    #[must_use]
    pub fn matches(&self, command_line: &str) -> bool {
        self.0.is_match(command_line)
    }
}

impl PartialEq for CommandLinePattern {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

impl Eq for CommandLinePattern {}

impl TryFrom<String> for CommandLinePattern {
    type Error = InvalidRule;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.trim().is_empty() {
            return Err(InvalidRule::Empty);
        }
        Regex::new(&value)
            .map(Self)
            .map_err(|error| InvalidRule::Regex {
                reason: error.to_string(),
                pattern: value,
            })
    }
}

impl From<CommandLinePattern> for String {
    fn from(value: CommandLinePattern) -> Self {
        value.0.as_str().to_owned()
    }
}
