//! HTTP handlers for the `/v1/data` data-CRUD API.
//!
//! Wire format mirrors `lib/supabase.ts` (camelCase, via `core::types`). DB
//! columns map 1:1 to `infra/db/migrations/0001_init.sql`.

use std::collections::BTreeMap;
use std::str::FromStr;

use chrono::{DateTime, NaiveDate, Utc};
use lambda_http::{Body, Request};
use lambda_http::http::Response;
use serde::Deserialize;
use serde_json::json;
use sqlx::types::BigDecimal;
use uuid::Uuid;

use core::error::ApiError;
use core::time::{ensure_pht_offset, parse_date_pht};

use crate::{ok, ok_body, parse_uuid, Ctx};

// ---------- helpers ----------

async fn body<T: serde::de::DeserializeOwned>(req: &Request) -> Result<T, ApiError> {
    let bytes = req.body().to_vec();
    serde_json::from_slice(&bytes).map_err(ApiError::from)
}

fn to_big_decimal(n: f64) -> BigDecimal {
    BigDecimal::from_str(&n.to_string()).unwrap_or_default()
}

fn to_f64(b: Option<BigDecimal>) -> Option<f64> {
    b.and_then(|d| d.to_string().parse::<f64>().ok())
}

fn parse_dt(s: &str) -> Result<DateTime<Utc>, ApiError> {
    let iso = ensure_pht_offset(s)?;
    DateTime::parse_from_rfc3339(&iso)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| ApiError::BadRequest(format!("invalid datetime: {s}")))
}

fn pht_today() -> NaiveDate {
    chrono::Utc::now()
        .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).expect("valid offset"))
        .date_naive()
}

fn query_param(req: &Request, name: &str) -> Option<String> {
    req.uri().query().and_then(|q| {
        q.split('&').find_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            (k == name).then(|| v.to_string())
        })
    })
}

fn generate_code() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..6].to_uppercase()
}

impl Ctx<'_> {
    async fn assert_member(&self, trip_id: Uuid) -> Result<(), ApiError> {
        core::auth::assert_trip_member(&self.state.pool, self.sub, trip_id).await
    }
}

/// Resolve the caller's active trip id (or the provided one, if any).
async fn resolve_trip_id(ctx: &Ctx<'_>, opt: Option<&str>) -> Result<Uuid, ApiError> {
    if let Some(s) = opt {
        return parse_uuid(s);
    }
    let id: Option<Uuid> = sqlx::query_scalar(
        "SELECT t.id FROM trips t \
         JOIN trip_members m ON m.trip_id = t.id \
         WHERE m.user_id = $1 AND t.status <> 'Completed' \
         ORDER BY t.start_date DESC LIMIT 1",
    )
    .bind(ctx.sub)
    .fetch_optional(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;
    id.ok_or_else(|| ApiError::NotFound("no active trip".to_string()))
}

/// Fetch the `trip_id` of an item row (whitelisted tables only).
async fn trip_id_of(ctx: &Ctx<'_>, table: &str, id: Uuid) -> Result<Uuid, ApiError> {
    let sql = match table {
        "trip_members" => "SELECT trip_id FROM trip_members WHERE id = $1",
        "packing_items" => "SELECT trip_id FROM packing_items WHERE id = $1",
        "expenses" => "SELECT trip_id FROM expenses WHERE id = $1",
        "places" => "SELECT trip_id FROM places WHERE id = $1",
        _ => return Err(ApiError::Internal("bad table".to_string())),
    };
    sqlx::query_scalar(sql)
        .bind(id)
        .fetch_optional(&ctx.state.pool)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound("item not found".to_string()))
}

fn assert_self(ctx: &Ctx<'_>, user_id: &str) -> Result<(), ApiError> {
    if user_id == ctx.sub {
        Ok(())
    } else {
        Err(ApiError::Forbidden)
    }
}

fn trip_property_column(key: &str) -> Option<&'static str> {
    Some(match key {
        "Accommodation Name" => "accommodation_name",
        "Accommodation Address" => "accommodation_address",
        "Check-in Time" => "check_in",
        "Check-out Time" => "check_out",
        "Transport Mode" => "transport_mode",
        "WiFi Network" => "wifi_ssid",
        "WiFi Password" => "wifi_password",
        "Door Code" => "door_code",
        "Notes" => "notes",
        "Hotel URL" => "hotel_url",
        "Airport Arrival Buffer" => "airport_arrival_buffer",
        "Airport to Hotel Time" => "airport_to_hotel_time",
        "Custom Quick Access" => "custom_quick_access",
        "Transport Notes" => "transport_notes",
        "House Rules" => "house_rules",
        "Emergency Contacts" => "emergency_contacts",
        "Hotel Photos" => "hotel_photos",
        "Destination" => "destination",
        "Trip Name" => "name",
        _ => return None,
    })
}

// ---------- row structs + mappers ----------

#[derive(sqlx::FromRow)]
struct TripRow {
    id: Uuid,
    name: String,
    destination: Option<String>,
    start_date: NaiveDate,
    end_date: NaiveDate,
    accommodation_name: Option<String>,
    accommodation_address: Option<String>,
    room_type: Option<String>,
    check_in: Option<String>,
    check_out: Option<String>,
    hotel_phone: Option<String>,
    booking_ref: Option<String>,
    currency: Option<String>,
    cover_image: Option<String>,
    transport_mode: Option<String>,
    wifi_ssid: Option<String>,
    wifi_password: Option<String>,
    door_code: Option<String>,
    notes: Option<String>,
    status: String,
    hotel_url: Option<String>,
    airport_arrival_buffer: Option<String>,
    airport_to_hotel_time: Option<String>,
    custom_quick_access: Option<String>,
    transport_notes: Option<String>,
    house_rules: Option<String>,
    emergency_contacts: Option<String>,
    hotel_photos: Option<String>,
    budget_limit: Option<BigDecimal>,
    budget_mode: Option<String>,
    user_id: Option<String>,
    is_past_import: Option<bool>,
    confidence_level: Option<String>,
    date_precision: Option<String>,
    country: Option<String>,
    country_code: Option<String>,
    latitude: Option<BigDecimal>,
    longitude: Option<BigDecimal>,
    total_spent: Option<BigDecimal>,
    total_nights: Option<BigDecimal>,
}

impl TripRow {
    fn into_trip(self) -> core::types::Trip {
        let nights = (self.end_date - self.start_date).num_days().max(1) as i32;
        core::types::Trip {
            id: self.id,
            name: self.name,
            destination: self.destination,
            startDate: self.start_date,
            endDate: self.end_date,
            nights,
            accommodation: self.accommodation_name,
            address: self.accommodation_address,
            roomType: self.room_type,
            checkIn: self.check_in,
            checkOut: self.check_out,
            hotelPhone: self.hotel_phone,
            bookingRef: self.booking_ref,
            cost: None,
            costCurrency: self.currency,
            transport: self.transport_mode,
            wifiSsid: self.wifi_ssid,
            wifiPassword: self.wifi_password,
            doorCode: self.door_code,
            locationRating: None,
            amenities: None,
            notes: self.notes,
            status: self.status,
            heroImageUrl: self.cover_image,
            hotelUrl: self.hotel_url,
            airportArrivalBuffer: self.airport_arrival_buffer,
            airportToHotelTime: self.airport_to_hotel_time,
            customQuickAccess: self.custom_quick_access,
            transportNotes: self.transport_notes,
            houseRules: self.house_rules,
            emergencyContacts: self.emergency_contacts,
            hotelPhotos: self.hotel_photos,
            budgetLimit: self.budget_limit,
            budgetMode: self.budget_mode,
            userId: self.user_id,
            isPastImport: self.is_past_import,
            confidenceLevel: self.confidence_level,
            datePrecision: self.date_precision,
            country: self.country,
            countryCode: self.country_code,
            latitude: to_f64(self.latitude),
            longitude: to_f64(self.longitude),
            totalSpent: self.total_spent,
            totalNights: to_f64(self.total_nights),
        }
    }
}

#[derive(sqlx::FromRow)]
struct MemberRow {
    id: Uuid,
    name: String,
    role: String,
    user_id: Option<String>,
    phone: Option<String>,
    email: Option<String>,
    avatar_url: Option<String>,
}

impl MemberRow {
    fn into_member(self) -> core::types::GroupMember {
        core::types::GroupMember {
            id: self.id,
            name: self.name,
            role: self.role,
            userId: self.user_id,
            flightId: None,
            checkedBaggage: None,
            phone: self.phone,
            email: self.email,
            profilePhoto: self.avatar_url,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ExpenseRow {
    id: Uuid,
    title: String,
    amount: BigDecimal,
    currency: String,
    category: String,
    expense_date: NaiveDate,
    paid_by: Option<String>,
    photo_url: Option<String>,
    place_name: Option<String>,
    split_type: Option<String>,
    notes: Option<String>,
}

impl ExpenseRow {
    fn into_expense(self) -> core::types::Expense {
        core::types::Expense {
            id: self.id,
            description: self.title,
            amount: self.amount,
            currency: self.currency,
            category: self.category,
            date: self.expense_date,
            paidBy: self.paid_by,
            photo: self.photo_url,
            placeName: self.place_name,
            placeLatitude: None,
            placeLongitude: None,
            splitType: self.split_type,
            notes: self.notes,
        }
    }
}

#[derive(sqlx::FromRow)]
struct PlaceRow {
    id: Uuid,
    name: String,
    category: String,
    distance: Option<String>,
    notes: Option<String>,
    price_estimate: Option<String>,
    rating: Option<i32>,
    source: String,
    vote: String,
    photo_url: Option<String>,
    google_place_id: Option<String>,
    google_maps_uri: Option<String>,
    total_ratings: Option<i32>,
    latitude: Option<BigDecimal>,
    longitude: Option<BigDecimal>,
    saved: bool,
}

impl PlaceRow {
    fn into_place(self) -> core::types::Place {
        core::types::Place {
            id: self.id,
            name: self.name,
            category: self.category,
            distance: self.distance,
            notes: self.notes,
            priceEstimate: self.price_estimate,
            rating: self.rating,
            source: self.source,
            vote: self.vote,
            voteByMember: None,
            photoUrl: self.photo_url,
            googlePlaceId: self.google_place_id,
            latitude: to_f64(self.latitude),
            longitude: to_f64(self.longitude),
            googleMapsUri: self.google_maps_uri,
            totalRatings: self.total_ratings,
            saved: self.saved,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ChecklistRow {
    id: Uuid,
    title: String,
    is_done: bool,
    done_by: Option<String>,
}

impl ChecklistRow {
    fn into_item(self) -> core::types::ChecklistItem {
        core::types::ChecklistItem {
            id: self.id,
            task: self.title,
            done: self.is_done,
            doneBy: self.done_by,
            dueAt: None,
        }
    }
}

#[derive(sqlx::FromRow)]
struct MomentRow {
    id: Uuid,
    caption: String,
    storage_path: Option<String>,
    public_url: Option<String>,
    location: Option<String>,
    uploaded_by: Option<String>,
    taken_at: NaiveDate,
    tags: Vec<String>,
}

impl MomentRow {
    fn into_moment(self) -> core::types::Moment {
        core::types::Moment {
            id: self.id,
            caption: self.caption,
            photo: self.public_url.clone().or(self.storage_path.clone()),
            location: self.location,
            takenBy: self.uploaded_by,
            date: self.taken_at,
            tags: self.tags,
        }
    }
}

#[derive(sqlx::FromRow)]
struct LifetimeStatsRow {
    total_trips: Option<i32>,
    total_countries: Option<i32>,
    total_nights: Option<i32>,
    total_miles: Option<BigDecimal>,
    total_spent: Option<BigDecimal>,
    home_currency: Option<String>,
    total_moments: Option<i32>,
    countries_list: Option<Vec<String>>,
    earliest_trip_date: Option<NaiveDate>,
}

impl LifetimeStatsRow {
    fn into_stats(self) -> core::types::LifetimeStats {
        core::types::LifetimeStats {
            totalTrips: self.total_trips,
            totalCountries: self.total_countries,
            totalNights: self.total_nights,
            totalMiles: to_f64(self.total_miles),
            totalSpent: self.total_spent,
            homeCurrency: self.home_currency,
            totalMoments: self.total_moments,
            countriesList: self.countries_list,
            earliestTripDate: self.earliest_trip_date,
        }
    }
}

// ---------- request bodies ----------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTripBody {
    name: String,
    destination: String,
    start_date: String,
    end_date: String,
    members: Option<Vec<String>>,
    accommodation: Option<String>,
    address: Option<String>,
    check_in: Option<String>,
    check_out: Option<String>,
    room_type: Option<String>,
    booking_ref: Option<String>,
    cost: Option<f64>,
    cost_currency: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddFlightBody {
    direction: String,
    flight_number: String,
    airline: Option<String>,
    from_city: Option<String>,
    to_city: Option<String>,
    depart_time: Option<String>,
    arrive_time: Option<String>,
    booking_ref: Option<String>,
    passenger: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddMemberBody {
    name: String,
    email: Option<String>,
    phone: Option<String>,
    role: Option<String>,
}

#[derive(Deserialize)]
struct KeyBody {
    key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendChatBody {
    sender_name: String,
    sender_avatar: Option<String>,
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddPackingBody {
    item: String,
    category: String,
    owner: Option<String>,
}

#[derive(Deserialize)]
struct PackedBody {
    packed: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddExpenseBody {
    description: String,
    amount: f64,
    currency: String,
    category: String,
    date: String,
    paid_by: Option<String>,
    photo: Option<String>,
    place_name: Option<String>,
    split_type: Option<String>,
    notes: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct UpdateExpenseBody {
    description: Option<String>,
    amount: Option<f64>,
    currency: Option<String>,
    category: Option<String>,
    date: Option<String>,
    paid_by: Option<String>,
    photo: Option<String>,
    place_name: Option<String>,
    split_type: Option<String>,
    notes: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddPlaceBody {
    name: String,
    category: String,
    distance: Option<String>,
    notes: Option<String>,
    price_estimate: Option<String>,
    rating: Option<i32>,
    source: String,
    vote: String,
    photo_url: Option<String>,
    google_place_id: Option<String>,
    google_maps_uri: Option<String>,
    total_ratings: Option<i32>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    saved: Option<bool>,
}

#[derive(Deserialize)]
struct VoteBody {
    vote: String,
}

#[derive(Deserialize)]
struct SaveBody {
    saved: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddMomentBody {
    caption: Option<String>,
    photo: Option<String>,
    location: Option<String>,
    taken_by: Option<String>,
    date: String,
    tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddTripFileBody {
    file_name: String,
    file_url: Option<String>,
    #[serde(rename = "type")]
    file_type: String,
    notes: Option<String>,
    print_required: bool,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct UpdateProfileBody {
    full_name: Option<String>,
    avatar_url: Option<String>,
    phone: Option<String>,
}

#[derive(Deserialize)]
struct EnsureBody {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PresignBody {
    prefix: String,
    content_type: String,
}

// ---------- handlers ----------

pub async fn get_active_trip(ctx: &Ctx<'_>) -> Result<Response<Body>, ApiError> {
    let row: Option<TripRow> = sqlx::query_as(
        "SELECT t.* FROM trips t \
         JOIN trip_members m ON m.trip_id = t.id \
         WHERE m.user_id = $1 AND t.status <> 'Completed' \
         ORDER BY t.start_date DESC LIMIT 1",
    )
    .bind(ctx.sub)
    .fetch_optional(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;

    match row {
        Some(r) => Ok(ok(json!(r.into_trip()))),
        None => Ok(ok(json!(null))),
    }
}

pub async fn create_trip(ctx: &Ctx<'_>, req: &Request) -> Result<Response<Body>, ApiError> {
    let input: CreateTripBody = body(req).await?;
    let start = parse_date_pht(&input.start_date)?;
    let end = parse_date_pht(&input.end_date)?;

    let today = pht_today();
    let status = if today > end {
        "Completed"
    } else if today >= start {
        "Active"
    } else {
        "Planning"
    };

    // Archive this user's prior active trips (single-active-trip model).
    sqlx::query("UPDATE trips SET status = 'Completed' WHERE user_id = $1 AND status IN ('Planning','Active')")
        .bind(ctx.sub)
        .execute(&ctx.state.pool)
        .await
        .map_err(ApiError::from)?;

    let name = if input.name.trim().is_empty() {
        format!("Trip to {}", input.destination)
    } else {
        input.name.clone()
    };

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO trips (name, destination, start_date, end_date, status, user_id, \
         accommodation_name, accommodation_address, check_in, check_out, budget_limit, currency, notes, room_type) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) RETURNING id",
    )
    .bind(name)
    .bind(input.destination)
    .bind(start)
    .bind(end)
    .bind(status)
    .bind(ctx.sub)
    .bind(input.accommodation)
    .bind(input.address)
    .bind(input.check_in)
    .bind(input.check_out)
    .bind(input.cost.map(to_big_decimal))
    .bind(input.cost_currency)
    .bind(input.booking_ref.map(|b| format!("Booking ref: {b}")))
    .bind(input.room_type)
    .fetch_one(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;

    // Organizer name: email prefix, else "Organizer".
    let user_name = ctx
        .email
        .and_then(|e| e.split('@').next())
        .filter(|s| !s.is_empty())
        .unwrap_or("Organizer")
        .to_string();

    sqlx::query("INSERT INTO trip_members (trip_id, name, role, user_id) VALUES ($1,$2,'Primary',$3)")
        .bind(id)
        .bind(&user_name)
        .bind(ctx.sub)
        .execute(&ctx.state.pool)
        .await
        .map_err(ApiError::from)?;

    if let Some(members) = &input.members {
        for m in members {
            let m = m.trim();
            if m.is_empty() || m.eq_ignore_ascii_case(&user_name) {
                continue;
            }
            sqlx::query("INSERT INTO trip_members (trip_id, name, role) VALUES ($1,$2,'Member')")
                .bind(id)
                .bind(m)
                .execute(&ctx.state.pool)
                .await
                .map_err(ApiError::from)?;
        }
    }

    Ok(ok(json!({ "id": id })))
}

pub async fn create_invite_code(ctx: &Ctx<'_>, req: &Request) -> Result<Response<Body>, ApiError> {
    #[derive(Deserialize)]
    struct B {
        #[serde(rename = "tripId")]
        trip_id: Option<String>,
    }
    let b: B = body(req).await?;
    let trip_id = resolve_trip_id(ctx, b.trip_id.as_deref()).await?;
    let code = generate_code();
    let expires = Utc::now() + chrono::Duration::days(7);

    sqlx::query("INSERT INTO trip_invites (trip_id, code, expires_at) VALUES ($1,$2,$3)")
        .bind(trip_id)
        .bind(&code)
        .bind(expires)
        .execute(&ctx.state.pool)
        .await
        .map_err(ApiError::from)?;

    Ok(ok(json!({ "code": code })))
}

pub async fn join_trip_by_code(ctx: &Ctx<'_>, req: &Request) -> Result<Response<Body>, ApiError> {
    #[derive(Deserialize)]
    struct B {
        code: String,
        #[serde(rename = "userName")]
        user_name: String,
    }
    let b: B = body(req).await?;
    let code = b.code.trim().to_uppercase();

    let invite: Option<(Uuid, Option<DateTime<Utc>>, bool)> = sqlx::query_as(
        "SELECT trip_id, expires_at, used FROM trip_invites WHERE code = $1",
    )
    .bind(&code)
    .fetch_optional(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;

    let (trip_id, expires_at, used) =
        invite.ok_or_else(|| ApiError::BadRequest("invalid invite code".to_string()))?;
    if used {
        return Err(ApiError::BadRequest("this invite has already been used".to_string()));
    }
    if let Some(exp) = expires_at {
        if exp < Utc::now() {
            return Err(ApiError::BadRequest("this invite has expired".to_string()));
        }
    }

    let trip_row: Option<TripRow> = sqlx::query_as("SELECT * FROM trips WHERE id = $1")
        .bind(trip_id)
        .fetch_optional(&ctx.state.pool)
        .await
        .map_err(ApiError::from)?;
    let trip = trip_row
        .ok_or_else(|| ApiError::NotFound("trip not found".to_string()))?
        .into_trip();

    sqlx::query("INSERT INTO trip_members (trip_id, name, role, user_id) VALUES ($1,$2,'Member',$3)")
        .bind(trip_id)
        .bind(b.user_name.trim())
        .bind(ctx.sub)
        .execute(&ctx.state.pool)
        .await
        .map_err(ApiError::from)?;

    sqlx::query("UPDATE trip_invites SET used = true WHERE code = $1")
        .bind(&code)
        .execute(&ctx.state.pool)
        .await
        .map_err(ApiError::from)?;

    Ok(ok(json!({ "tripId": trip_id, "trip": trip })))
}

pub async fn get_invites(ctx: &Ctx<'_>, req: &Request) -> Result<Response<Body>, ApiError> {
    let trip_id = resolve_trip_id(ctx, query_param(req, "tripId").as_deref()).await?;
    let invites: Vec<core::types::TripInvite> = sqlx::query_as(
        "SELECT id, code, created_at, expires_at, used FROM trip_invites \
         WHERE trip_id = $1 ORDER BY created_at DESC LIMIT 10",
    )
    .bind(trip_id)
    .fetch_all(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;
    Ok(ok(json!(invites)))
}

pub async fn get_flights(ctx: &Ctx<'_>, trip_id: Uuid) -> Result<Response<Body>, ApiError> {
    ctx.assert_member(trip_id).await?;
    let flights: Vec<core::types::Flight> =
        sqlx::query_as("SELECT * FROM flights WHERE trip_id = $1 ORDER BY depart_time")
            .bind(trip_id)
            .fetch_all(&ctx.state.pool)
            .await
            .map_err(ApiError::from)?;
    Ok(ok(json!(flights)))
}

pub async fn add_flight(
    ctx: &Ctx<'_>,
    trip_id: Uuid,
    req: &Request,
) -> Result<Response<Body>, ApiError> {
    ctx.assert_member(trip_id).await?;
    let b: AddFlightBody = body(req).await?;
    let depart = parse_dt(
        b.depart_time
            .as_deref()
            .ok_or_else(|| ApiError::BadRequest("departTime required".to_string()))?,
    )?;
    let arrive = parse_dt(
        b.arrive_time
            .as_deref()
            .ok_or_else(|| ApiError::BadRequest("arriveTime required".to_string()))?,
    )?;

    sqlx::query(
        "INSERT INTO flights (trip_id, direction, flight_number, airline, from_city, to_city, \
         depart_time, arrive_time, booking_ref, passenger) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
    )
    .bind(trip_id)
    .bind(b.direction)
    .bind(b.flight_number)
    .bind(b.airline)
    .bind(b.from_city)
    .bind(b.to_city)
    .bind(depart)
    .bind(arrive)
    .bind(b.booking_ref)
    .bind(b.passenger)
    .execute(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(ok_body())
}

pub async fn get_group_members(ctx: &Ctx<'_>, trip_id: Uuid) -> Result<Response<Body>, ApiError> {
    ctx.assert_member(trip_id).await?;
    let rows: Vec<MemberRow> = sqlx::query_as(
        "SELECT id, name, role, user_id, phone, email, avatar_url FROM trip_members WHERE trip_id = $1",
    )
    .bind(trip_id)
    .fetch_all(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;
    let out: Vec<core::types::GroupMember> = rows.into_iter().map(MemberRow::into_member).collect();
    Ok(ok(json!(out)))
}

pub async fn add_group_member(
    ctx: &Ctx<'_>,
    trip_id: Uuid,
    req: &Request,
) -> Result<Response<Body>, ApiError> {
    ctx.assert_member(trip_id).await?;
    let b: AddMemberBody = body(req).await?;
    sqlx::query("INSERT INTO trip_members (trip_id, name, role, email, phone) VALUES ($1,$2,$3,$4,$5)")
        .bind(trip_id)
        .bind(b.name.trim())
        .bind(b.role.unwrap_or_else(|| "Member".to_string()))
        .bind(b.email.map(|e| e.trim().to_string()))
        .bind(b.phone.map(|p| p.trim().to_string()))
        .execute(&ctx.state.pool)
        .await
        .map_err(ApiError::from)?;
    Ok(ok_body())
}

pub async fn update_member_photo(
    ctx: &Ctx<'_>,
    member_id: Uuid,
    req: &Request,
) -> Result<Response<Body>, ApiError> {
    let b: KeyBody = body(req).await?;
    let trip_id = trip_id_of(ctx, "trip_members", member_id).await?;
    ctx.assert_member(trip_id).await?;
    sqlx::query("UPDATE trip_members SET avatar_url = $1 WHERE id = $2")
        .bind(b.key)
        .bind(member_id)
        .execute(&ctx.state.pool)
        .await
        .map_err(ApiError::from)?;
    Ok(ok_body())
}

pub async fn update_member_email(
    ctx: &Ctx<'_>,
    member_id: Uuid,
    req: &Request,
) -> Result<Response<Body>, ApiError> {
    #[derive(Deserialize)]
    struct B {
        email: String,
    }
    let b: B = body(req).await?;
    let trip_id = trip_id_of(ctx, "trip_members", member_id).await?;
    ctx.assert_member(trip_id).await?;
    sqlx::query("UPDATE trip_members SET email = $1 WHERE id = $2")
        .bind(b.email)
        .bind(member_id)
        .execute(&ctx.state.pool)
        .await
        .map_err(ApiError::from)?;
    Ok(ok_body())
}

pub async fn update_member_phone(
    ctx: &Ctx<'_>,
    member_id: Uuid,
    req: &Request,
) -> Result<Response<Body>, ApiError> {
    #[derive(Deserialize)]
    struct B {
        phone: String,
    }
    let b: B = body(req).await?;
    let trip_id = trip_id_of(ctx, "trip_members", member_id).await?;
    ctx.assert_member(trip_id).await?;
    sqlx::query("UPDATE trip_members SET phone = $1 WHERE id = $2")
        .bind(b.phone)
        .bind(member_id)
        .execute(&ctx.state.pool)
        .await
        .map_err(ApiError::from)?;
    Ok(ok_body())
}

pub async fn get_chat_messages(ctx: &Ctx<'_>, trip_id: Uuid) -> Result<Response<Body>, ApiError> {
    ctx.assert_member(trip_id).await?;
    let msgs: Vec<core::types::ChatMessage> = sqlx::query_as(
        "SELECT id, trip_id, sender_name, sender_avatar, message, created_at \
         FROM chat_messages WHERE trip_id = $1 ORDER BY created_at ASC LIMIT 200",
    )
    .bind(trip_id)
    .fetch_all(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;
    Ok(ok(json!(msgs)))
}

pub async fn send_chat_message(
    ctx: &Ctx<'_>,
    trip_id: Uuid,
    req: &Request,
) -> Result<Response<Body>, ApiError> {
    ctx.assert_member(trip_id).await?;
    let b: SendChatBody = body(req).await?;
    sqlx::query(
        "INSERT INTO chat_messages (trip_id, sender_name, sender_avatar, message) VALUES ($1,$2,$3,$4)",
    )
    .bind(trip_id)
    .bind(b.sender_name)
    .bind(b.sender_avatar)
    .bind(b.message.trim())
    .execute(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;
    Ok(ok_body())
}

pub async fn get_packing_list(ctx: &Ctx<'_>, trip_id: Uuid) -> Result<Response<Body>, ApiError> {
    ctx.assert_member(trip_id).await?;
    let items: Vec<core::types::PackingItem> = sqlx::query_as(
        "SELECT id, name, category, is_packed, owner FROM packing_items WHERE trip_id = $1",
    )
    .bind(trip_id)
    .fetch_all(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;
    Ok(ok(json!(items)))
}

pub async fn add_packing_item(
    ctx: &Ctx<'_>,
    trip_id: Uuid,
    req: &Request,
) -> Result<Response<Body>, ApiError> {
    ctx.assert_member(trip_id).await?;
    let b: AddPackingBody = body(req).await?;
    sqlx::query(
        "INSERT INTO packing_items (trip_id, name, category, is_packed, owner) VALUES ($1,$2,$3,false,$4)",
    )
    .bind(trip_id)
    .bind(b.item)
    .bind(b.category)
    .bind(b.owner)
    .execute(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;
    Ok(ok_body())
}

pub async fn toggle_packed(
    ctx: &Ctx<'_>,
    item_id: Uuid,
    req: &Request,
) -> Result<Response<Body>, ApiError> {
    let b: PackedBody = body(req).await?;
    let trip_id = trip_id_of(ctx, "packing_items", item_id).await?;
    ctx.assert_member(trip_id).await?;
    sqlx::query("UPDATE packing_items SET is_packed = $1 WHERE id = $2")
        .bind(b.packed)
        .bind(item_id)
        .execute(&ctx.state.pool)
        .await
        .map_err(ApiError::from)?;
    Ok(ok_body())
}

pub async fn get_expenses(ctx: &Ctx<'_>, trip_id: Uuid) -> Result<Response<Body>, ApiError> {
    ctx.assert_member(trip_id).await?;
    let rows: Vec<ExpenseRow> = sqlx::query_as(
        "SELECT id, title, amount, currency, category, expense_date, paid_by, photo_url, \
         place_name, split_type, notes FROM expenses WHERE trip_id = $1 ORDER BY expense_date DESC",
    )
    .bind(trip_id)
    .fetch_all(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;
    let out: Vec<core::types::Expense> = rows.into_iter().map(ExpenseRow::into_expense).collect();
    Ok(ok(json!(out)))
}

pub async fn add_expense(
    ctx: &Ctx<'_>,
    trip_id: Uuid,
    req: &Request,
) -> Result<Response<Body>, ApiError> {
    ctx.assert_member(trip_id).await?;
    let b: AddExpenseBody = body(req).await?;
    let date = parse_date_pht(&b.date)?;
    sqlx::query(
        "INSERT INTO expenses (trip_id, title, amount, currency, category, expense_date, \
         paid_by, photo_url, place_name, split_type, notes) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
    )
    .bind(trip_id)
    .bind(b.description)
    .bind(to_big_decimal(b.amount))
    .bind(b.currency)
    .bind(b.category)
    .bind(date)
    .bind(b.paid_by)
    .bind(b.photo)
    .bind(b.place_name)
    .bind(b.split_type)
    .bind(b.notes)
    .execute(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;
    Ok(ok_body())
}

pub async fn update_expense(
    ctx: &Ctx<'_>,
    expense_id: Uuid,
    req: &Request,
) -> Result<Response<Body>, ApiError> {
    let b: UpdateExpenseBody = body(req).await?;
    let trip_id = trip_id_of(ctx, "expenses", expense_id).await?;
    ctx.assert_member(trip_id).await?;
    let date = b.date.as_deref().map(parse_date_pht).transpose()?;

    sqlx::query(
        "UPDATE expenses SET title = COALESCE($1, title), amount = COALESCE($2, amount), \
         currency = COALESCE($3, currency), category = COALESCE($4, category), \
         expense_date = COALESCE($5, expense_date), paid_by = COALESCE($6, paid_by), \
         photo_url = COALESCE($7, photo_url), place_name = COALESCE($8, place_name), \
         split_type = COALESCE($9, split_type), notes = COALESCE($10, notes) WHERE id = $11",
    )
    .bind(b.description)
    .bind(b.amount.map(to_big_decimal))
    .bind(b.currency)
    .bind(b.category)
    .bind(date)
    .bind(b.paid_by)
    .bind(b.photo)
    .bind(b.place_name)
    .bind(b.split_type)
    .bind(b.notes)
    .bind(expense_id)
    .execute(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;
    Ok(ok_body())
}

pub async fn delete_expense(ctx: &Ctx<'_>, expense_id: Uuid) -> Result<Response<Body>, ApiError> {
    let trip_id = trip_id_of(ctx, "expenses", expense_id).await?;
    ctx.assert_member(trip_id).await?;
    sqlx::query("DELETE FROM expenses WHERE id = $1")
        .bind(expense_id)
        .execute(&ctx.state.pool)
        .await
        .map_err(ApiError::from)?;
    Ok(ok_body())
}

pub async fn get_expense_summary(ctx: &Ctx<'_>, trip_id: Uuid) -> Result<Response<Body>, ApiError> {
    ctx.assert_member(trip_id).await?;
    let rows: Vec<(BigDecimal, String)> =
        sqlx::query_as("SELECT amount, category FROM expenses WHERE trip_id = $1")
            .bind(trip_id)
            .fetch_all(&ctx.state.pool)
            .await
            .map_err(ApiError::from)?;

    let mut by_category: BTreeMap<String, f64> = BTreeMap::new();
    let mut total = 0f64;
    let mut count = 0usize;
    for (amount, cat) in rows {
        let a = to_f64(Some(amount)).unwrap_or(0.0);
        *by_category.entry(cat).or_insert(0.0) += a;
        total += a;
        count += 1;
    }

    Ok(ok(json!({ "total": total, "byCategory": by_category, "count": count })))
}

pub async fn get_saved_places(ctx: &Ctx<'_>, trip_id: Uuid) -> Result<Response<Body>, ApiError> {
    ctx.assert_member(trip_id).await?;
    let rows: Vec<PlaceRow> = sqlx::query_as(
        "SELECT id, name, category, distance, notes, price_estimate, rating, source, vote, \
         photo_url, google_place_id, google_maps_uri, total_ratings, latitude, longitude, saved \
         FROM places WHERE trip_id = $1",
    )
    .bind(trip_id)
    .fetch_all(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;
    let out: Vec<core::types::Place> = rows.into_iter().map(PlaceRow::into_place).collect();
    Ok(ok(json!(out)))
}

pub async fn add_place(
    ctx: &Ctx<'_>,
    trip_id: Uuid,
    req: &Request,
) -> Result<Response<Body>, ApiError> {
    ctx.assert_member(trip_id).await?;
    let b: AddPlaceBody = body(req).await?;
    sqlx::query(
        "INSERT INTO places (trip_id, name, category, distance, notes, price_estimate, rating, \
         source, vote, photo_url, google_place_id, google_maps_uri, total_ratings, latitude, longitude, saved) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)",
    )
    .bind(trip_id)
    .bind(b.name)
    .bind(b.category)
    .bind(b.distance)
    .bind(b.notes)
    .bind(b.price_estimate)
    .bind(b.rating)
    .bind(b.source)
    .bind(b.vote)
    .bind(b.photo_url)
    .bind(b.google_place_id)
    .bind(b.google_maps_uri)
    .bind(b.total_ratings)
    .bind(b.latitude.map(to_big_decimal))
    .bind(b.longitude.map(to_big_decimal))
    .bind(b.saved.unwrap_or(true))
    .execute(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;
    Ok(ok_body())
}

pub async fn vote_on_place(
    ctx: &Ctx<'_>,
    place_id: Uuid,
    req: &Request,
) -> Result<Response<Body>, ApiError> {
    let b: VoteBody = body(req).await?;
    let trip_id = trip_id_of(ctx, "places", place_id).await?;
    ctx.assert_member(trip_id).await?;
    sqlx::query("UPDATE places SET vote = $1 WHERE id = $2")
        .bind(b.vote)
        .bind(place_id)
        .execute(&ctx.state.pool)
        .await
        .map_err(ApiError::from)?;
    Ok(ok_body())
}

pub async fn save_place(
    ctx: &Ctx<'_>,
    place_id: Uuid,
    req: &Request,
) -> Result<Response<Body>, ApiError> {
    let b: SaveBody = body(req).await?;
    let trip_id = trip_id_of(ctx, "places", place_id).await?;
    ctx.assert_member(trip_id).await?;
    sqlx::query("UPDATE places SET saved = $1 WHERE id = $2")
        .bind(b.saved)
        .bind(place_id)
        .execute(&ctx.state.pool)
        .await
        .map_err(ApiError::from)?;
    Ok(ok_body())
}

pub async fn get_checklist(ctx: &Ctx<'_>, trip_id: Uuid) -> Result<Response<Body>, ApiError> {
    ctx.assert_member(trip_id).await?;
    let rows: Vec<ChecklistRow> = sqlx::query_as(
        "SELECT id, title, is_done, done_by FROM checklist_items WHERE trip_id = $1 ORDER BY sort_order ASC",
    )
    .bind(trip_id)
    .fetch_all(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;
    let out: Vec<core::types::ChecklistItem> = rows.into_iter().map(ChecklistRow::into_item).collect();
    Ok(ok(json!(out)))
}

pub async fn get_moments(ctx: &Ctx<'_>, trip_id: Uuid) -> Result<Response<Body>, ApiError> {
    ctx.assert_member(trip_id).await?;
    let rows: Vec<MomentRow> = sqlx::query_as(
        "SELECT id, caption, storage_path, public_url, location, uploaded_by, taken_at, tags \
         FROM moments WHERE trip_id = $1 ORDER BY taken_at DESC",
    )
    .bind(trip_id)
    .fetch_all(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;
    let out: Vec<core::types::Moment> = rows.into_iter().map(MomentRow::into_moment).collect();
    Ok(ok(json!(out)))
}

pub async fn add_moment(
    ctx: &Ctx<'_>,
    trip_id: Uuid,
    req: &Request,
) -> Result<Response<Body>, ApiError> {
    ctx.assert_member(trip_id).await?;
    let b: AddMomentBody = body(req).await?;
    let date = parse_date_pht(&b.date)?;
    sqlx::query(
        "INSERT INTO moments (trip_id, caption, public_url, location, uploaded_by, taken_at, tags) \
         VALUES ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(trip_id)
    .bind(b.caption.unwrap_or_else(|| "Untitled".to_string()))
    .bind(b.photo)
    .bind(b.location)
    .bind(b.taken_by)
    .bind(date)
    .bind(b.tags.unwrap_or_default())
    .execute(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;
    Ok(ok_body())
}

pub async fn get_trip_files(ctx: &Ctx<'_>, trip_id: Uuid) -> Result<Response<Body>, ApiError> {
    ctx.assert_member(trip_id).await?;
    let files: Vec<core::types::TripFile> = sqlx::query_as(
        "SELECT id, name, file_url, file_type, description, print_required FROM trip_files WHERE trip_id = $1",
    )
    .bind(trip_id)
    .fetch_all(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;
    Ok(ok(json!(files)))
}

pub async fn add_trip_file(
    ctx: &Ctx<'_>,
    trip_id: Uuid,
    req: &Request,
) -> Result<Response<Body>, ApiError> {
    ctx.assert_member(trip_id).await?;
    let b: AddTripFileBody = body(req).await?;
    sqlx::query(
        "INSERT INTO trip_files (trip_id, name, file_url, file_type, description, print_required) \
         VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(trip_id)
    .bind(b.file_name)
    .bind(b.file_url)
    .bind(b.file_type)
    .bind(b.notes)
    .bind(b.print_required)
    .execute(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;
    Ok(ok_body())
}

pub async fn update_trip_property(
    ctx: &Ctx<'_>,
    trip_id: Uuid,
    req: &Request,
) -> Result<Response<Body>, ApiError> {
    ctx.assert_member(trip_id).await?;
    #[derive(Deserialize)]
    struct B {
        key: String,
        value: String,
    }
    let b: B = body(req).await?;
    let column = trip_property_column(&b.key)
        .ok_or_else(|| ApiError::BadRequest(format!("unknown property key: {}", b.key)))?;

    // `column` is from a whitelist — safe to interpolate.
    let sql = format!("UPDATE trips SET {column} = $1 WHERE id = $2");
    sqlx::query(&sql)
        .bind(b.value)
        .bind(trip_id)
        .execute(&ctx.state.pool)
        .await
        .map_err(ApiError::from)?;
    Ok(ok_body())
}

pub async fn update_trip_budget_mode(
    ctx: &Ctx<'_>,
    trip_id: Uuid,
    req: &Request,
) -> Result<Response<Body>, ApiError> {
    ctx.assert_member(trip_id).await?;
    #[derive(Deserialize)]
    struct B {
        mode: String,
    }
    let b: B = body(req).await?;
    sqlx::query("UPDATE trips SET budget_mode = $1 WHERE id = $2")
        .bind(b.mode)
        .bind(trip_id)
        .execute(&ctx.state.pool)
        .await
        .map_err(ApiError::from)?;
    Ok(ok_body())
}

pub async fn update_trip_budget_limit(
    ctx: &Ctx<'_>,
    trip_id: Uuid,
    req: &Request,
) -> Result<Response<Body>, ApiError> {
    ctx.assert_member(trip_id).await?;
    #[derive(Deserialize)]
    struct B {
        limit: f64,
    }
    let b: B = body(req).await?;
    sqlx::query("UPDATE trips SET budget_limit = $1 WHERE id = $2")
        .bind(to_big_decimal(b.limit))
        .bind(trip_id)
        .execute(&ctx.state.pool)
        .await
        .map_err(ApiError::from)?;
    Ok(ok_body())
}

pub async fn get_profile(ctx: &Ctx<'_>, user_id: &str) -> Result<Response<Body>, ApiError> {
    assert_self(ctx, user_id)?;
    let p: Option<core::types::Profile> =
        sqlx::query_as("SELECT id, full_name, avatar_url, phone FROM profiles WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&ctx.state.pool)
            .await
            .map_err(ApiError::from)?;
    match p {
        Some(p) => Ok(ok(json!(p))),
        None => Ok(ok(json!(null))),
    }
}

pub async fn update_profile(
    ctx: &Ctx<'_>,
    user_id: &str,
    req: &Request,
) -> Result<Response<Body>, ApiError> {
    assert_self(ctx, user_id)?;
    let b: UpdateProfileBody = body(req).await?;
    sqlx::query(
        "UPDATE profiles SET full_name = COALESCE($1, full_name), \
         avatar_url = COALESCE($2, avatar_url), phone = COALESCE($3, phone) WHERE id = $4",
    )
    .bind(b.full_name)
    .bind(b.avatar_url)
    .bind(b.phone)
    .bind(user_id)
    .execute(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;
    Ok(ok_body())
}

pub async fn ensure_profile(ctx: &Ctx<'_>, req: &Request) -> Result<Response<Body>, ApiError> {
    let b: EnsureBody = body(req).await?;
    sqlx::query("INSERT INTO profiles (id, full_name) VALUES ($1, $2) ON CONFLICT (id) DO NOTHING")
        .bind(ctx.sub)
        .bind(b.name)
        .execute(&ctx.state.pool)
        .await
        .map_err(ApiError::from)?;
    Ok(ok_body())
}

pub async fn get_lifetime_stats(ctx: &Ctx<'_>) -> Result<Response<Body>, ApiError> {
    let row: Option<LifetimeStatsRow> = sqlx::query_as(
        "SELECT total_trips, total_countries, total_nights, total_miles, total_spent, \
         home_currency, total_moments, countries_list, earliest_trip_date \
         FROM lifetime_stats WHERE user_id = $1",
    )
    .bind(ctx.sub)
    .fetch_optional(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;

    match row {
        Some(r) => Ok(ok(json!(r.into_stats()))),
        None => Ok(ok(json!(null))),
    }
}

pub async fn get_highlights(ctx: &Ctx<'_>) -> Result<Response<Body>, ApiError> {
    let highlights: Vec<core::types::Highlight> = sqlx::query_as(
        "SELECT id, type, display_text, supporting_data, rank FROM highlights \
         WHERE user_id = $1 ORDER BY rank ASC",
    )
    .bind(ctx.sub)
    .fetch_all(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;
    Ok(ok(json!(highlights)))
}

pub async fn get_past_trips(ctx: &Ctx<'_>) -> Result<Response<Body>, ApiError> {
    let rows: Vec<TripRow> = sqlx::query_as(
        "SELECT * FROM trips WHERE user_id = $1 \
         AND (is_past_import = true OR status = 'Completed') ORDER BY start_date DESC",
    )
    .bind(ctx.sub)
    .fetch_all(&ctx.state.pool)
    .await
    .map_err(ApiError::from)?;
    let out: Vec<core::types::Trip> = rows.into_iter().map(TripRow::into_trip).collect();
    Ok(ok(json!(out)))
}

pub async fn presign(ctx: &Ctx<'_>, req: &Request) -> Result<Response<Body>, ApiError> {
    use aws_sdk_s3::presigning::PresigningConfig;
    use std::time::Duration;

    let b: PresignBody = body(req).await?;
    let ext = extension_for(&b.content_type);
    let key = format!(
        "{}/{}.{}",
        b.prefix.trim_end_matches('/'),
        uuid::Uuid::new_v4(),
        ext
    );

    let presigned = ctx
        .state
        .s3
        .put_object()
        .bucket(&ctx.state.media_bucket)
        .key(&key)
        .content_type(&b.content_type)
        .presigned(
            PresigningConfig::expires_in(Duration::from_secs(300))
                .map_err(|e| ApiError::Internal(format!("presign config: {e}")))?,
        )
        .await
        .map_err(|e| ApiError::Internal(format!("presign failed: {e}")))?;

    Ok(ok(json!({
        "uploadUrl": presigned.uri().to_string(),
        "key": key,
    })))
}

fn extension_for(content_type: &str) -> &'static str {
    match content_type {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/png" => "png",
        "image/heic" | "image/heif" => "heic",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "video/mp4" => "mp4",
        "video/quicktime" => "mov",
        "application/pdf" => "pdf",
        _ => "bin",
    }
}
