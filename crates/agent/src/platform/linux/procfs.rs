//! Reading processes from `/proc`.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use parentcontrol_core::process::{ProcessId, ProcessInfo};

use super::passwd::Users;

/// Every process that could be read.
pub(super) fn processes(users: &Users) -> io::Result<Vec<ProcessInfo>> {
    let processes = fs::read_dir("/proc")?
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse::<u32>().ok())
        .filter_map(|pid| read(pid, users))
        .collect();
    Ok(processes)
}

/// Whether `process` still runs, as opposed to having exited or its PID
/// having been reused.
pub(super) fn is_running(process: ProcessId) -> bool {
    Stat::read(process.pid()).is_some_and(|stat| !stat.exited && stat.started == process.started())
}

/// Files `process` has mapped into memory or open.
pub(super) fn loaded_files(process: ProcessId) -> Vec<PathBuf> {
    let dir = process_dir(process.pid());
    let mut files = BTreeSet::new();
    if let Ok(maps) = fs::read_to_string(dir.join("maps")) {
        files.extend(maps.lines().filter_map(mapped_path));
    }
    if let Ok(fds) = fs::read_dir(dir.join("fd")) {
        files.extend(
            fds.filter_map(|fd| fs::read_link(fd.ok()?.path()).ok())
                .filter(|target| target.is_absolute()),
        );
    }
    // Check last: if the PID was reused meanwhile, the files are a stranger's.
    if is_running(process) {
        files.into_iter().collect()
    } else {
        Vec::new()
    }
}

fn read(pid: u32, users: &Users) -> Option<ProcessInfo> {
    let dir = process_dir(pid);
    let stat = Stat::read(pid)?;
    if stat.exited {
        return None;
    }
    let started = stat.started;
    let uid = fs::metadata(&dir).ok()?.uid();
    let command_line = fs::read(dir.join("cmdline"))
        .map(|raw| split_command_line(&raw))
        .unwrap_or_default();
    Some(ProcessInfo {
        id: ProcessId::new(pid, started),
        user: users.name(uid).cloned(),
        executable: fs::read_link(dir.join("exe")).ok(),
        command_line,
    })
}

fn process_dir(pid: u32) -> PathBuf {
    Path::new("/proc").join(pid.to_string())
}

/// The fields of `/proc/<pid>/stat` the agent needs.
#[derive(Debug, PartialEq, Eq)]
struct Stat {
    /// Start time in clock ticks since boot (field 22).
    started: u64,
    /// Whether the process is a zombie or dead (field 3 is `Z` or `X`):
    /// it has exited and only waits for its parent to collect it.
    exited: bool,
}

impl Stat {
    fn read(pid: u32) -> Option<Self> {
        Self::parse(&fs::read_to_string(process_dir(pid).join("stat")).ok()?)
    }

    fn parse(stat: &str) -> Option<Self> {
        // The command name (field 2) is in parentheses and may contain
        // spaces or parentheses itself, so count fields from the last `)`.
        let mut fields = stat[stat.rfind(')')? + 1..].split_whitespace();
        let state = fields.next()?;
        let started = fields.nth(18)?.parse().ok()?;
        Some(Self {
            started,
            exited: matches!(state, "Z" | "X"),
        })
    }
}

/// Splits `/proc/<pid>/cmdline`: arguments, each terminated by a NUL.
fn split_command_line(raw: &[u8]) -> Vec<String> {
    let raw = raw.strip_suffix(&[0]).unwrap_or(raw);
    if raw.is_empty() {
        return Vec::new();
    }
    raw.split(|&byte| byte == 0)
        .map(|argument| String::from_utf8_lossy(argument).into_owned())
        .collect()
}

/// The path column of a `/proc/<pid>/maps` line, if it names a file.
fn mapped_path(line: &str) -> Option<PathBuf> {
    let path = line.splitn(6, char::is_whitespace).nth(5)?.trim_start();
    path.starts_with('/').then(|| PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_survives_odd_command_names() {
        let stat =
            "4242 (java) x) S 1 4242 4242 0 -1 4194560 100 0 0 0 5 3 0 0 20 0 30 0 987654 0 0";
        let expected = Stat {
            started: 987_654,
            exited: false,
        };
        assert_eq!(Stat::parse(stat), Some(expected));
    }

    #[test]
    fn zombies_count_as_exited() {
        let stat = "4242 (java) Z 1 4242 4242 0 -1 4194560 100 0 0 0 5 3 0 0 20 0 30 0 9 0 0";
        assert_eq!(Stat::parse(stat).map(|stat| stat.exited), Some(true));
    }

    #[test]
    fn command_line_drops_trailing_terminator_only() {
        assert_eq!(
            split_command_line(b"java\0-cp\0\0a.jar\0"),
            ["java", "-cp", "", "a.jar"]
        );
        assert_eq!(split_command_line(b""), Vec::<String>::new());
    }

    #[test]
    fn maps_line_yields_file_paths() {
        let line =
            "7f00-7f10 r--s 00000000 08:01 1234    /home/kid/.minecraft/versions/1.21/1.21.jar";
        assert_eq!(
            mapped_path(line),
            Some(PathBuf::from("/home/kid/.minecraft/versions/1.21/1.21.jar"))
        );
        assert_eq!(
            mapped_path("7f00-7f10 rw-p 00000000 00:00 0    [heap]"),
            None
        );
        assert_eq!(mapped_path("7f00-7f10 rw-p 00000000 00:00 0"), None);
    }

    #[test]
    fn reads_its_own_process() {
        let pid = std::process::id();
        let me = read(pid, &Users::default()).unwrap();
        assert!(is_running(me.id));
        assert_ne!(me.command_line, Vec::<String>::new());
        assert!(me.executable.is_some());
    }
}
