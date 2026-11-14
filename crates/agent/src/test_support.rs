//! Fakes of the OS and the clock for unit tests.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;

use jiff::Timestamp;
use parentcontrol_core::clock::Clock;
use parentcontrol_core::enforcement::Termination;
use parentcontrol_core::policy::Policy;
use parentcontrol_core::process::{OsUser, ProcessId, ProcessInfo, ProcessInspector};

use crate::platform::{Platform, PlatformError};

/// A policy for account `kid`: group 1 "Игры" with Minecraft, `quota`
/// minutes a day inside 16:00–20:00, days starting at 04:00 in Moscow.
pub(crate) fn policy(quota: u32) -> Policy {
    let day = format!(r#"{{"quota": {quota}, "windows": [{{"start": "16:00", "end": "20:00"}}]}}"#);
    let json = format!(
        r#"{{
            "time_zone": "Europe/Moscow",
            "day_start": "04:00",
            "accounts": ["kid"],
            "groups": [{{
                "id": 1,
                "name": "Игры",
                "rules": [{{"builtin": "minecraft_java"}}],
                "schedule": {{
                    "monday": {day}, "tuesday": {day}, "wednesday": {day}, "thursday": {day},
                    "friday": {day}, "saturday": {day}, "sunday": {day}
                }}
            }}]
        }}"#
    );
    serde_json::from_str(&json).unwrap()
}

/// A Minecraft client process of `user`.
pub(crate) fn minecraft(pid: u32, user: &str) -> ProcessInfo {
    ProcessInfo {
        id: ProcessId::new(pid, 1),
        user: Some(OsUser::new(user)),
        executable: Some(PathBuf::from("/usr/bin/java")),
        command_line: [
            "/usr/bin/java",
            "-Xmx2G",
            "net.minecraft.client.main.Main",
            "--gameDir",
            "/home/x",
        ]
        .map(String::from)
        .to_vec(),
    }
}

/// An OS whose process list the test controls and which records
/// terminations instead of performing them.
#[derive(Debug, Default)]
pub(crate) struct FakePlatform {
    pub(crate) processes: RefCell<Vec<ProcessInfo>>,
    pub(crate) terminated: RefCell<Vec<(u32, Termination)>>,
    /// Behave like Windows: soft closing needs the tray.
    pub(crate) needs_tray_to_close: bool,
}

impl Platform for FakePlatform {
    fn processes(&self) -> Result<Vec<ProcessInfo>, PlatformError> {
        Ok(self.processes.borrow().clone())
    }

    fn terminate(&self, process: ProcessId, mode: Termination) -> Result<(), PlatformError> {
        self.terminated.borrow_mut().push((process.pid(), mode));
        Ok(())
    }

    fn closes_softly(&self) -> bool {
        !self.needs_tray_to_close
    }
}

impl ProcessInspector for FakePlatform {
    fn loaded_files(&self, _: ProcessId) -> Vec<PathBuf> {
        Vec::new()
    }
}

/// A clock the test moves by hand.
#[derive(Debug)]
pub(crate) struct FakeClock(pub(crate) Cell<Timestamp>);

impl FakeClock {
    /// A clock showing `rfc3339`.
    pub(crate) fn at(rfc3339: &str) -> Self {
        Self(Cell::new(rfc3339.parse().unwrap()))
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Timestamp {
        self.0.get()
    }
}

impl Platform for &FakePlatform {
    fn processes(&self) -> Result<Vec<ProcessInfo>, PlatformError> {
        (**self).processes()
    }

    fn terminate(&self, process: ProcessId, mode: Termination) -> Result<(), PlatformError> {
        (**self).terminate(process, mode)
    }

    fn closes_softly(&self) -> bool {
        (**self).closes_softly()
    }
}

impl ProcessInspector for &FakePlatform {
    fn loaded_files(&self, id: ProcessId) -> Vec<PathBuf> {
        (**self).loaded_files(id)
    }
}

impl Clock for &FakeClock {
    fn now(&self) -> Timestamp {
        (**self).now()
    }
}
