//! Google Places classic (legacy) endpoints. Mirrors `lib/google-places.ts`.

use serde::{Deserialize, Serialize};

use core::error::ApiError;

// ── Output types (JSON wire format == lib/google-places.ts) ────────────────

#[derive(Debug, Serialize)]
pub struct NearbyPlace {
    pub place_id: String,
    pub name: String,
    pub rating: f64,
    pub total_ratings: i64,
    pub price_level: Option<i64>,
    pub address: String,
    pub lat: f64,
    pub lng: f64,
    pub open_now: Option<bool>,
    pub photo_url: Option<String>,
    pub types: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PlaceDetails {
    pub name: String,
    pub rating: f64,
    pub formatted_phone_number: Option<String>,
    pub formatted_address: String,
    pub opening_hours: Option<OpeningHours>,
    pub reviews: Option<Vec<Review>>,
    pub photos: Vec<String>,
    pub website: Option<String>,
    pub url: Option<String>,
    pub price_level: Option<i64>,
    pub editorial_summary: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OpeningHours {
    pub weekday_text: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Review {
    pub author_name: String,
    pub rating: f64,
    pub text: String,
    pub relative_time_description: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutocompleteResult {
    pub place_id: String,
    pub description: String,
}

#[derive(Debug, Serialize)]
pub struct PlaceLocation {
    pub name: String,
    pub lat: f64,
    pub lng: f64,
}

/// Internal result of a `findplacefromtext` lookup (not serialized directly).
struct PlaceSearchResult {
    place_id: String,
    name: String,
    address: String,
    rating: f64,
    total_ratings: i64,
    photo_url: Option<String>,
    lat: f64,
    lng: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichRequest {
    pub recs: Vec<serde_json::Value>,
}

// ── searchNearby ───────────────────────────────────────────────────────────

pub async fn search_nearby(
    client: &reqwest::Client,
    key: &str,
    type_: Option<&str>,
    keyword: Option<&str>,
    lat: Option<f64>,
    lng: Option<f64>,
) -> Result<Vec<NearbyPlace>, ApiError> {
    require_key(key, "Google Places")?;
    let (lat, lng) = match (lat, lng) {
        (Some(lat), Some(lng)) => (lat, lng),
        _ => return Ok(Vec::new()), // no trip coords → no results; never guess a location
    };
    let mut url = places_url("nearbysearch/json");
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("location", &format!("{lat},{lng}"));
        q.append_pair("radius", "1500");
        q.append_pair("key", key);
        if let Some(t) = type_ {
            q.append_pair("type", t);
        }
        if let Some(k) = keyword {
            q.append_pair("keyword", k);
        }
    }

    // One page (20 results) to save API tokens — same as the client.
    let data = get_json(client, url).await?;
    let results = data
        .get("results")
        .and_then(|r| r.as_array())
        .map(|arr| arr.iter().map(|p| map_nearby(p, key)).collect())
        .unwrap_or_default();
    Ok(results)
}

fn map_nearby(place: &serde_json::Value, key: &str) -> NearbyPlace {
    NearbyPlace {
        place_id: str_field(place, "place_id"),
        name: str_field(place, "name"),
        rating: f64_field(place, "rating"),
        total_ratings: i64_field(place, "user_ratings_total"),
        price_level: place.get("price_level").and_then(|v| v.as_i64()),
        address: str_field(place, "vicinity"),
        lat: nested_f64(place, &["geometry", "location", "lat"]),
        lng: nested_f64(place, &["geometry", "location", "lng"]),
        open_now: place
            .get("opening_hours")
            .and_then(|o| o.get("open_now"))
            .and_then(|v| v.as_bool()),
        photo_url: pick_best_photo(place.get("photos"), 1200, key),
        types: place
            .get("types")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

// ── getPlaceDetails ────────────────────────────────────────────────────────

pub async fn get_place_details(
    client: &reqwest::Client,
    key: &str,
    place_id: &str,
) -> Result<Option<PlaceDetails>, ApiError> {
    require_key(key, "Google Places")?;
    let mut url = places_url("details/json");
    url.query_pairs_mut()
        .append_pair("place_id", place_id)
        .append_pair(
            "fields",
            "name,rating,formatted_phone_number,formatted_address,opening_hours,reviews,photos,website,url,price_level,editorial_summary",
        )
        .append_pair("key", key);

    let data = get_json(client, url).await?;
    let r = match data.get("result") {
        Some(r) => r,
        None => return Ok(None),
    };

    let photos: Vec<String> = r
        .get("photos")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .take(6)
                .filter_map(|p| p.get("photo_reference").and_then(|v| v.as_str()))
                .map(|r_| photo_url(r_, 600, key))
                .collect()
        })
        .unwrap_or_default();

    let reviews: Option<Vec<Review>> = r
        .get("reviews")
        .and_then(|rv| rv.as_array())
        .map(|arr| {
            arr.iter()
                .take(3)
                .map(|rv| Review {
                    author_name: str_field(rv, "author_name"),
                    rating: f64_field(rv, "rating"),
                    text: str_field(rv, "text"),
                    relative_time_description: str_field(rv, "relative_time_description"),
                })
                .collect()
        });

    Ok(Some(PlaceDetails {
        name: str_field(r, "name"),
        rating: f64_field(r, "rating"),
        formatted_phone_number: r
            .get("formatted_phone_number")
            .and_then(|v| v.as_str())
            .map(String::from),
        formatted_address: str_field(r, "formatted_address"),
        opening_hours: r.get("opening_hours").map(|o| OpeningHours {
            weekday_text: o
                .get("weekday_text")
                .and_then(|w| w.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        }),
        reviews,
        photos,
        website: r.get("website").and_then(|v| v.as_str()).map(String::from),
        url: r.get("url").and_then(|v| v.as_str()).map(String::from),
        price_level: r.get("price_level").and_then(|v| v.as_i64()),
        editorial_summary: r
            .get("editorial_summary")
            .and_then(|e| e.get("overview"))
            .and_then(|v| v.as_str())
            .map(String::from),
    }))
}

// ── getPlaceLocation ───────────────────────────────────────────────────────

pub async fn get_place_location(
    client: &reqwest::Client,
    key: &str,
    place_id: &str,
) -> Result<Option<PlaceLocation>, ApiError> {
    require_key(key, "Google Places")?;
    let mut url = places_url("details/json");
    url.query_pairs_mut()
        .append_pair("place_id", place_id)
        .append_pair("fields", "name,geometry")
        .append_pair("key", key);

    let data = get_json(client, url).await?;
    let r = match data.get("result") {
        Some(r) => r,
        None => return Ok(None),
    };
    let lat = nested_f64(r, &["geometry", "location", "lat"]);
    let lng = nested_f64(r, &["geometry", "location", "lng"]);
    if lat == 0.0 && lng == 0.0 {
        return Ok(None);
    }
    Ok(Some(PlaceLocation {
        name: str_field(r, "name"),
        lat,
        lng,
    }))
}

// ── placeAutocomplete ──────────────────────────────────────────────────────

pub async fn place_autocomplete(
    client: &reqwest::Client,
    key: &str,
    input: &str,
) -> Result<Vec<AutocompleteResult>, ApiError> {
    require_key(key, "Google Places")?;
    let input = input.trim();
    if input.is_empty() {
        return Ok(Vec::new());
    }
    let mut url = places_url("autocomplete/json");
    url.query_pairs_mut()
        .append_pair("input", input)
        .append_pair("key", key);

    let data = get_json(client, url).await?;
    Ok(data
        .get("predictions")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .map(|p| AutocompleteResult {
                    place_id: str_field(p, "place_id"),
                    description: str_field(p, "description"),
                })
                .collect()
        })
        .unwrap_or_default())
}

// ── enrichRecommendations ──────────────────────────────────────────────────

pub async fn enrich_recommendations(
    client: &reqwest::Client,
    key: &str,
    req: &EnrichRequest,
) -> Result<Vec<serde_json::Value>, ApiError> {
    if key.is_empty() {
        return Ok(req
            .recs
            .iter()
            .map(|rec| enrich_one(rec, None))
            .collect());
    }

    // ponytail: sequential lookup; parallelize (futures::join_all) if recs grow large.
    let mut out = Vec::with_capacity(req.recs.len());
    for rec in &req.recs {
        let name = rec.get("name").and_then(|n| n.as_str()).unwrap_or("");
        let found = search_place(client, key, name).await?;
        out.push(enrich_one(rec, found.as_ref()));
    }
    Ok(out)
}

fn enrich_one(rec: &serde_json::Value, found: Option<&PlaceSearchResult>) -> serde_json::Value {
    let mut map = rec
        .as_object()
        .cloned()
        .unwrap_or_default();

    let (photo_uri, maps_uri, place_id, total_ratings, lat, lng) = match found {
        Some(v) => (
            v.photo_url.clone(),
            Some(format!(
                "https://www.google.com/maps/place/?q=place_id:{}",
                v.place_id
            )),
            Some(v.place_id.clone()),
            v.total_ratings,
            v.lat,
            v.lng,
        ),
        None => (None, None, None, 0, 0.0, 0.0),
    };

    map.insert("photoUri".to_string(), serde_json::json!(photo_uri));
    map.insert("googleMapsUri".to_string(), serde_json::json!(maps_uri));
    map.insert("googlePlaceId".to_string(), serde_json::json!(place_id));
    map.insert("totalRatings".to_string(), serde_json::json!(total_ratings));
    map.insert("lat".to_string(), serde_json::json!(lat));
    map.insert("lng".to_string(), serde_json::json!(lng));

    serde_json::Value::Object(map)
}

async fn search_place(
    client: &reqwest::Client,
    key: &str,
    query: &str,
) -> Result<Option<PlaceSearchResult>, ApiError> {
    let mut url = places_url("findplacefromtext/json");
    url.query_pairs_mut()
        .append_pair("input", query)
        .append_pair("inputtype", "textquery")
        .append_pair(
            "fields",
            "place_id,name,formatted_address,rating,user_ratings_total,photos,geometry",
        )
        .append_pair("key", key);

    let data = get_json(client, url).await?;
    let candidate = match data.get("candidates").and_then(|c| c.get(0)) {
        Some(c) => c,
        None => return Ok(None),
    };

    Ok(Some(PlaceSearchResult {
        place_id: str_field(candidate, "place_id"),
        name: str_field(candidate, "name"),
        address: str_field(candidate, "formatted_address"),
        rating: f64_field(candidate, "rating"),
        total_ratings: i64_field(candidate, "user_ratings_total"),
        photo_url: pick_best_photo(candidate.get("photos"), 800, key),
        lat: nested_f64(candidate, &["geometry", "location", "lat"]),
        lng: nested_f64(candidate, &["geometry", "location", "lng"]),
    }))
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn places_url(endpoint: &str) -> reqwest::Url {
    reqwest::Url::parse(&format!(
        "https://maps.googleapis.com/maps/api/place/{endpoint}"
    ))
    .expect("static places URL is valid")
}

fn photo_url(photo_ref: &str, max_width: u32, key: &str) -> String {
    format!(
        "https://maps.googleapis.com/maps/api/place/photo?maxwidth={max_width}&photo_reference={photo_ref}&key={key}"
    )
}

/// Prefer the first landscape photo (index 0–2 are most likely storefront /
/// exterior shots), falling back to Google's chosen cover photo.
fn pick_best_photo(
    photos: Option<&serde_json::Value>,
    max_width: u32,
    key: &str,
) -> Option<String> {
    let photos = photos?.as_array()?;
    if photos.is_empty() {
        return None;
    }
    let landscape = photos
        .iter()
        .take(5)
        .find(|p| {
            let w = p.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let h = p.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0);
            w > h
        });
    let best = landscape.or_else(|| photos.first())?;
    best.get("photo_reference")
        .and_then(|r| r.as_str())
        .map(|r| photo_url(r, max_width, key))
}

async fn get_json(client: &reqwest::Client, url: reqwest::Url) -> Result<serde_json::Value, ApiError> {
    let res = client
        .get(url)
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("places request failed: {e}")))?;
    if !res.status().is_success() {
        return Err(ApiError::Internal(format!(
            "Google Places {}: {}",
            res.status(),
            res.text().await.unwrap_or_default()
        )));
    }
    res.json().await.map_err(ApiError::from)
}

fn require_key(key: &str, provider: &str) -> Result<(), ApiError> {
    if key.is_empty() {
        return Err(ApiError::Internal(format!("missing {provider} API key")));
    }
    Ok(())
}

fn str_field(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}

fn f64_field(v: &serde_json::Value, key: &str) -> f64 {
    v.get(key).and_then(|x| x.as_f64()).unwrap_or(0.0)
}

fn i64_field(v: &serde_json::Value, key: &str) -> i64 {
    v.get(key).and_then(|x| x.as_i64()).unwrap_or(0)
}

fn nested_f64(v: &serde_json::Value, path: &[&str]) -> f64 {
    let mut cur = v;
    for key in path {
        cur = match cur.get(key) {
            Some(next) => next,
            None => return 0.0,
        };
    }
    cur.as_f64().unwrap_or(0.0)
}
