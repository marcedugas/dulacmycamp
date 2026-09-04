//! Environment feeds for the landing page: forecast, tides, moon and sun.
//!
//! Weather and tides are proxied from NOAA and cached in-process — both feeds
//! move far more slowly than the page is loaded, and `api.weather.gov` asks
//! callers not to poll. Lunar and solar values are computed locally; they are
//! closed-form astronomy and need no upstream at all.

use crate::{ApiResult, AppError, CAMP_LAT, CAMP_LON, CacheEntry, Shared};
use axum::{Json, extract::State};
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::America::Chicago;
use serde::Serialize;
use serde_json::{Value, json};
use std::time::{Duration as StdDuration, Instant};

const WEATHER_TTL: StdDuration = StdDuration::from_secs(30 * 60);
const TIDES_TTL: StdDuration = StdDuration::from_secs(60 * 60);

/// Returns the cached value when it is still fresh.
async fn cached(slot: &tokio::sync::RwLock<Option<CacheEntry>>, ttl: StdDuration) -> Option<Value> {
    let guard = slot.read().await;
    guard
        .as_ref()
        .filter(|e| e.fetched_at.elapsed() < ttl)
        .map(|e| e.value.clone())
}

async fn store(slot: &tokio::sync::RwLock<Option<CacheEntry>>, value: Value) {
    *slot.write().await = Some(CacheEntry {
        fetched_at: Instant::now(),
        value,
    });
}

async fn fetch_json(state: &Shared, url: &str) -> anyhow::Result<Value> {
    let resp = state.http.get(url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("{url} returned {}", resp.status());
    }
    Ok(resp.json().await?)
}

fn c_to_f(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

fn kmh_to_mph(k: f64) -> f64 {
    k * 0.621_371
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

// ─────────────────────────── weather ───────────────────────────

/// `GET /api/weather` — current conditions plus a seven-day forecast.
///
/// NOAA's API is a two-hop lookup: `/points/{lat},{lon}` hands back the URLs
/// for this location's forecast grid and its observation stations.
pub async fn weather(State(state): State<Shared>) -> ApiResult<Json<Value>> {
    if let Some(v) = cached(&state.cache.weather, WEATHER_TTL).await {
        return Ok(Json(v));
    }

    let value = fetch_weather(&state).await.map_err(|e| {
        tracing::warn!(error = ?e, "NOAA weather fetch failed");
        AppError::Internal(e)
    })?;

    store(&state.cache.weather, value.clone()).await;
    Ok(Json(value))
}

async fn fetch_weather(state: &Shared) -> anyhow::Result<Value> {
    let points = fetch_json(
        state,
        &format!("https://api.weather.gov/points/{CAMP_LAT},{CAMP_LON}"),
    )
    .await?;

    let forecast_url = points["properties"]["forecast"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("no forecast url in points response"))?
        .to_string();
    let stations_url = points["properties"]["observationStations"]
        .as_str()
        .map(str::to_string);

    let forecast = fetch_json(state, &forecast_url).await?;
    let periods = forecast["properties"]["periods"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    // Daytime periods carry the headline temperature; the night period that
    // follows carries the low, so pair them into one row per day.
    let mut days = Vec::new();
    for (i, p) in periods.iter().enumerate() {
        if !p["isDaytime"].as_bool().unwrap_or(true) {
            continue;
        }
        let night_low = periods
            .get(i + 1)
            .filter(|n| !n["isDaytime"].as_bool().unwrap_or(true))
            .and_then(|n| n["temperature"].as_i64());

        days.push(json!({
            "name": p["name"],
            "start_time": p["startTime"],
            "high_f": p["temperature"],
            "low_f": night_low,
            "short_forecast": p["shortForecast"],
            "detailed_forecast": p["detailedForecast"],
            "wind": p["windSpeed"],
            "wind_direction": p["windDirection"],
            "precip_chance": p["probabilityOfPrecipitation"]["value"],
            "icon": p["icon"],
        }));
        if days.len() == 7 {
            break;
        }
    }

    let current = match current_conditions(state, stations_url.as_deref()).await {
        Ok(mut c) => {
            if c["conditions"].is_null() {
                c["conditions"] = periods
                    .first()
                    .map_or(Value::Null, |p| p["shortForecast"].clone());
            }
            c
        }
        Err(e) => {
            // A missing observation shouldn't cost us the forecast; fall back
            // to the first forecast period so the card still renders.
            tracing::warn!(error = ?e, "observation fetch failed, using forecast period");
            periods.first().map_or(Value::Null, |p| {
                json!({
                    "temp_f": p["temperature"],
                    "conditions": p["shortForecast"],
                    "wind_mph": Value::Null,
                    "wind_text": p["windSpeed"],
                    "wind_direction": p["windDirection"],
                    "humidity": Value::Null,
                    "observed_at": p["startTime"],
                    "source": "forecast",
                })
            })
        }
    };

    Ok(json!({
        "location": "Dulac, Louisiana",
        "current": current,
        "forecast": days,
        "updated_at": Utc::now(),
    }))
}

async fn current_conditions(state: &Shared, stations_url: Option<&str>) -> anyhow::Result<Value> {
    let stations_url = stations_url.ok_or_else(|| anyhow::anyhow!("no observationStations url"))?;
    let stations = fetch_json(state, stations_url).await?;
    let station = stations["features"][0]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("no observation station near the camp"))?
        .to_string();

    let obs = fetch_json(state, &format!("{station}/observations/latest")).await?;
    let p = &obs["properties"];

    // Stations frequently report an empty textDescription; treat blank as
    // absent so the card can fall back rather than render an empty line.
    let conditions = p["textDescription"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    Ok(json!({
        "temp_f": p["temperature"]["value"].as_f64().map(|c| round1(c_to_f(c))),
        "conditions": conditions,
        "wind_mph": p["windSpeed"]["value"].as_f64().map(|k| round1(kmh_to_mph(k))),
        "wind_gust_mph": p["windGust"]["value"].as_f64().map(|k| round1(kmh_to_mph(k))),
        "wind_direction_deg": p["windDirection"]["value"],
        "humidity": p["relativeHumidity"]["value"].as_f64().map(|h| h.round()),
        "observed_at": p["timestamp"],
        "source": "observation",
    }))
}

// ─────────────────────────── tides ───────────────────────────

/// `GET /api/tides` — the next seven days of high and low water.
pub async fn tides(State(state): State<Shared>) -> ApiResult<Json<Value>> {
    if let Some(v) = cached(&state.cache.tides, TIDES_TTL).await {
        return Ok(Json(v));
    }

    let value = fetch_tides(&state).await.map_err(|e| {
        tracing::warn!(error = ?e, "NOAA tide fetch failed");
        AppError::Internal(e)
    })?;

    store(&state.cache.tides, value.clone()).await;
    Ok(Json(value))
}

async fn fetch_tides(state: &Shared) -> anyhow::Result<Value> {
    let station = &state.cfg.noaa_station_id;
    let begin = Utc::now().with_timezone(&Chicago).format("%Y%m%d");

    // `interval=hilo` returns only the turning points, which is all the widget
    // shows. `range=168` is seven days in hours.
    let url = format!(
        "https://api.tidesandcurrents.noaa.gov/api/prod/datagetter\
         ?product=predictions&application=dulacmycamp&begin_date={begin}&range=168\
         &datum=MLLW&station={station}&time_zone=lst_ldt&units=english\
         &interval=hilo&format=json"
    );

    let data = fetch_json(state, &url).await?;
    if let Some(err) = data["error"]["message"].as_str() {
        anyhow::bail!("NOAA tides: {err}");
    }

    let predictions: Vec<Value> = data["predictions"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|p| {
            json!({
                // Already local (lst_ldt) — "YYYY-MM-DD HH:MM".
                "time": p["t"],
                "height_ft": p["v"].as_str().and_then(|v| v.parse::<f64>().ok()),
                "kind": match p["type"].as_str() {
                    Some("H") => "high",
                    Some("L") => "low",
                    _ => "unknown",
                },
            })
        })
        .collect();

    // Station metadata gives us a real name rather than a bare id in the UI.
    let station_name = fetch_json(
        state,
        &format!("https://api.tidesandcurrents.noaa.gov/mdapi/prod/webapi/stations/{station}.json"),
    )
    .await
    .ok()
    .and_then(|m| m["stations"][0]["name"].as_str().map(str::to_string));

    Ok(json!({
        "station_id": station,
        "station_name": station_name,
        "timezone": "America/Chicago",
        "predictions": predictions,
        "updated_at": Utc::now(),
    }))
}

// ─────────────────────────── lunar + solar ───────────────────────────

/// Mean length of one lunation, in days.
const SYNODIC_MONTH: f64 = 29.530_588_853;
/// A known new moon: 2000-01-06 18:14 UTC, as a Julian date.
const KNOWN_NEW_MOON_JD: f64 = 2_451_550.259_722;

fn to_julian(dt: DateTime<Utc>) -> f64 {
    dt.timestamp() as f64 / 86_400.0 + 2_440_587.5
}

fn from_julian(jd: f64) -> DateTime<Utc> {
    let secs = (jd - 2_440_587.5) * 86_400.0;
    Utc.timestamp_opt(secs.round() as i64, 0)
        .single()
        .unwrap_or_else(Utc::now)
}

/// Position within the current lunation, 0.0 at new moon, 0.5 at full.
fn moon_fraction(dt: DateTime<Utc>) -> f64 {
    let age = (to_julian(dt) - KNOWN_NEW_MOON_JD).rem_euclid(SYNODIC_MONTH);
    age / SYNODIC_MONTH
}

fn phase_name(f: f64) -> (&'static str, &'static str) {
    // Quarters are instants, so give each a narrow band (roughly ±1 day)
    // and let the four broad phases fill the rest.
    match f {
        f if !(0.021..0.979).contains(&f) => ("New Moon", "🌑"),
        f if f < 0.231 => ("Waxing Crescent", "🌒"),
        f if f < 0.269 => ("First Quarter", "🌓"),
        f if f < 0.481 => ("Waxing Gibbous", "🌔"),
        f if f < 0.519 => ("Full Moon", "🌕"),
        f if f < 0.731 => ("Waning Gibbous", "🌖"),
        f if f < 0.769 => ("Last Quarter", "🌗"),
        _ => ("Waning Crescent", "🌘"),
    }
}

/// Illuminated share of the disc, from the phase angle.
fn illumination(f: f64) -> f64 {
    (1.0 - (2.0 * std::f64::consts::PI * f).cos()) / 2.0
}

#[derive(Debug, Serialize)]
struct SunTimes {
    sunrise: Option<DateTime<Utc>>,
    sunset: Option<DateTime<Utc>>,
    sunrise_local: Option<String>,
    sunset_local: Option<String>,
}

/// Sunrise and sunset for `date` at the camp, via the standard NOAA sunrise
/// equation. Accurate to well under a minute, which is all a widget needs.
fn sun_times(date: NaiveDate) -> SunTimes {
    let midnight = date
        .and_hms_opt(0, 0, 0)
        .map(|d| d.and_utc())
        .unwrap_or_else(Utc::now);

    let rad = std::f64::consts::PI / 180.0;
    // The equation takes west longitude as positive.
    let l_w = -CAMP_LON;
    let phi = CAMP_LAT * rad;

    let n = (to_julian(midnight) - 2_451_545.0 + 0.000_8).ceil();
    let j_star = n + l_w / 360.0;

    let m = (357.529_1 + 0.985_600_28 * j_star).rem_euclid(360.0);
    let c = 1.914_8 * (m * rad).sin()
        + 0.020_0 * (2.0 * m * rad).sin()
        + 0.000_3 * (3.0 * m * rad).sin();
    let lambda = (m + c + 180.0 + 102.937_2).rem_euclid(360.0);

    let j_transit =
        2_451_545.0 + j_star + 0.005_3 * (m * rad).sin() - 0.006_9 * (2.0 * lambda * rad).sin();

    let sin_dec = (lambda * rad).sin() * (23.439_7 * rad).sin();
    let cos_dec = (1.0 - sin_dec * sin_dec).sqrt();

    // −0.833° accounts for refraction and the sun's apparent radius.
    let cos_omega = ((-0.833 * rad).sin() - phi.sin() * sin_dec) / (phi.cos() * cos_dec);

    // |cos ω₀| > 1 means the sun never rises or never sets that day. It never
    // happens at 29°N, but the guard keeps the maths total.
    if !(-1.0..=1.0).contains(&cos_omega) {
        return SunTimes {
            sunrise: None,
            sunset: None,
            sunrise_local: None,
            sunset_local: None,
        };
    }

    let omega = cos_omega.acos() / rad;
    let rise = from_julian(j_transit - omega / 360.0);
    let set = from_julian(j_transit + omega / 360.0);

    let fmt = |dt: DateTime<Utc>| dt.with_timezone(&Chicago).format("%-I:%M %p").to_string();
    SunTimes {
        sunrise: Some(rise),
        sunset: Some(set),
        sunrise_local: Some(fmt(rise)),
        sunset_local: Some(fmt(set)),
    }
}

/// The next instant the moon is full, found by scanning forward in hours and
/// taking the point of maximum illumination. Good to within half an hour.
fn next_full_moon(from: DateTime<Utc>) -> DateTime<Utc> {
    let mut best = from;
    let mut best_illum = -1.0;
    for h in 0..=(24 * 30) {
        let t = from + Duration::hours(h);
        let f = moon_fraction(t);
        // Only consider the window around fullness, so we return the *next*
        // full moon rather than a local maximum in the current gibbous phase.
        if (f - 0.5).abs() < 0.02 {
            let i = illumination(f);
            if i > best_illum {
                best_illum = i;
                best = t;
            } else if best_illum > 0.0 {
                break; // past the peak
            }
        }
    }
    best
}

/// `GET /api/lunar` — moon phase and sun times for the next 30 days.
pub async fn lunar() -> Json<Value> {
    let now = Utc::now();
    let today = now.with_timezone(&Chicago).date_naive();

    let mut days = Vec::with_capacity(31);
    for offset in 0..=30 {
        let date = today + Duration::days(offset);
        // Sample at local noon so a day's phase reflects that day, not the
        // moment the date rolled over.
        let noon = date
            .and_hms_opt(17, 0, 0)
            .map(|d| d.and_utc())
            .unwrap_or(now);
        let f = moon_fraction(noon);
        let (name, emoji) = phase_name(f);
        let sun = sun_times(date);

        days.push(json!({
            "date": date,
            "phase": name,
            "emoji": emoji,
            "fraction": (f * 10_000.0).round() / 10_000.0,
            "illumination": (illumination(f) * 100.0).round(),
            "sunrise": sun.sunrise,
            "sunset": sun.sunset,
            "sunrise_local": sun.sunrise_local,
            "sunset_local": sun.sunset_local,
        }));
    }

    // Read `current` off the first day rather than resampling at `now`: the
    // widget shows the phase and today's sun times side by side, and taking
    // them from different instants can straddle a phase boundary and disagree.
    let today_entry = days.first().cloned().unwrap_or(Value::Null);
    let full = next_full_moon(now);

    Json(json!({
        "location": "Dulac, Louisiana",
        "timezone": "America/Chicago",
        "current": {
            "date": today,
            "phase": today_entry["phase"].clone(),
            "emoji": today_entry["emoji"].clone(),
            "illumination": today_entry["illumination"].clone(),
        },
        "next_full_moon": {
            "at": full,
            "date": full.with_timezone(&Chicago).date_naive(),
            "days_away": (full.with_timezone(&Chicago).date_naive() - today).num_days(),
        },
        "days": days,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reference epoch must read as a new moon.
    #[test]
    fn known_new_moon_reads_as_new() {
        let t = from_julian(KNOWN_NEW_MOON_JD);
        let f = moon_fraction(t);
        assert!(!(0.01..=0.99).contains(&f), "fraction was {f}");
        assert_eq!(phase_name(f).0, "New Moon");
    }

    /// Half a lunation later the moon must be full and fully lit.
    #[test]
    fn half_a_lunation_is_full() {
        let t = from_julian(KNOWN_NEW_MOON_JD + SYNODIC_MONTH / 2.0);
        let f = moon_fraction(t);
        assert_eq!(phase_name(f).0, "Full Moon");
        assert!(
            illumination(f) > 0.99,
            "illumination was {}",
            illumination(f)
        );
    }

    /// Cross-check against a published full moon: 2026-09-26 (Sept 2026).
    #[test]
    fn next_full_moon_is_within_a_day_of_the_almanac() {
        let from = Utc.with_ymd_and_hms(2026, 9, 3, 12, 0, 0).unwrap();
        let full = next_full_moon(from).date_naive();
        let expected = NaiveDate::from_ymd_opt(2026, 9, 26).unwrap();
        let delta = (full - expected).num_days().abs();
        assert!(
            delta <= 1,
            "got {full}, expected within a day of {expected}"
        );
    }

    /// Sunrise precedes sunset, and a Louisiana summer day is 12–14 hours.
    #[test]
    fn sun_times_are_sane_for_the_camp() {
        let sun = sun_times(NaiveDate::from_ymd_opt(2026, 6, 21).unwrap());
        let (rise, set) = (sun.sunrise.unwrap(), sun.sunset.unwrap());
        assert!(rise < set);
        let hours = (set - rise).num_minutes() as f64 / 60.0;
        assert!((13.0..14.5).contains(&hours), "day length was {hours}h");
    }

    /// The winter solstice must be materially shorter than the summer one.
    #[test]
    fn winter_days_are_shorter_than_summer_days() {
        let summer = sun_times(NaiveDate::from_ymd_opt(2026, 6, 21).unwrap());
        let winter = sun_times(NaiveDate::from_ymd_opt(2026, 12, 21).unwrap());
        let len = |s: &SunTimes| (s.sunset.unwrap() - s.sunrise.unwrap()).num_minutes();
        assert!(len(&winter) + 120 < len(&summer));
    }

    #[test]
    fn unit_conversions_round_trip() {
        assert!((c_to_f(0.0) - 32.0).abs() < 1e-9);
        assert!((c_to_f(100.0) - 212.0).abs() < 1e-9);
        assert!((kmh_to_mph(100.0) - 62.137).abs() < 0.01);
    }
}
