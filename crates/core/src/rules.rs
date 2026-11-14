//! Rules that recognise the programs of a group.

mod matchers;
mod minecraft;

use serde::{Deserialize, Serialize};

pub use self::matchers::{
    CommandLinePattern, ExecutableName, InvalidRule, NonEmptyText, PathPattern,
};
use crate::process::{ProcessInfo, ProcessInspector};

/// A recognisable program that needs more than one simple matcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinProfile {
    /// The Minecraft Java Edition client from any launcher, but not the
    /// launcher itself.
    MinecraftJava,
}

/// One way of recognising a program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    /// A built-in profile.
    Builtin(BuiltinProfile),
    /// The executable's file name, compared case-insensitively.
    Executable(ExecutableName),
    /// A glob over the executable's full path.
    PathGlob(PathPattern),
    /// A substring of the space-joined command line.
    CommandLineContains(NonEmptyText),
    /// A regular expression over the space-joined command line.
    CommandLineRegex(CommandLinePattern),
}

impl Rule {
    /// Whether `process` is a program this rule describes.
    ///
    /// `inspector` is consulted only by rules that need more than the
    /// process snapshot.
    #[must_use]
    pub fn matches(&self, process: &ProcessInfo, inspector: &dyn ProcessInspector) -> bool {
        match self {
            Self::Builtin(BuiltinProfile::MinecraftJava) => {
                minecraft::is_minecraft(process, inspector)
            }
            Self::Executable(name) => name.matches(process),
            Self::PathGlob(pattern) => pattern.matches(process),
            Self::CommandLineContains(text) => joined_command_line(process).contains(text.as_str()),
            Self::CommandLineRegex(pattern) => pattern.matches(&joined_command_line(process)),
        }
    }
}

fn joined_command_line(process: &ProcessInfo) -> String {
    process.command_line.join(" ")
}

#[cfg(test)]
mod tests;
