//! Date helpers — timezone-neutral.
//! Date-only strings must not shift to UTC; offset-less datetimes are read as UTC.

use chrono::NaiveDate;

use crate::error::ApiError;

/// Parse a date-only string (`YYYY-MM-DD`) into a `NaiveDate` with no UTC shift.
pub fn parse_date(s: &str) -> Result<NaiveDate, ApiError> {
    let s = s.trim();
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| ApiError::BadRequest(format!("invalid date: {s}")))
}

/// Ensure an ISO datetime carries an explicit offset.
/// If the string has no `+`/`Z` offset, read it as UTC; otherwise pass through.
pub fn ensure_utc_offset(iso: &str) -> Result<String, ApiError> {
    let iso = iso.trim();
    if iso.is_empty() {
        return Ok(iso.to_string());
    }
    if iso.contains('+') || iso.ends_with('Z') || iso.ends_with('z') {
        Ok(iso.to_string())
    } else {
        Ok(format!("{iso}+00:00"))
    }
}
