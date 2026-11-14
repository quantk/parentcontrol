//! Server configuration from command-line flags or environment variables.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use clap::Args;
use jiff::tz::TimeZone;
use parentcontrol_core::calendar::{Calendar, TimeOfDay};

/// How to run the server.
#[derive(Debug, Clone, Args)]
pub struct Config {
    /// Address and port to listen on.
    #[arg(long, env = "PARENTCONTROL_LISTEN", default_value = "0.0.0.0:8443")]
    pub listen: SocketAddr,

    /// Directory for the database and the generated certificate.
    #[arg(
        long,
        env = "PARENTCONTROL_DATA_DIR",
        default_value = "/var/lib/parentcontrol-server"
    )]
    pub data_dir: PathBuf,

    /// IANA time zone in which budget days and windows are counted, such
    /// as `Europe/Moscow`. Used on first start; later changed in the panel.
    #[arg(long, env = "PARENTCONTROL_TIME_ZONE", value_parser = parse_time_zone)]
    pub time_zone: TimeZone,

    /// When each budget day starts. Used on first start; later changed in
    /// the panel.
    #[arg(long, env = "PARENTCONTROL_DAY_START", default_value = "04:00")]
    pub day_start: TimeOfDay,

    /// PEM certificate chain to serve instead of the generated one.
    #[arg(long, env = "PARENTCONTROL_TLS_CERT", requires = "tls_key")]
    pub tls_cert: Option<PathBuf>,

    /// PEM private key matching `--tls-cert`.
    #[arg(long, env = "PARENTCONTROL_TLS_KEY", requires = "tls_cert")]
    pub tls_key: Option<PathBuf>,
}

impl Config {
    /// The calendar to start with when the database has none yet.
    pub(crate) fn initial_calendar(&self) -> Calendar {
        Calendar::new(self.time_zone.clone(), self.day_start)
    }

    /// Path of the SQLite database.
    pub(crate) fn database_path(&self) -> PathBuf {
        database_path(&self.data_dir)
    }
}

/// Path of the SQLite database in `data_dir`.
pub(crate) fn database_path(data_dir: &Path) -> PathBuf {
    data_dir.join("parentcontrol.sqlite")
}

fn parse_time_zone(name: &str) -> Result<TimeZone, String> {
    TimeZone::get(name).map_err(|error| error.to_string())
}
