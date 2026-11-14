//! Installing the agent as a service and the tray for every user.
//!
//! Binaries go to `Program Files`, which only administrators can change;
//! the service runs as `LocalSystem`, starts with Windows and restarts
//! after a crash. Standard users can query the service but not stop it.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::{env, fs, io, thread};

use windows_service::service::{
    ServiceAccess, ServiceAction, ServiceActionType, ServiceErrorControl, ServiceFailureActions,
    ServiceFailureResetPeriod, ServiceInfo, ServiceStartType, ServiceState, ServiceType,
};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use windows_sys::Win32::System::Registry::{
    HKEY_LOCAL_MACHINE, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW,
};

use super::super::PlatformError;
use super::service::SERVICE_NAME;
use super::wide::to_wide;

const AGENT_EXE: &str = "parentcontrol-agent.exe";
const TRAY_EXE: &str = "parentcontrol-tray.exe";

/// Where per-machine programs started at every logon are listed.
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const TRAY_VALUE: &str = "ParentControlTray";

/// How long to wait for the old service to stop when upgrading.
const STOP_TIMEOUT: Duration = Duration::from_secs(30);

/// Installs or upgrades the service and starts it.
pub(super) fn install() -> Result<(), PlatformError> {
    let target = program_dir();
    fs::create_dir_all(&target)
        .map_err(PlatformError::io(format!("creating {}", target.display())))?;

    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )
    .map_err(service_error("opening the service manager"))?;
    let access = ServiceAccess::QUERY_STATUS
        | ServiceAccess::START
        | ServiceAccess::STOP
        | ServiceAccess::CHANGE_CONFIG;
    let existing = manager.open_service(SERVICE_NAME, access).ok();
    if let Some(service) = &existing {
        stop_and_wait(service)?;
    }

    let source = env::current_exe().map_err(PlatformError::io("locating this program"))?;
    let agent = target.join(AGENT_EXE);
    copy_unless_same(&source, &agent)?;
    let tray = source.with_file_name(TRAY_EXE);
    let tray_installed = tray.exists();
    if tray_installed {
        copy_unless_same(&tray, &target.join(TRAY_EXE))?;
    }

    let info = ServiceInfo {
        name: SERVICE_NAME.into(),
        display_name: "Parent Control".into(),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: agent,
        launch_arguments: vec![OsString::from("service")],
        dependencies: Vec::new(),
        account_name: None,
        account_password: None,
    };
    let service = match existing {
        Some(service) => {
            service
                .change_config(&info)
                .map_err(service_error("updating the service"))?;
            service
        }
        None => manager
            .create_service(&info, access)
            .map_err(service_error("creating the service"))?,
    };
    service
        .set_description(
            "Ограничивает время в выбранных программах по правилам из родительской панели.",
        )
        .map_err(service_error("describing the service"))?;
    let restart = ServiceAction {
        action_type: ServiceActionType::Restart,
        delay: Duration::from_secs(5),
    };
    service
        .update_failure_actions(ServiceFailureActions {
            reset_period: ServiceFailureResetPeriod::After(Duration::from_hours(24)),
            reboot_msg: None,
            command: None,
            actions: Some(vec![restart.clone(), restart.clone(), restart]),
        })
        .map_err(service_error("setting restart on failure"))?;
    service
        .set_failure_actions_on_non_crash_failures(true)
        .map_err(service_error("setting restart on failure"))?;

    if tray_installed {
        let command = format!("\"{}\"", target.join(TRAY_EXE).display());
        set_run_value(&command)?;
    }
    service
        .start::<&str>(&[])
        .map_err(service_error("starting the service"))
}

/// Stops and removes the service and the tray autostart. Files and data
/// stay, so a reinstall continues with the same enrollment and history.
pub(super) fn uninstall() -> Result<(), PlatformError> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(service_error("opening the service manager"))?;
    let service = manager
        .open_service(
            SERVICE_NAME,
            ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE,
        )
        .map_err(service_error("opening the service"))?;
    stop_and_wait(&service)?;
    service
        .delete()
        .map_err(service_error("deleting the service"))?;
    delete_run_value();
    Ok(())
}

fn program_dir() -> PathBuf {
    let base = env::var_os("ProgramFiles").unwrap_or_else(|| r"C:\Program Files".into());
    PathBuf::from(base).join("ParentControl")
}

fn copy_unless_same(from: &Path, to: &Path) -> Result<(), PlatformError> {
    let same = fs::canonicalize(from)
        .ok()
        .zip(fs::canonicalize(to).ok())
        .is_some_and(|(a, b)| a == b);
    if !same {
        fs::copy(from, to).map_err(PlatformError::io(format!("copying to {}", to.display())))?;
    }
    Ok(())
}

fn stop_and_wait(service: &windows_service::service::Service) -> Result<(), PlatformError> {
    let status = service
        .query_status()
        .map_err(service_error("querying the service"))?;
    if status.current_state == ServiceState::Stopped {
        return Ok(());
    }
    // Stopping fails if it is already stopping; the wait below covers that.
    let _ = service.stop();
    let deadline = Instant::now() + STOP_TIMEOUT;
    while Instant::now() < deadline {
        let status = service
            .query_status()
            .map_err(service_error("querying the service"))?;
        if status.current_state == ServiceState::Stopped {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(250));
    }
    Err(PlatformError::Io {
        action: "stopping the service".to_owned(),
        source: io::ErrorKind::TimedOut.into(),
    })
}

fn set_run_value(command: &str) -> Result<(), PlatformError> {
    let (key, name, data) = (to_wide(RUN_KEY), to_wide(TRAY_VALUE), to_wide(command));
    let size = u32::try_from(data.len() * 2).unwrap_or(u32::MAX);
    // SAFETY: all strings are NUL-terminated and `size` covers `data`
    // including its terminator, as `REG_SZ` requires.
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            name.as_ptr(),
            REG_SZ,
            data.as_ptr().cast(),
            size,
        )
    };
    if status != 0 {
        return Err(PlatformError::Io {
            action: "registering the tray autostart".to_owned(),
            source: io::Error::from_raw_os_error(i32::try_from(status).unwrap_or(i32::MAX)),
        });
    }
    Ok(())
}

fn delete_run_value() {
    let (key, name) = (to_wide(RUN_KEY), to_wide(TRAY_VALUE));
    // SAFETY: both strings are NUL-terminated. A missing value is fine.
    unsafe { RegDeleteKeyValueW(HKEY_LOCAL_MACHINE, key.as_ptr(), name.as_ptr()) };
}

fn service_error(action: &str) -> impl FnOnce(windows_service::Error) -> PlatformError + '_ {
    move |error| PlatformError::Io {
        action: action.to_owned(),
        source: io::Error::other(error),
    }
}
