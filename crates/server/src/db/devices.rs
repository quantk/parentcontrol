//! Devices, their enrollment and their bound accounts.

use std::collections::BTreeSet;

use jiff::Timestamp;
use parentcontrol_core::ids::{DeviceId, ProfileId};
use parentcontrol_core::process::OsUser;
use parentcontrol_core::protocol::DevicePlatform;
use rusqlite::{Connection, OptionalExtension, params};

use super::DbError;
use super::convert::{corrupt, json, optional_timestamp, text, to_json};

/// A device as stored.
#[derive(Debug, Clone)]
pub(crate) struct Device {
    pub(crate) id: DeviceId,
    pub(crate) name: String,
    pub(crate) platform: Option<DevicePlatform>,
    /// Whether an agent has enrolled; until then only a code exists.
    pub(crate) enrolled: bool,
    pub(crate) last_seen: Option<Timestamp>,
    pub(crate) local_users: Vec<OsUser>,
    pub(crate) bindings: BTreeSet<OsUser>,
}

/// All devices, ordered by name.
pub(crate) fn list(connection: &Connection) -> Result<Vec<Device>, DbError> {
    let mut statement = connection.prepare(
        "SELECT id, name, platform, token_hash IS NOT NULL, last_seen, local_users
         FROM devices ORDER BY name, id",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, bool>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(id, name, platform, enrolled, last_seen, local_users)| {
            let id = DeviceId::new(id);
            Ok(Device {
                id,
                name,
                platform: platform
                    .map(|text| {
                        text.parse()
                            .map_err(|error| corrupt("devices.platform", error))
                    })
                    .transpose()?,
                enrolled,
                last_seen: optional_timestamp(last_seen.as_deref(), "devices.last_seen")?,
                local_users: json(&local_users, "devices.local_users")?,
                bindings: bindings(connection, id)?,
            })
        })
        .collect()
}

/// Creates a device that waits for an agent to enroll with `code_hash`.
pub(crate) fn create(
    connection: &Connection,
    name: &str,
    code_hash: &str,
    expires: Timestamp,
) -> Result<DeviceId, DbError> {
    connection.execute(
        "INSERT INTO devices (name, enroll_code_hash, enroll_expires) VALUES (?1, ?2, ?3)",
        params![name, code_hash, text(expires)],
    )?;
    Ok(DeviceId::new(connection.last_insert_rowid()))
}

/// Issues a new enrollment code for an existing device, for reinstalls.
pub(crate) fn reissue_code(
    connection: &Connection,
    device: DeviceId,
    code_hash: &str,
    expires: Timestamp,
) -> Result<bool, DbError> {
    let changed = connection.execute(
        "UPDATE devices SET enroll_code_hash = ?2, enroll_expires = ?3 WHERE id = ?1",
        params![device.get(), code_hash, text(expires)],
    )?;
    Ok(changed > 0)
}

/// Completes enrollment: trades a valid code for a token. The code is
/// consumed and the previous token, if any, stops working.
pub(crate) fn enroll(
    connection: &mut Connection,
    code_hash: &str,
    token_hash: &str,
    platform: DevicePlatform,
    now: Timestamp,
) -> Result<Option<DeviceId>, DbError> {
    let transaction = connection.transaction()?;
    let device: Option<i64> = transaction
        .query_row(
            "SELECT id FROM devices WHERE enroll_code_hash = ?1 AND enroll_expires > ?2",
            params![code_hash, text(now)],
            |row| row.get(0),
        )
        .optional()?;
    let Some(device) = device else {
        return Ok(None);
    };
    transaction.execute(
        "UPDATE devices SET token_hash = ?2, enroll_code_hash = NULL, enroll_expires = NULL,
         platform = ?3, last_seen = ?4 WHERE id = ?1",
        params![device, token_hash, platform.as_str(), text(now)],
    )?;
    transaction.commit()?;
    Ok(Some(DeviceId::new(device)))
}

/// The device holding the token with `token_hash`.
pub(crate) fn authenticate(
    connection: &Connection,
    token_hash: &str,
) -> Result<Option<DeviceId>, DbError> {
    let device = connection
        .query_row(
            "SELECT id FROM devices WHERE token_hash = ?1",
            [token_hash],
            |row| row.get(0),
        )
        .optional()?;
    Ok(device.map(DeviceId::new))
}

/// Stores what an agent reported on connecting.
pub(crate) fn record_hello(
    connection: &Connection,
    device: DeviceId,
    platform: DevicePlatform,
    local_users: &[OsUser],
    now: Timestamp,
) -> Result<(), DbError> {
    connection.execute(
        "UPDATE devices SET platform = ?2, local_users = ?3, last_seen = ?4 WHERE id = ?1",
        params![
            device.get(),
            platform.as_str(),
            to_json(&local_users, "devices.local_users")?,
            text(now)
        ],
    )?;
    Ok(())
}

/// Whether the device exists.
pub(crate) fn exists(connection: &Connection, device: DeviceId) -> Result<bool, DbError> {
    let found = connection
        .query_row(
            "SELECT 1 FROM devices WHERE id = ?1",
            [device.get()],
            |_| Ok(()),
        )
        .optional()?;
    Ok(found.is_some())
}

/// Notes that the device is alive.
pub(crate) fn touch(
    connection: &Connection,
    device: DeviceId,
    now: Timestamp,
) -> Result<(), DbError> {
    connection.execute(
        "UPDATE devices SET last_seen = ?2 WHERE id = ?1",
        params![device.get(), text(now)],
    )?;
    Ok(())
}

/// Renames a device.
pub(crate) fn rename(
    connection: &Connection,
    device: DeviceId,
    name: &str,
) -> Result<bool, DbError> {
    Ok(connection.execute(
        "UPDATE devices SET name = ?2 WHERE id = ?1",
        params![device.get(), name],
    )? > 0)
}

/// Deletes a device and its bindings; its agent's token stops working.
pub(crate) fn delete(connection: &Connection, device: DeviceId) -> Result<bool, DbError> {
    Ok(connection.execute("DELETE FROM devices WHERE id = ?1", [device.get()])? > 0)
}

/// Binds an account of the device to a profile.
pub(crate) fn bind(
    connection: &Connection,
    device: DeviceId,
    user: &OsUser,
    profile: ProfileId,
) -> Result<(), DbError> {
    connection.execute(
        "INSERT INTO bindings (device_id, os_user, profile_id) VALUES (?1, ?2, ?3)
         ON CONFLICT (device_id, os_user) DO UPDATE SET profile_id = excluded.profile_id",
        params![device.get(), user.as_str(), profile.get()],
    )?;
    Ok(())
}

/// Unbinds an account of the device.
pub(crate) fn unbind(
    connection: &Connection,
    device: DeviceId,
    user: &OsUser,
) -> Result<bool, DbError> {
    let changed = connection.execute(
        "DELETE FROM bindings WHERE device_id = ?1 AND os_user = ?2",
        params![device.get(), user.as_str()],
    )?;
    Ok(changed > 0)
}

/// Accounts of the device bound to any profile.
pub(crate) fn bindings(
    connection: &Connection,
    device: DeviceId,
) -> Result<BTreeSet<OsUser>, DbError> {
    let mut statement = connection.prepare("SELECT os_user FROM bindings WHERE device_id = ?1")?;
    let users = statement
        .query_map([device.get()], |row| {
            row.get::<_, String>(0).map(OsUser::new)
        })?
        .collect::<Result<_, _>>()?;
    Ok(users)
}

#[cfg(test)]
mod tests;
