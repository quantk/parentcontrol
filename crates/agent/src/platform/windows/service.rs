//! Running under the Windows service control manager.

use std::ffi::OsString;
use std::io;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use tokio::sync::Notify;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{
    self, ServiceControlHandlerResult, ServiceStatusHandle,
};
use windows_service::{define_windows_service, service_dispatcher};

use super::super::{PlatformError, ServiceBody};

/// Name of the service in the service control manager.
pub(super) const SERVICE_NAME: &str = "ParentControl";

/// Signalled when the service control manager asks the service to stop.
static STOP: Notify = Notify::const_new();

/// The agent's work, handed from `run` to the service thread the
/// dispatcher creates.
static BODY: Mutex<Option<ServiceBody>> = Mutex::new(None);

define_windows_service!(ffi_service_main, service_main);

/// Connects to the service control manager and runs `body` as the
/// service; returns when the service has stopped.
pub(super) fn run(body: ServiceBody) -> Result<(), PlatformError> {
    *BODY.lock().unwrap_or_else(PoisonError::into_inner) = Some(body);
    service_dispatcher::start(SERVICE_NAME, ffi_service_main).map_err(|error| PlatformError::Io {
        action: "connecting to the service control manager".to_owned(),
        source: io::Error::other(error),
    })
}

/// Resolves when the service is asked to stop.
pub(super) async fn stop_requested() {
    STOP.notified().await;
}

fn service_main(_: Vec<OsString>) {
    let handler = |control| match control {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            STOP.notify_one();
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    };
    let Ok(status) = service_control_handler::register(SERVICE_NAME, handler) else {
        return;
    };
    report(status, ServiceState::Running, ServiceExitCode::Win32(0));
    let body = BODY.lock().unwrap_or_else(PoisonError::into_inner).take();
    let failed = body.is_some_and(|body| body().is_err());
    let exit = if failed {
        ServiceExitCode::ServiceSpecific(1)
    } else {
        ServiceExitCode::Win32(0)
    };
    report(status, ServiceState::Stopped, exit);
}

fn report(status: ServiceStatusHandle, state: ServiceState, exit_code: ServiceExitCode) {
    let accepted = if state == ServiceState::Running {
        ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN
    } else {
        ServiceControlAccept::empty()
    };
    // A failure to report leaves the manager waiting; nothing better can
    // be done from inside the service.
    let _ = status.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: accepted,
        exit_code,
        checkpoint: 0,
        wait_hint: Duration::ZERO,
        process_id: None,
    });
}
