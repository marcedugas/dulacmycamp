//! Environment feeds for the landing page: forecast, tides, moon and sun.
//!
//! Weather and tides are proxied from NOAA and cached in-process — both feeds
//! move far more slowly than the page is loaded, and `api.weather.gov` asks
//! callers not to poll. Lunar and solar values are computed locally; they are
//! closed-form astronomy and need no upstream at all.

use crate::{
    ApiResult, AppError, CAMP_LAT, CAMP_LON, CacheEntry, FISHING_LAT, FISHING_LON, Shared,
};
use axum::{Json, extract::State};
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::America::Chicago;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::{Duration as StdDuration, Instant};

const WEATHER_TTL: StdDuration = StdDuration::from_secs(30 * 60);
const TIDES_TTL: StdDuration = StdDuration::from_secs(60 * 60);
/// Station datums are a decadal average — a day between refreshes is generous.
const DATUMS_TTL: StdDuration = StdDuration::from_secs(24 * 60 * 60);
/// Observations move a little faster than the forecast; refresh on the same
/// cadence as the weather card that mostly drives them.
const OBS_TTL: StdDuration = StdDuration::from_secs(30 * 60);

/// Returns the cached value when it is still fresh.
pub(crate) async fn cached(
    slot: &tokio::sync::RwLock<Option<CacheEntry>>,
    ttl: StdDuration,
) -> Option<Value> {
    let guard = slot.read().await;
    guard
        .as_ref()
        .filter(|e| e.fetched_at.elapsed() < ttl)
        .map(|e| e.value.clone())
}

pub(crate) async fn store(slot: &tokio::sync::RwLock<Option<CacheEntry>>, value: Value) {
    *slot.write().await = Some(CacheEntry {
        fetched_at: Instant::now(),
        value,
    });
}

pub(crate) async fn fetch_json(state: &Shared, url: &str) -> anyhow::Result<Value> {
    let resp = state.http.get(url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("{url} returned {}", resp.status());
    }
    Ok(resp.json().await?)
}

fn c_to_f(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

pub(crate) fn kmh_to_mph(k: f64) -> f64 {
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
    // The estuary grid, not the camp's — wind and pressure on the open water
    // are what a fisherman is reading the card for.
    let points = fetch_json(
        state,
        &format!("https://api.weather.gov/points/{FISHING_LAT},{FISHING_LON}"),
    )
    .await?;

    let forecast_url = points["properties"]["forecast"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("no forecast url in points response"))?
        .to_string();

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

    let current = match current_conditions(state).await {
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
                    "pressure_trend": "steady",
                    "observed_at": p["startTime"],
                    "source": "forecast",
                })
            })
        }
    };

    Ok(json!({
        "location": "Cocodrie estuary, Louisiana",
        "current": current,
        "forecast": days,
        "updated_at": Utc::now(),
    }))
}

/// One normalized station observation. NOAA reports SI units with a nested
/// `{ value, unitCode }` shape and frequent nulls; this flattens it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Observation {
    pub at: DateTime<Utc>,
    pub temp_c: Option<f64>,
    pub wind_kmh: Option<f64>,
    pub wind_gust_kmh: Option<f64>,
    pub wind_dir_deg: Option<f64>,
    pub humidity: Option<f64>,
    pub pressure_pa: Option<f64>,
    pub text: Option<String>,
}

/// Recent observations from the station nearest the fishing grounds, newest
/// first (typically the last ~12 hours). Cached ~30 min and shared: the
/// weather card reads the head of the list, the fishing forecast reads a
/// barometric trend off the whole window.
pub(crate) async fn recent_observations(state: &Shared) -> anyhow::Result<Vec<Observation>> {
    if let Some(v) = cached(&state.cache.observations, OBS_TTL).await
        && let Ok(obs) = serde_json::from_value::<Vec<Observation>>(v)
    {
        return Ok(obs);
    }
    let obs = fetch_observation_series(state).await?;
    store(&state.cache.observations, serde_json::to_value(&obs)?).await;
    Ok(obs)
}

async fn fetch_observation_series(state: &Shared) -> anyhow::Result<Vec<Observation>> {
    let points = fetch_json(
        state,
        &format!("https://api.weather.gov/points/{FISHING_LAT},{FISHING_LON}"),
    )
    .await?;
    let stations_url = points["properties"]["observationStations"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("no observationStations url in points response"))?;
    let stations = fetch_json(state, stations_url).await?;
    let station = stations["features"][0]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("no observation station near the fishing grounds"))?;

    let list = fetch_json(state, &format!("{station}/observations?limit=12")).await?;
    let mut out: Vec<Observation> = list["features"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|f| {
            let p = &f["properties"];
            let at = p["timestamp"]
                .as_str()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())?
                .with_timezone(&Utc);
            Some(Observation {
                at,
                temp_c: p["temperature"]["value"].as_f64(),
                wind_kmh: p["windSpeed"]["value"].as_f64(),
                wind_gust_kmh: p["windGust"]["value"].as_f64(),
                wind_dir_deg: p["windDirection"]["value"].as_f64(),
                humidity: p["relativeHumidity"]["value"].as_f64(),
                pressure_pa: p["barometricPressure"]["value"].as_f64(),
                text: p["textDescription"]
                    .as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
            })
        })
        .collect();
    out.sort_by_key(|o| std::cmp::Reverse(o.at));
    Ok(out)
}

/// Direction of barometric change across the observation window, in millibars
/// (newest reading minus the oldest within ~6 hours). Falling pressure — a
/// front moving in — tends to trigger feeding; a rising glass behind it tends
/// to shut it down. Returns the label and the signed change.
pub(crate) fn pressure_trend(obs: &[Observation]) -> (&'static str, Option<f64>) {
    let series: Vec<(DateTime<Utc>, f64)> = obs
        .iter()
        .filter_map(|o| o.pressure_pa.map(|pa| (o.at, pa / 100.0)))
        .collect();
    let Some(&(newest_at, newest)) = series.first() else {
        return ("steady", None);
    };
    let cutoff = newest_at - Duration::hours(6);
    let Some(&(_, oldest)) = series.iter().rfind(|(t, _)| *t >= cutoff) else {
        return ("steady", None);
    };
    let delta = round1(newest - oldest);
    let label = if delta <= -0.6 {
        "falling"
    } else if delta >= 0.6 {
        "rising"
    } else {
        "steady"
    };
    (label, Some(delta))
}

async fn current_conditions(state: &Shared) -> anyhow::Result<Value> {
    let obs = recent_observations(state).await?;
    let latest = obs
        .first()
        .ok_or_else(|| anyhow::anyhow!("no recent observations"))?;
    let (trend, change) = pressure_trend(&obs);

    Ok(json!({
        "temp_f": latest.temp_c.map(|c| round1(c_to_f(c))),
        "conditions": latest.text,
        "wind_mph": latest.wind_kmh.map(|k| round1(kmh_to_mph(k))),
        "wind_gust_mph": latest.wind_gust_kmh.map(|k| round1(kmh_to_mph(k))),
        "wind_direction_deg": latest.wind_dir_deg,
        "humidity": latest.humidity.map(|h| h.round()),
        "pressure_mb": latest.pressure_pa.map(|pa| round1(pa / 100.0)),
        "pressure_trend": trend,
        "pressure_change_mb": change,
        "observed_at": latest.at,
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
    let now_local = Utc::now().with_timezone(&Chicago);
    let begin = now_local.format("%Y%m%d");

    // `interval=hilo` returns only the turning points — the list the widget has
    // always shown. `range=168` is seven days in hours.
    let hilo_url = format!(
        "https://api.tidesandcurrents.noaa.gov/api/prod/datagetter\
         ?product=predictions&application=dulacmycamp&begin_date={begin}&range=168\
         &datum=MLLW&station={station}&time_zone=lst_ldt&units=english\
         &interval=hilo&format=json"
    );

    let data = fetch_json(state, &hilo_url).await?;
    if let Some(err) = data["error"]["message"].as_str() {
        anyhow::bail!("NOAA tides: {err}");
    }

    let next_tides: Vec<Value> = data["predictions"]
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

    // A continuous hourly curve so the widget can draw the tide's shape, not
    // just its turning points. Best-effort on its own request: if only this
    // call fails the widget falls back to the list, so a NOAA hiccup here
    // doesn't cost the whole feed. Window: the last ~6 h through the next ~42 h.
    let curve_begin = (now_local - Duration::hours(6))
        .format("%Y%m%d %H:%M")
        .to_string()
        .replace(' ', "%20");
    let curve = match fetch_tide_curve(state, station, &curve_begin).await {
        Ok(points) => points,
        Err(e) => {
            tracing::warn!(error = ?e, "NOAA tide curve fetch failed; widget shows the list only");
            Vec::new()
        }
    };

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
        "next_tides": next_tides,
        "curve": curve,
        "updated_at": Utc::now(),
    }))
}

/// URL for a continuous hourly prediction window — `interval=h`, `range=48`
/// hours from `begin` (which the caller sets to ~6 h ago). Split out so the
/// query shape is unit-testable without a network call.
fn tide_curve_url(station: &str, begin: &str) -> String {
    format!(
        "https://api.tidesandcurrents.noaa.gov/api/prod/datagetter\
         ?product=predictions&application=dulacmycamp&begin_date={begin}&range=48\
         &datum=MLLW&station={station}&time_zone=lst_ldt&units=english\
         &interval=h&format=json"
    )
}

async fn fetch_tide_curve(
    state: &Shared,
    station: &str,
    begin: &str,
) -> anyhow::Result<Vec<Value>> {
    let data = fetch_json(state, &tide_curve_url(station, begin)).await?;
    parse_tide_curve(&data)
}

/// Pulls the hourly `{ time, height_ft }` points out of a datagetter response.
/// Errors — rather than returning an empty `Vec` — on a NOAA error message or a
/// response with too few usable points, so the caller falls back to the hi/lo
/// list instead of drawing a blank graph.
fn parse_tide_curve(data: &Value) -> anyhow::Result<Vec<Value>> {
    if let Some(err) = data["error"]["message"].as_str() {
        anyhow::bail!("NOAA tide curve: {err}");
    }
    let points: Vec<Value> = data["predictions"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|p| {
            let time = p["t"].as_str()?;
            let height_ft = p["v"].as_str().and_then(|v| v.parse::<f64>().ok())?;
            // Same local "YYYY-MM-DD HH:MM" string as `next_tides`, so the
            // frontend puts both on one axis with no timezone arithmetic.
            Some(json!({ "time": time, "height_ft": height_ft }))
        })
        .collect();
    if points.len() < 2 {
        anyhow::bail!("NOAA tide curve: only {} usable point(s)", points.len());
    }
    Ok(points)
}

/// The station's decadal-average tidal range (feet) — Great Diurnal Range,
/// falling back to Mean Range of Tide — from the CO-OPS metadata API (the
/// `datagetter` datums product was retired). Cached a day: it is a
/// 2012–2016-epoch average and barely moves. `None` on any failure so the
/// fishing forecast treats tide strength as neutral rather than blocking.
pub(crate) async fn baseline_tidal_range(state: &Shared) -> Option<f64> {
    if let Some(v) = cached(&state.cache.tide_datums, DATUMS_TTL).await {
        return v.as_f64();
    }
    let station = &state.cfg.noaa_station_id;
    let data = fetch_json(
        state,
        &format!(
            "https://api.tidesandcurrents.noaa.gov/mdapi/prod/webapi/stations/{station}/datums.json"
        ),
    )
    .await
    .ok()?;
    let datums = data["datums"].as_array()?;
    let value = |name: &str| {
        datums
            .iter()
            .find(|d| d["name"].as_str() == Some(name))
            .and_then(|d| d["value"].as_f64())
    };
    let range = value("GT").or_else(|| value("MN"))?;
    // Cache only on success — a miss should retry (at most once an hour, since
    // the forecast that calls this is itself cached), not stick for a day.
    store(&state.cache.tide_datums, json!(range)).await;
    Some(range)
}

/// The 7-day hi/lo predictions, read from the tide feed's cache (populating it
/// if cold). Shared with `GET /api/tides` so the fishing forecast doesn't fetch
/// the same data twice. Empty on failure — the caller drops the tide modifier.
pub(crate) async fn tide_hilo(state: &Shared) -> Vec<Value> {
    let blob = match cached(&state.cache.tides, TIDES_TTL).await {
        Some(v) => v,
        None => match fetch_tides(state).await {
            Ok(v) => {
                store(&state.cache.tides, v.clone()).await;
                v
            }
            Err(e) => {
                tracing::warn!(error = ?e, "tide feed unavailable for the fishing forecast");
                return Vec::new();
            }
        },
    };
    blob["next_tides"].as_array().cloned().unwrap_or_default()
}

// ─────────────────────────── lunar + solar ───────────────────────────

/// Mean length of one lunation, in days.
pub(crate) const SYNODIC_MONTH: f64 = 29.530_588_853;
/// Meeus' *mean* new-moon epoch (Astronomical Algorithms 2nd ed., eq. 49.1,
/// k = 0): JDE 2451550.09766 ≈ 2000-01-06 14:20 UTC — the mean instant, which
/// is the right anchor to propagate at the mean synodic rate.
const KNOWN_NEW_MOON_JD: f64 = 2_451_550.097_66;
/// Mean synodic month at J2000 (eq. 49.1's linear term) — a hair different from
/// `SYNODIC_MONTH`, kept exact here because `phase_jde` propagates 300+ cycles.
const MEAN_LUNATION: f64 = 29.530_588_861;

pub(crate) fn to_julian(dt: DateTime<Utc>) -> f64 {
    dt.timestamp() as f64 / 86_400.0 + 2_440_587.5
}

fn from_julian(jd: f64) -> DateTime<Utc> {
    let secs = (jd - 2_440_587.5) * 86_400.0;
    Utc.timestamp_opt(secs.round() as i64, 0)
        .single()
        .unwrap_or_else(Utc::now)
}

/// Julian Ephemeris Day of a new (`k` integer) or full (`k` + ½) moon, with the
/// Meeus ch. 49 periodic corrections applied on top of the mean phase time.
///
/// A pure mean-motion estimate is off from the true (apparent) syzygy by up to
/// ~14 h over a year — enough to land the phase *label* on the wrong calendar
/// day. The correction series pulls that to a few minutes. ΔT (~+69 s in 2026)
/// is neglected: it is far inside the accuracy this needs.
pub(crate) fn phase_jde(k: f64) -> f64 {
    let t = k / 1236.85;
    let (t2, t3, t4) = (t * t, t * t * t, t * t * t * t);
    let rad = std::f64::consts::PI / 180.0;
    let tau = std::f64::consts::TAU;

    let mut jde = KNOWN_NEW_MOON_JD + MEAN_LUNATION * k + 0.000_154_37 * t2 - 0.000_000_150 * t3
        + 0.000_000_000_73 * t4;

    let m = ((2.5534 + 29.105_356_70 * k - 0.000_000_14 * t2 - 0.000_000_11 * t3) * rad)
        .rem_euclid(tau);
    let mp = ((201.5643 + 385.816_935_28 * k + 0.010_758_2 * t2 + 0.000_012_38 * t3
        - 0.000_000_058 * t4)
        * rad)
        .rem_euclid(tau);
    let f = ((160.7108 + 390.670_502_84 * k - 0.001_611_8 * t2 - 0.000_002_27 * t3
        + 0.000_000_011 * t4)
        * rad)
        .rem_euclid(tau);
    let omega = ((124.7746 - 1.563_755_88 * k + 0.002_067_2 * t2 + 0.000_002_15 * t3) * rad)
        .rem_euclid(tau);
    let e = 1.0 - 0.002_516 * t - 0.000_007_4 * t2;

    // New/full correction table (Meeus, p. 351) — identical for both.
    jde += -0.407_20 * mp.sin()
        + 0.172_41 * e * m.sin()
        + 0.016_08 * (2.0 * mp).sin()
        + 0.010_39 * (2.0 * f).sin()
        + 0.007_39 * e * (mp - m).sin()
        - 0.005_14 * e * (mp + m).sin()
        + 0.002_08 * e * e * (2.0 * m).sin()
        - 0.001_11 * (mp - 2.0 * f).sin()
        - 0.000_57 * (mp + 2.0 * f).sin()
        + 0.000_56 * e * (2.0 * mp + m).sin()
        - 0.000_42 * (3.0 * mp).sin()
        + 0.000_42 * e * (m + 2.0 * f).sin()
        + 0.000_38 * e * (m - 2.0 * f).sin()
        - 0.000_24 * e * (2.0 * mp - m).sin()
        - 0.000_17 * omega.sin()
        - 0.000_07 * (mp + 2.0 * m).sin()
        + 0.000_04 * (2.0 * mp - 2.0 * f).sin()
        + 0.000_04 * (3.0 * m).sin()
        + 0.000_03 * (mp + m - 2.0 * f).sin()
        + 0.000_03 * (2.0 * mp + 2.0 * f).sin()
        - 0.000_03 * (mp + m + 2.0 * f).sin()
        + 0.000_03 * (mp - m + 2.0 * f).sin()
        - 0.000_02 * (mp - m - 2.0 * f).sin()
        - 0.000_02 * (3.0 * mp + m).sin()
        + 0.000_02 * (4.0 * mp).sin();

    // Planetary-argument additive terms (Meeus, p. 252).
    let a = |c0: f64, c1: f64, c2: f64| ((c0 + c1 * k + c2 * t2) * rad).sin();
    jde += 0.000_325 * a(299.77, 0.107_408, -0.009_173)
        + 0.000_165 * a(251.88, 0.016_321, 0.0)
        + 0.000_164 * a(251.83, 26.651_886, 0.0)
        + 0.000_126 * a(349.42, 36.412_478, 0.0)
        + 0.000_110 * a(84.66, 18.206_239, 0.0)
        + 0.000_062 * a(141.74, 53.303_771, 0.0)
        + 0.000_060 * a(207.14, 2.453_732, 0.0)
        + 0.000_056 * a(154.84, 7.306_860, 0.0)
        + 0.000_047 * a(34.52, 27.261_239, 0.0)
        + 0.000_042 * a(207.19, 0.121_824, 0.0)
        + 0.000_040 * a(291.34, 1.844_379, 0.0)
        + 0.000_037 * a(161.72, 24.198_154, 0.0)
        + 0.000_035 * a(239.56, 25.513_099, 0.0)
        + 0.000_023 * a(331.55, 3.592_518, 0.0);

    jde
}

/// `k` for the new moon nearest `jd` (integer, as [`phase_jde`] takes it).
fn nearest_new_moon_k(jd: f64) -> f64 {
    ((jd - KNOWN_NEW_MOON_JD) / MEAN_LUNATION).round()
}

/// Unsigned days from `dt` to the nearest *true* new or full moon. This is the
/// one distance the fishing rating's moon component keys on.
pub(crate) fn days_to_nearest_syzygy(dt: DateTime<Utc>) -> f64 {
    let jd = to_julian(dt);
    let k0 = nearest_new_moon_k(jd);
    let mut best = f64::MAX;
    for dk in -1..=1 {
        for half in [0.0, 0.5] {
            best = best.min((jd - phase_jde(k0 + f64::from(dk) + half)).abs());
        }
    }
    best
}

/// Fraction through the current lunation, bounded by the *true* new moons on
/// either side of `dt`. Feeding this to [`phase_name`] (rather than a
/// mean-motion age) keeps the label on the right calendar day near the annual
/// extremes.
pub(crate) fn true_moon_fraction(dt: DateTime<Utc>) -> f64 {
    let jd = to_julian(dt);
    let k = nearest_new_moon_k(jd);
    let (mut prev, mut next) = (phase_jde(k), phase_jde(k + 1.0));
    if prev > jd {
        next = prev;
        prev = phase_jde(k - 1.0);
    } else if next <= jd {
        prev = next;
        next = phase_jde(k + 2.0);
    }
    ((jd - prev) / (next - prev)).clamp(0.0, 1.0)
}

/// The next full moon strictly after `from` (Meeus ch. 49).
fn next_true_full_moon(from: DateTime<Utc>) -> DateTime<Utc> {
    let jd = to_julian(from);
    let k0 = nearest_new_moon_k(jd) - 1.0;
    for i in 0..4 {
        let full = phase_jde(k0 + f64::from(i) + 0.5);
        if full > jd {
            return from_julian(full);
        }
    }
    from_julian(phase_jde(k0 + 2.5))
}

pub(crate) fn phase_name(f: f64) -> (&'static str, &'static str) {
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
        let f = true_moon_fraction(noon);
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
    let full = next_true_full_moon(now);

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

    /// A computed new-moon instant must read as a new moon and sit ~0 days from
    /// the nearest syzygy.
    #[test]
    fn computed_new_moon_reads_as_new() {
        let t = from_julian(phase_jde(325.0)); // a 2026 new moon
        assert_eq!(phase_name(true_moon_fraction(t)).0, "New Moon");
        assert!(
            days_to_nearest_syzygy(t) < 0.05,
            "{}",
            days_to_nearest_syzygy(t)
        );
    }

    /// A computed full-moon instant must read as full and fully lit.
    #[test]
    fn computed_full_moon_reads_as_full() {
        let t = from_julian(phase_jde(325.5));
        let f = true_moon_fraction(t);
        assert_eq!(phase_name(f).0, "Full Moon");
        assert!(
            illumination(f) > 0.99,
            "illumination was {}",
            illumination(f)
        );
        assert!(days_to_nearest_syzygy(t) < 0.05);
    }

    /// Meeus ch. 49 must land the September 2026 full moon within an hour of
    /// USNO's 2026-09-26 16:49 UTC — the correction terms' whole job.
    #[test]
    fn next_full_moon_matches_usno_to_the_hour() {
        let from = Utc.with_ymd_and_hms(2026, 9, 3, 12, 0, 0).unwrap();
        let full = next_true_full_moon(from);
        let expected = Utc.with_ymd_and_hms(2026, 9, 26, 16, 49, 0).unwrap();
        let off_min = (full - expected).num_minutes().abs();
        assert!(
            off_min <= 60,
            "got {full}, USNO {expected}, off {off_min} min"
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

    // ── tide curve ──

    #[test]
    fn tide_curve_url_requests_an_hourly_48h_window() {
        let url = tide_curve_url("8762928", "20260908%2006:00");
        assert!(url.contains("interval=h"), "{url}");
        assert!(url.contains("range=48"), "{url}");
        assert!(url.contains("begin_date=20260908%2006:00"), "{url}");
        assert!(url.contains("station=8762928"), "{url}");
        assert!(url.contains("time_zone=lst_ldt"), "{url}");
    }

    #[test]
    fn tide_curve_keeps_the_full_hourly_window() {
        let start = NaiveDate::from_ymd_opt(2026, 9, 8)
            .unwrap()
            .and_hms_opt(6, 0, 0)
            .unwrap();
        let preds: Vec<Value> = (0i32..=48)
            .map(|h| {
                json!({
                    "t": (start + Duration::hours(i64::from(h)))
                        .format("%Y-%m-%d %H:%M")
                        .to_string(),
                    "v": format!("{:.3}", 1.0 + (f64::from(h) * 0.5).sin()),
                })
            })
            .collect();

        let curve = parse_tide_curve(&json!({ "predictions": preds })).unwrap();
        assert_eq!(curve.len(), 49, "one point per hour across the window");
        assert_eq!(curve.first().unwrap()["time"], "2026-09-08 06:00");
        assert_eq!(curve.last().unwrap()["time"], "2026-09-10 06:00");
        assert!(curve[1]["height_ft"].as_f64().is_some());
    }

    #[test]
    fn tide_curve_errors_rather_than_returning_an_empty_array() {
        // A NOAA error message, an empty array, a missing key, and a row with no
        // usable height all have to surface as `Err` so `fetch_tides` falls
        // back to the hi/lo list instead of handing the widget a blank graph.
        assert!(parse_tide_curve(&json!({ "error": { "message": "No data found" } })).is_err());
        assert!(parse_tide_curve(&json!({ "predictions": [] })).is_err());
        assert!(parse_tide_curve(&json!({})).is_err());
        assert!(
            parse_tide_curve(&json!({ "predictions": [{ "t": "2026-09-08 06:00" }] })).is_err(),
            "a single unusable row is not a curve"
        );
    }
}
