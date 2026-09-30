//! PHT date helpers — ported from `lib/utils.ts` / `lib/supabase.ts`.
//! All trip dates are PHT (UTC+8); date-only strings must not shift to UTC.

use chrono::NaiveDate;

use crate::error::ApiError;

/// Parse a date-only string (`YYYY-MM-DD`) into a `NaiveDate` with no UTC shift.
pub fn parse_date_pht(s: &str) -> Result<NaiveDate, ApiError> {
    let s = s.trim();
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| ApiError::BadRequest(format!("invalid date: {s}")))
}

/// Ensure an ISO datetime carries an explicit PHT (+08:00) offset.
/// If the string has no `+`/`Z` offset, append `+08:00`; otherwise pass through.
pub fn ensure_pht_offset(iso: &str) -> Result<String, ApiError> {
    let iso = iso.trim();
    if iso.is_empty() {
        return Ok(iso.to_string());
    }
    if iso.contains('+') || iso.ends_with('Z') || iso.ends_with('z') {
        Ok(iso.to_string())
    } else {
        Ok(format!("{iso}+08:00"))
    }
}
