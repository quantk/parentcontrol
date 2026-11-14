use std::path::PathBuf;

use super::*;
use crate::process::ProcessId;

struct NoFiles;

impl ProcessInspector for NoFiles {
    fn loaded_files(&self, _: ProcessId) -> Vec<PathBuf> {
        Vec::new()
    }
}

fn process(executable: Option<&str>, arguments: &[&str]) -> ProcessInfo {
    ProcessInfo {
        id: ProcessId::new(7, 1),
        user: None,
        executable: executable.map(PathBuf::from),
        command_line: arguments.iter().map(ToString::to_string).collect(),
    }
}

fn rule(json: &str) -> Rule {
    serde_json::from_str(json).unwrap()
}

#[test]
fn executable_name_ignores_case_and_directory() {
    let rule = rule(r#"{"executable": "Game.exe"}"#);
    assert!(rule.matches(&process(Some(r"C:\Games\game.EXE"), &[]), &NoFiles));
    assert!(rule.matches(&process(Some("/opt/Game.exe"), &[]), &NoFiles));
    assert!(!rule.matches(&process(Some("/opt/Game.exe.bak"), &[]), &NoFiles));
}

#[test]
fn executable_name_falls_back_to_first_argument() {
    let rule = rule(r#"{"executable": "steam"}"#);
    assert!(rule.matches(&process(None, &["/usr/bin/steam", "-silent"]), &NoFiles));
}

#[test]
fn executable_name_rejects_paths_and_blanks() {
    assert!(serde_json::from_str::<Rule>(r#"{"executable": "/usr/bin/x"}"#).is_err());
    assert!(serde_json::from_str::<Rule>(r#"{"executable": "  "}"#).is_err());
}

#[test]
fn path_glob_single_star_stays_in_one_directory() {
    let rule = rule(r#"{"path_glob": "/opt/games/*"}"#);
    assert!(rule.matches(&process(Some("/opt/games/tetris"), &[]), &NoFiles));
    assert!(!rule.matches(&process(Some("/opt/games/sub/tetris"), &[]), &NoFiles));
}

#[test]
fn path_glob_double_star_spans_directories() {
    let rule = rule(r#"{"path_glob": "/opt/games/**/*"}"#);
    assert!(rule.matches(&process(Some("/opt/games/sub/tetris"), &[]), &NoFiles));
}

#[test]
fn path_glob_needs_a_known_executable() {
    let rule = rule(r#"{"path_glob": "**"}"#);
    assert!(!rule.matches(&process(None, &["tetris"]), &NoFiles));
}

#[test]
fn invalid_glob_is_rejected() {
    assert!(serde_json::from_str::<Rule>(r#"{"path_glob": "/opt/[x"}"#).is_err());
}

#[test]
fn command_line_substring() {
    let rule = rule(r#"{"command_line_contains": "--level 3"}"#);
    assert!(rule.matches(&process(None, &["game", "--level", "3"]), &NoFiles));
    assert!(!rule.matches(&process(None, &["game", "--level", "4"]), &NoFiles));
}

#[test]
fn command_line_regex() {
    let rule = rule(r#"{"command_line_regex": "^python3? .*bot\\.py$"}"#);
    assert!(rule.matches(&process(None, &["python3", "/home/kid/bot.py"]), &NoFiles));
    assert!(!rule.matches(&process(None, &["python3", "/home/kid/bot.pyc"]), &NoFiles));
}

#[test]
fn invalid_regex_is_rejected() {
    assert!(serde_json::from_str::<Rule>(r#"{"command_line_regex": "(unclosed"}"#).is_err());
}

#[test]
fn builtin_profile_round_trips() {
    let rule = rule(r#"{"builtin": "minecraft_java"}"#);
    assert_eq!(rule, Rule::Builtin(BuiltinProfile::MinecraftJava));
    assert_eq!(
        serde_json::to_string(&rule).unwrap(),
        r#"{"builtin":"minecraft_java"}"#
    );
}
