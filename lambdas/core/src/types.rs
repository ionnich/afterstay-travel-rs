//! Domain types shared across the `api`, `integrations`, and `chat` lambdas.
//!
//! The JSON wire format mirrors `lib/types.ts` exactly (camelCase field names,
//! via `#[serde(rename_all = "camelCase")]`); DB columns map 1:1 to
//! `infra/db/migrations/0001_init.sql` (snake_case, via
//! `#[sqlx(rename_all = "snake_case")]`). Structs whose table row maps directly
//! (no computed/client-only fields) derive [`sqlx::FromRow`]; the rest are
//! `Serialize`/`Deserialize` only and are mapped by hand in the `api` crate.
//! Column names are noted on those structs' doc comments.

#![allow(non_snake_case)] // field names mirror lib/types.ts camelCase

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::types::BigDecimal;

/// A trip. Column map (snake_case): `accommodation` <- `accommodation_name`,
/// `address` <- `accommodation_address`, `costCurrency` <- `currency`,
/// `transport` <- `transport_mode`, `heroImageUrl` <- `cover_image`.
/// `nights` is computed from `startDate`/`endDate`; `cost`, `locationRating`,
/// and `amenities` are client-only (no column).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trip {
    pub id: uuid::Uuid,
    pub name: String,
    pub destination: Option<String>,
    pub startDate: NaiveDate,
    pub endDate: NaiveDate,
    /// Derived from `startDate`/`endDate`; not a column.
    #[serde(default)]
    pub nights: i32,
    pub accommodation: Option<String>,
    pub address: Option<String>,
    pub roomType: Option<String>,
    pub checkIn: Option<String>,
    pub checkOut: Option<String>,
    pub hotelPhone: Option<String>,
    pub bookingRef: Option<String>,
    /// Client-only; not persisted.
    pub cost: Option<f64>,
    pub costCurrency: Option<String>,
    pub transport: Option<String>,
    pub wifiSsid: Option<String>,
    pub wifiPassword: Option<String>,
    pub doorCode: Option<String>,
    /// Client-only; not persisted.
    pub locationRating: Option<String>,
    /// Client-only; not persisted.
    pub amenities: Option<Vec<String>>,
    pub notes: Option<String>,
    pub status: String,
    pub heroImageUrl: Option<String>,
    pub hotelUrl: Option<String>,
    pub airportArrivalBuffer: Option<String>,
    pub airportToHotelTime: Option<String>,
    pub customQuickAccess: Option<String>,
    pub transportNotes: Option<String>,
    pub houseRules: Option<String>,
    pub emergencyContacts: Option<String>,
    /// `trips.hotel_photos` is a text column holding a JSON array of URL strings.
    pub hotelPhotos: Option<String>,
    pub budgetLimit: Option<BigDecimal>,
    pub budgetMode: Option<String>,
    pub userId: Option<String>,
    pub isPastImport: Option<bool>,
    pub confidenceLevel: Option<String>,
    pub datePrecision: Option<String>,
    pub country: Option<String>,
    pub countryCode: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub totalSpent: Option<BigDecimal>,
    pub totalNights: Option<f64>,
}

impl Trip {
    /// Parse the `hotelPhotos` JSON-array string into a list of URLs.
    pub fn hotel_photos_parsed(&self) -> Vec<String> {
        self.hotelPhotos
            .as_deref()
            .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
            .unwrap_or_default()
    }
}

/// A flight. `from`/`to` map to the `from_city`/`to_city` columns.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
#[sqlx(rename_all = "snake_case")]
pub struct Flight {
    pub id: uuid::Uuid,
    pub direction: String,
    pub flightNumber: String,
    pub airline: Option<String>,
    #[sqlx(rename = "from_city")]
    pub from: Option<String>,
    #[sqlx(rename = "to_city")]
    pub to: Option<String>,
    pub departTime: DateTime<Utc>,
    pub arriveTime: DateTime<Utc>,
    pub bookingRef: Option<String>,
    pub baggage: Option<String>,
    pub passenger: Option<String>,
}

/// A trip member. Column map: `profilePhoto` <- `avatar_url`.
/// `flightId`/`checkedBaggage` are client-only (no column in `trip_members`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupMember {
    pub id: uuid::Uuid,
    pub name: String,
    pub role: String,
    pub userId: Option<String>,
    /// Client-only; not persisted.
    pub flightId: Option<String>,
    /// Client-only; not persisted.
    pub checkedBaggage: Option<bool>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub profilePhoto: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
#[sqlx(rename_all = "snake_case")]
pub struct PackingItem {
    pub id: uuid::Uuid,
    #[sqlx(rename = "name")]
    pub item: String,
    pub category: String,
    #[sqlx(rename = "is_packed")]
    pub packed: bool,
    pub owner: Option<String>,
}

/// An expense. Column map: `description` <- `title`, `date` <- `expense_date`,
/// `photo` <- `photo_url`. `placeLatitude`/`placeLongitude` are client-only
/// (no column).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Expense {
    pub id: uuid::Uuid,
    pub description: String,
    pub amount: BigDecimal,
    pub currency: String,
    pub category: String,
    pub date: NaiveDate,
    pub paidBy: Option<String>,
    pub photo: Option<String>,
    pub placeName: Option<String>,
    /// Client-only; not persisted.
    pub placeLatitude: Option<f64>,
    /// Client-only; not persisted.
    pub placeLongitude: Option<f64>,
    pub splitType: Option<String>,
    pub notes: Option<String>,
}

/// A saved/discovered place. `voteByMember` is client-only (no column).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub id: uuid::Uuid,
    pub name: String,
    pub category: String,
    pub distance: Option<String>,
    pub notes: Option<String>,
    pub priceEstimate: Option<String>,
    pub rating: Option<i32>,
    pub source: String,
    pub vote: String,
    /// Client-only; not persisted.
    pub voteByMember: Option<serde_json::Value>,
    pub photoUrl: Option<String>,
    pub googlePlaceId: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub googleMapsUri: Option<String>,
    pub totalRatings: Option<i32>,
    pub saved: bool,
}

/// A checklist item. Column map: `task` <- `title`, `done` <- `is_done`.
/// `dueAt` is client-only (no `due_at` column).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecklistItem {
    pub id: uuid::Uuid,
    pub task: String,
    pub done: bool,
    pub doneBy: Option<String>,
    /// Client-only; not persisted.
    pub dueAt: Option<DateTime<Utc>>,
}

/// A moment/photo. Column map: `takenBy` <- `uploaded_by`, `date` <- `taken_at`.
/// `photo` is derived from `public_url` (with `storage_path` fallback) at read
/// time — not a direct column; the `api` layer sets it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Moment {
    pub id: uuid::Uuid,
    pub caption: String,
    pub photo: Option<String>,
    pub location: Option<String>,
    pub takenBy: Option<String>,
    pub date: NaiveDate,
    pub tags: Vec<String>,
}

/// A trip file. Column map: `fileName` <- `name`, `type` <- `file_type`,
/// `notes` <- `description`.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
#[sqlx(rename_all = "snake_case")]
pub struct TripFile {
    pub id: uuid::Uuid,
    #[sqlx(rename = "name")]
    pub fileName: String,
    pub fileUrl: Option<String>,
    #[serde(rename = "type")]
    #[sqlx(rename = "file_type")]
    pub r#type: String,
    #[sqlx(rename = "description")]
    pub notes: Option<String>,
    pub printRequired: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
#[sqlx(rename_all = "snake_case")]
pub struct ChatMessage {
    pub id: uuid::Uuid,
    pub tripId: uuid::Uuid,
    pub senderName: String,
    pub senderAvatar: Option<String>,
    pub message: String,
    pub createdAt: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
#[sqlx(rename_all = "snake_case")]
pub struct TripInvite {
    pub id: uuid::Uuid,
    pub code: String,
    pub createdAt: Option<DateTime<Utc>>,
    pub expiresAt: Option<DateTime<Utc>>,
    pub used: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
#[sqlx(rename_all = "snake_case")]
pub struct Profile {
    pub id: String,
    pub fullName: String,
    pub avatarUrl: Option<String>,
    pub phone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
#[sqlx(rename_all = "snake_case")]
pub struct LifetimeStats {
    pub totalTrips: Option<i32>,
    pub totalCountries: Option<i32>,
    pub totalNights: Option<i32>,
    pub totalMiles: Option<f64>,
    pub totalSpent: Option<BigDecimal>,
    pub homeCurrency: Option<String>,
    pub totalMoments: Option<i32>,
    pub countriesList: Option<Vec<String>>,
    pub earliestTripDate: Option<NaiveDate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
#[sqlx(rename_all = "snake_case")]
pub struct Highlight {
    pub id: uuid::Uuid,
    #[serde(rename = "type")]
    #[sqlx(rename = "type")]
    pub r#type: Option<String>,
    pub displayText: Option<String>,
    pub supportingData: Option<serde_json::Value>,
    pub rank: Option<i32>,
}
