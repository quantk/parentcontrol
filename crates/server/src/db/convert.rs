//! Conversions between domain values and their stored representation.

use std::time::Duration;

use jiff::Timestamp;
use parentcontrol_core::calendar::BudgetDay;
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::DbError;

pub(super) fn corrupt(column: &'static str, reason: impl std::fmt::Display) -> DbError {
    DbError::Corrupt {
        column,
        reason: reason.to_string(),
    }
}

/// Stored form of a moment: fixed millisecond precision, so that string
/// comparison in SQL orders moments correctly.
pub(super) fn text(moment: Timestamp) -> String {
    format!("{moment:.3}")
}

pub(super) fn timestamp(text: &str, column: &'static str) -> Result<Timestamp, DbError> {
    text.parse().map_err(|error| corrupt(column, error))
}

pub(super) fn optional_timestamp(
    text: Option<&str>,
    column: &'static str,
) -> Result<Option<Timestamp>, DbError> {
    text.map(|text| timestamp(text, column)).transpose()
}

pub(super) fn day(text: &str, column: &'static str) -> Result<BudgetDay, DbError> {
    text.parse()
        .map(BudgetDay::new)
        .map_err(|error| corrupt(column, error))
}

pub(super) fn json<T: DeserializeOwned>(text: &str, column: &'static str) -> Result<T, DbError> {
    serde_json::from_str(text).map_err(|error| corrupt(column, error))
}

/// Encodes a domain value; they all serialize infallibly, but a failure
/// would still be reported rather than stored as garbage.
pub(super) fn to_json(value: &impl Serialize, column: &'static str) -> Result<String, DbError> {
    serde_json::to_string(value).map_err(|error| corrupt(column, error))
}

pub(super) fn duration(millis: i64) -> Duration {
    Duration::from_millis(millis.try_into().unwrap_or(0))
}

pub(super) fn millis(duration: Duration) -> i64 {
    duration.as_millis().try_into().unwrap_or(i64::MAX)
}
