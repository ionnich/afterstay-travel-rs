//! Anthropic (Claude) integration: recommendations, itinerary, receipt scan,
//! and trip-document scan. Mirrors `lib/anthropic.ts` exactly.

use serde::{Deserialize, Serialize};

use core::error::ApiError;

const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const MODEL: &str = "claude-sonnet-4-20250514";

// ── Output types (JSON wire format == lib/types.ts / lib/anthropic.ts) ─────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIRecommendation {
    pub name: String,
    pub category: String,
    pub distance: String,
    #[serde(rename = "price_estimate")]
    pub price_estimate: String,
    pub reason: String,
    pub rating: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItineraryActivity {
    pub name: String,
    pub category: String,
    pub time_slot: String,
    pub duration: String,
    pub cost: String,
    pub tip: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItineraryDay {
    pub day: i32,
    pub date: String,
    pub theme: String,
    pub activities: Vec<ItineraryActivity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptLineItem {
    pub name: String,
    pub qty: i32,
    pub amount: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedReceipt {
    pub place_name: String,
    pub description: String,
    pub amount: f64,
    pub currency: String,
    pub category: String,
    pub date: String,
    pub items: Vec<ReceiptLineItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedFlight {
    pub direction: String,
    pub flight_number: String,
    pub airline: Option<String>,
    #[serde(rename = "from")]
    pub from: String,
    #[serde(rename = "to")]
    pub to: String,
    pub depart_time: String,
    pub arrive_time: String,
    pub booking_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedTripDetails {
    pub destination: String,
    pub start_date: String,
    pub end_date: String,
    pub accommodation: Option<String>,
    pub address: Option<String>,
    pub check_in: Option<String>,
    pub check_out: Option<String>,
    pub room_type: Option<String>,
    pub booking_ref: Option<String>,
    pub cost: Option<f64>,
    pub cost_currency: Option<String>,
    pub flights: Option<Vec<ScannedFlight>>,
    pub members: Option<Vec<String>>,
}

// ── Input types ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendationsArgs {
    pub first_time: String,
    pub interests: Vec<String>,
    pub trip: Option<TripCtx>,
    pub group_size: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TripCtx {
    pub destination: Option<String>,
    pub accommodation: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub nights: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItineraryArgs {
    pub scope: String,
    pub pace: String,
    pub interests: Vec<String>,
    pub trip_days: Option<i32>,
    pub start_date: Option<String>,
    pub destination: Option<String>,
    pub hotel_name: Option<String>,
    pub group_size: Option<u32>,
    pub budget: Option<f64>,
    pub budget_currency: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptRequest {
    pub base64_image: String,
    #[serde(default = "default_mime")]
    pub mime_type: String,
}

fn default_mime() -> String {
    "image/jpeg".to_string()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanTripRequest {
    pub images: Vec<ImageInput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageInput {
    pub base64: String,
    pub mime_type: String,
}

// ── Recommendations ────────────────────────────────────────────────────────

pub async fn generate_recommendations(
    client: &reqwest::Client,
    key: &str,
    args: &RecommendationsArgs,
) -> Result<Vec<AIRecommendation>, ApiError> {
    let user_msg = format!(
        "Visitor profile: {}.\nInterests (Top 5 per category please): {}.\nReturn ONLY a single JSON array, no prose, no code fences.",
        args.first_time,
        args.interests.join(", ")
    );
    let payload = serde_json::json!({
        "model": MODEL,
        "max_tokens": 4096,
        "system": build_recommendation_prompt(args.trip.as_ref(), args.group_size.unwrap_or(2)),
        "messages": [{ "role": "user", "content": user_msg }],
    });
    let value = call_claude(client, key, payload).await?;
    let text = claude_text(&value);
    let json = extract_json_array(text)?;
    serde_json::from_value(json).map_err(ApiError::from)
}

fn build_recommendation_prompt(trip: Option<&TripCtx>, group_size: u32) -> String {
    let dest = trip
        .and_then(|t| t.destination.as_deref())
        .unwrap_or("your destination");
    let hotel = trip
        .and_then(|t| t.accommodation.as_deref())
        .unwrap_or("your hotel");
    let dates = match trip {
        Some(t) if t.start_date.is_some() && t.end_date.is_some() => format!(
            "{} to {}",
            t.start_date.as_deref().unwrap(),
            t.end_date.as_deref().unwrap()
        ),
        _ => "your trip dates".to_string(),
    };
    let nights = trip.and_then(|t| t.nights).unwrap_or(7);

    let template = r#"You are a local travel expert for {dest}. The user is staying at {hotel} for {nights} nights ({dates}). Generate a Top 5 list for each selected interest category.

For each recommendation include:
- Name
- Category
- Distance from {hotel} (approximate)
- Price estimate in local currency
- One-line reason why
- Rating (1-5 stars)

Return as JSON array. Format:
[
  {
    "name": "Place Name",
    "category": "Eat",
    "distance": "1.2 km",
    "price_estimate": "500-800/person",
    "reason": "Best local cuisine with great atmosphere",
    "rating": 5
  }
]

Group of {group_size} travelers. Mix popular tourist spots with hidden gems locals know."#;

    template
        .replace("{dest}", dest)
        .replace("{hotel}", hotel)
        .replace("{nights}", &nights.to_string())
        .replace("{dates}", &dates)
        .replace("{group_size}", &group_size.to_string())
}

// ── Itinerary ──────────────────────────────────────────────────────────────

pub async fn generate_itinerary(
    client: &reqwest::Client,
    key: &str,
    args: &ItineraryArgs,
) -> Result<Vec<ItineraryDay>, ApiError> {
    let pace_desc = match args.pace.as_str() {
        "relaxed" => "Relaxed pace: 2-3 activities/day, plenty of free time and rest.",
        "packed" => "Packed schedule: 5-6 activities/day, maximize every hour.",
        _ => "Moderate pace: 3-4 activities/day, balanced with downtime.",
    };

    let num_days = if args.scope == "today" {
        1
    } else {
        args.trip_days.unwrap_or(7)
    };
    let today_str = short_date(utc_today());
    let start_date_str = args
        .start_date
        .as_deref()
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .map(short_date)
        .unwrap_or_else(|| today_str.clone());

    let date_info = if args.scope == "today" {
        format!(
            "Plan for TODAY only (1 day). Today is {}. Current time of day: {}.",
            today_str,
            time_of_day()
        )
    } else {
        format!(
            "{} days, starting {}. Use correct calendar dates for each day.",
            num_days, start_date_str
        )
    };

    let surprise_note = if args.scope == "surprise" {
        "\nSurprise the user — pick a random mix of popular and offbeat activities. Vary the theme each day."
    } else {
        ""
    };

    let mut parts: Vec<String> = vec![
        format!("Pace: {pace_desc}"),
        format!("Interests: {}.", args.interests.join(", ")),
        date_info,
    ];
    if !surprise_note.is_empty() {
        parts.push(surprise_note.to_string());
    }
    parts.push("Return ONLY a JSON array, no prose, no code fences.".to_string());
    let user_msg = parts.join("\n");

    let payload = serde_json::json!({
        "model": MODEL,
        "max_tokens": if args.scope == "today" { 1024 } else { 4096 },
        "system": build_itinerary_system(args),
        "messages": [{ "role": "user", "content": user_msg }],
    });
    let value = call_claude(client, key, payload).await?;
    let text = claude_text(&value);
    let json = extract_json_array(text)?;
    serde_json::from_value(json).map_err(ApiError::from)
}

fn build_itinerary_system(args: &ItineraryArgs) -> String {
    let dest = args
        .destination
        .clone()
        .unwrap_or_else(|| "the destination".to_string());
    let hotel = args
        .hotel_name
        .as_ref()
        .map(|h| format!("The user is staying at {h}."))
        .unwrap_or_default();
    let group = args
        .group_size
        .map(|g| format!("Group of {g} travelers."))
        .unwrap_or_default();
    let budget_line = match args.budget {
        Some(b) => format!(
            "Trip budget: {} {}. Keep daily costs within a reasonable share of this.",
            args.budget_currency
                .clone()
                .unwrap_or_else(|| "PHP".to_string()),
            format_thousands(b)
        ),
        None => "No budget limit — but still be practical with costs.".to_string(),
    };

    let template = r#"You are a local travel expert for {dest}. {hotel} {group}
{budget_line}

Return ONLY a JSON array of day objects (no prose, no code fences):
[
  {
    "day": 1,
    "date": "Apr 21",
    "theme": "Arrival & Beach Day",
    "activities": [
      {
        "name": "Place or Activity Name",
        "category": "Food|Beach|Activity|Culture|Nightlife|Wellness|Shopping|Transport",
        "timeSlot": "morning|afternoon|evening",
        "duration": "1-2 hrs",
        "cost": "₱500-800",
        "tip": "Go early to avoid crowds",
        "description": "Why this is worth it in one sentence"
      }
    ]
  }
]

Rules:
- Use the EXACT dates provided in the user message. The "date" field must match the actual calendar date for each day.
- Relaxed pace: 2-3 activities per day. Moderate: 3-4. Packed: 5-6.
- Order activities by time within each day (morning→afternoon→evening).
- Include at least one food recommendation per day.
- Mix popular tourist spots with hidden gems locals know.
- Each activity must have a real place name (not generic like "the beach").
- Cost should be specific local currency ranges, or "Free"."#;

    template
        .replace("{dest}", &dest)
        .replace("{hotel}", &hotel)
        .replace("{group}", &group)
        .replace("{budget_line}", &budget_line)
}

// ── Receipt scan ───────────────────────────────────────────────────────────

pub async fn scan_receipt(
    client: &reqwest::Client,
    key: &str,
    req: &ReceiptRequest,
) -> Result<ScannedReceipt, ApiError> {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let prompt = RECEIPT_PROMPT.replace("{today}", &today);

    let payload = serde_json::json!({
        "model": MODEL,
        "max_tokens": 1024,
        "messages": [{
            "role": "user",
            "content": [
                image_block(&req.base64_image, &req.mime_type),
                { "type": "text", "text": prompt },
            ],
        }],
    });
    let value = call_claude(client, key, payload).await?;
    let text = claude_text(&value);
    let json = extract_json_object(text, "Could not parse receipt data.")?;
    serde_json::from_value(json).map_err(ApiError::from)
}

const RECEIPT_PROMPT: &str = r#"Extract receipt information from this image. Read every line item with its quantity and price. Return ONLY a JSON object (no prose, no code fences) with these fields:
{
  "placeName": "store/restaurant name",
  "description": "brief summary, e.g. 'Lunch for 3'",
  "amount": 123.45,
  "currency": "PHP",
  "category": "Food|Transport|Activity|Accommodation|Shopping|Other",
  "date": "YYYY-MM-DD",
  "items": [
    { "name": "Chicken Adobo", "qty": 2, "amount": 350 },
    { "name": "Rice", "qty": 3, "amount": 75 }
  ]
}
Rules:
- "amount" is the receipt TOTAL (sum of all items + tax/service charge if shown).
- Each item in "items" has name, qty (default 1), and amount (unit price × qty).
- Include tax/service charge as a separate item if shown.
- If you cannot read a field, use reasonable defaults.
- Default currency to PHP. Default date to today: {today}."#;

// ── Trip document scan ─────────────────────────────────────────────────────

pub async fn scan_trip_documents(
    client: &reqwest::Client,
    key: &str,
    req: &ScanTripRequest,
) -> Result<ScannedTripDetails, ApiError> {
    let mut content: Vec<serde_json::Value> = req
        .images
        .iter()
        .map(|img| image_block(&img.base64, &img.mime_type))
        .collect();
    content.push(serde_json::json!({ "type": "text", "text": SCAN_TRIP_PROMPT }));

    let payload = serde_json::json!({
        "model": MODEL,
        "max_tokens": 2048,
        "messages": [{ "role": "user", "content": content }],
    });
    let value = call_claude(client, key, payload).await?;
    let text = claude_text(&value);
    let json = extract_json_object(text, "Could not parse trip details.")?;
    serde_json::from_value(json).map_err(ApiError::from)
}

const SCAN_TRIP_PROMPT: &str = r#"Extract trip details from these screenshots. They may be flight bookings, hotel confirmations, itineraries, or general trip screenshots.

Return ONLY a JSON object (no prose, no code fences):
{
  "destination": "City, Country",
  "startDate": "YYYY-MM-DD",
  "endDate": "YYYY-MM-DD",
  "accommodation": "Hotel name",
  "address": "Hotel address",
  "checkIn": "3:00 PM",
  "checkOut": "12:00 PM",
  "roomType": "Deluxe King",
  "bookingRef": "ABC123",
  "cost": 15000,
  "costCurrency": "PHP",
  "flights": [
    {
      "direction": "Outbound",
      "flightNumber": "BA 276",
      "airline": "British Airways",
      "from": "New York (JFK)",
      "to": "Paris (CDG)",
      "departTime": "2026-04-20T06:00:00-04:00",
      "arriveTime": "2026-04-20T07:05:00+02:00",
      "bookingRef": "XYZ789"
    }
  ],
  "members": ["Peter", "Jane"]
}

Rules:
- Extract as much as you can from the images. Leave fields empty/null if not found.
- Dates must be YYYY-MM-DD format. Times must include a timezone offset (e.g. -04:00, +09:00).
- If multiple flights found, include all of them with correct direction (Outbound or Return).
- If you see passenger names, list them in "members".
- Cost should be numeric (no currency symbol). Currency as ISO code.
- Default currency to PHP if not specified."#;

// ── Helpers ────────────────────────────────────────────────────────────────

fn image_block(base64: &str, mime_type: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "image",
        "source": {
            "type": "base64",
            "media_type": mime_type,
            "data": base64,
        },
    })
}

async fn call_claude(
    client: &reqwest::Client,
    key: &str,
    payload: serde_json::Value,
) -> Result<serde_json::Value, ApiError> {
    let res = client
        .post(ANTHROPIC_URL)
        .header("x-api-key", key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("content-type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("anthropic request failed: {e}")))?;

    let status = res.status();
    let body = res.text().await.unwrap_or_default();
    if !status.is_success() {
        if body.contains("credit balance is too low") {
            return Err(ApiError::Internal(
                "Anthropic API credits exhausted. Please add credits at console.anthropic.com → Plans & Billing."
                    .to_string(),
            ));
        }
        return Err(ApiError::Internal(format!("Anthropic {status}: {body}")));
    }
    serde_json::from_str::<serde_json::Value>(&body).map_err(ApiError::from)
}

fn claude_text(value: &serde_json::Value) -> &str {
    value
        .get("content")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
}

/// Strip ```json … ``` fences, then extract the JSON array between the first
/// `[` and last `]`.
fn extract_json_array(text: &str) -> Result<serde_json::Value, ApiError> {
    let candidate = strip_fences(text);
    let first = candidate
        .find('[')
        .ok_or_else(|| ApiError::Internal("No JSON array found in AI response.".to_string()))?;
    let last = candidate
        .rfind(']')
        .ok_or_else(|| ApiError::Internal("No JSON array found in AI response.".to_string()))?;
    serde_json::from_str(&candidate[first..=last]).map_err(ApiError::from)
}

/// Strip fences, then extract the JSON object between the first `{` and last `}`.
fn extract_json_object(text: &str, err: &str) -> Result<serde_json::Value, ApiError> {
    let candidate = strip_fences(text);
    let first = candidate
        .find('{')
        .ok_or_else(|| ApiError::Internal(err.to_string()))?;
    let last = candidate
        .rfind('}')
        .ok_or_else(|| ApiError::Internal(err.to_string()))?;
    serde_json::from_str(&candidate[first..=last]).map_err(ApiError::from)
}

fn strip_fences(text: &str) -> String {
    if let Some(start) = text.find("```") {
        let after = &text[start + 3..];
        let after = after.strip_prefix("json").unwrap_or(after);
        if let Some(end) = after.find("```") {
            return after[..end].to_string();
        }
    }
    text.to_string()
}

fn utc_today() -> chrono::NaiveDate {
    chrono::Utc::now().date_naive()
}

fn short_date(d: chrono::NaiveDate) -> String {
    format!("{}", d.format("%b %-d"))
}

fn time_of_day() -> &'static str {
    let h: u32 = (chrono::Utc::now() + chrono::Duration::hours(8))
        .format("%H")
        .to_string()
        .parse()
        .unwrap_or(12);
    if h < 6 {
        "early morning (before 6 AM) — suggest breakfast and morning activities only"
    } else if h < 12 {
        "morning — suggest remaining morning + afternoon + evening activities"
    } else if h < 17 {
        "afternoon — skip morning, suggest afternoon + evening activities"
    } else {
        "evening — suggest evening and nightlife activities only"
    }
}

/// Thousands grouping to match JS `Number.prototype.toLocaleString()`.
// ponytail: budgets are whole-currency amounts; cents are ignored. Add fraction
// handling if fractional budgets ever appear.
fn format_thousands(n: f64) -> String {
    let s = format!("{}", n.round() as i64);
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}
