use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::process::ProcessId;

/// An inspector that reports a fixed list of loaded files.
struct LoadedFiles(Vec<PathBuf>);

impl ProcessInspector for LoadedFiles {
    fn loaded_files(&self, _: ProcessId) -> Vec<PathBuf> {
        self.0.clone()
    }
}

fn process(arguments: &[&str]) -> ProcessInfo {
    ProcessInfo {
        id: ProcessId::new(1, 1),
        user: None,
        executable: arguments.first().map(PathBuf::from),
        command_line: arguments.iter().map(ToString::to_string).collect(),
    }
}

fn detected(arguments: &[&str]) -> bool {
    is_minecraft(&process(arguments), &LoadedFiles(Vec::new()))
}

fn fixtures(kind: &str) -> Vec<(PathBuf, Vec<String>)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/minecraft")
        .join(kind);
    let mut fixtures: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "args"))
        .map(|path| {
            let arguments = fs::read_to_string(&path)
                .unwrap()
                .lines()
                .filter(|line| !line.starts_with('#'))
                .map(str::to_owned)
                .collect();
            (path, arguments)
        })
        .collect();
    assert!(!fixtures.is_empty(), "no fixtures in {}", dir.display());
    fixtures.sort();
    fixtures
}

#[test]
fn every_game_fixture_is_detected() {
    for (path, arguments) in fixtures("game") {
        let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
        assert!(detected(&arguments), "not detected: {}", path.display());
    }
}

#[test]
fn no_launcher_or_other_program_fixture_is_detected() {
    for (path, arguments) in fixtures("not-game") {
        let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
        assert!(!detected(&arguments), "false positive: {}", path.display());
    }
}

#[test]
fn main_class_alone_is_enough() {
    assert!(detected(&["x", "net.minecraft.client.main.Main"]));
    assert!(detected(&["x", "org.prismlauncher.EntryPoint"]));
}

#[test]
fn game_argument_alone_is_enough() {
    assert!(detected(&["x", "Other", "--gameDir", "/tmp/g"]));
}

#[test]
fn launcher_property_alone_is_enough() {
    assert!(detected(&[
        "x",
        "-Dminecraft.launcher.brand=custom",
        "Other"
    ]));
}

#[test]
fn client_jar_on_classpath_alone_is_enough() {
    assert!(detected(&[
        "x",
        "-cp",
        "/a.jar:/g/versions/1.8.9/1.8.9.jar",
        "Other"
    ]));
    assert!(detected(&[
        "x",
        "-Djava.class.path=/p/libraries/com/mojang/minecraft/1.21/minecraft-1.21-client.jar",
        "Other"
    ]));
    assert!(detected(&[
        "x",
        "-cp",
        r"C:\a.jar;C:\g\versions\1.8.9\1.8.9.jar",
        "Other"
    ]));
}

#[test]
fn executable_name_is_not_trusted_as_a_signal() {
    assert!(!detected(&["net.minecraft.client.main.Main"]));
}

#[test]
fn jar_in_versions_dir_must_match_its_directory() {
    assert!(!detected(&[
        "x",
        "-cp",
        "/g/versions/1.8.9/other.jar",
        "Other"
    ]));
}

#[test]
fn falls_back_to_loaded_files() {
    let inspector = LoadedFiles(vec![PathBuf::from(
        "/home/kid/.minecraft/versions/1.21/1.21.jar",
    )]);
    assert!(is_minecraft(
        &process(&["/opt/renamed", "Hidden"]),
        &inspector
    ));
}

#[test]
fn unrelated_loaded_files_do_not_match() {
    let inspector = LoadedFiles(vec![
        PathBuf::from("/usr/lib/jvm/lib/modules"),
        PathBuf::from("/opt/app.jar"),
    ]);
    assert!(!is_minecraft(
        &process(&["/usr/bin/java", "-jar", "/opt/app.jar"]),
        &inspector
    ));
}

#[test]
fn client_jar_layouts() {
    assert!(is_client_jar(
        "/home/kid/.minecraft/versions/1.21.1/1.21.1.jar"
    ));
    assert!(is_client_jar(
        r"C:\Users\kid\.minecraft\Versions\1.21.1\1.21.1.JAR"
    ));
    assert!(is_client_jar(
        "/p/libraries/com/mojang/minecraft/1.21/minecraft-1.21-client.jar"
    ));
    assert!(is_client_jar(
        "/m/libraries/net/minecraft/client/1.20.1-1/client-1.20.1-1-srg.jar"
    ));
    assert!(!is_client_jar(
        "/home/kid/.minecraft/versions/1.21.1/1.21.1.json"
    ));
    assert!(!is_client_jar("1.21.1.jar"));
    assert!(!is_client_jar(""));
}
