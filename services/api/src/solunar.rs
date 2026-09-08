//! Solunar fishing forecast — the "when" and the "how good" of a fishing day,
//! in the spirit of the lunar widget: reference content, computed locally, no
//! upstream astronomy service.
//!
//! Two independent things:
//!
//! 1. **When** — major and minor bite windows, pure astronomy, weather plays no
//!    part. Majors (~2 h) bracket the moon's upper and lower transit; minors
//!    (~1 h) bracket moonrise and moonset.
//! 2. **How good** — a 1–5 star rating from the moon phase, nudged by the
//!    barometric trend and the wind.
//!
//! The hard part is the moon's actual position. The lunar widget's phase maths
//! (mean synodic month) says nothing about *where* the moon is, which is what
//! rise/set/transit need. This uses Jean Meeus' periodic series (Astronomical
//! Algorithms, 2nd ed., ch. 47), truncated to the terms that matter at
//! arc-minute precision — good enough for rise/set within a few minutes, the
//! same bar the sunrise/sunset maths already meets.

use crate::{ApiResult, AppError, FISHING_LAT, FISHING_LON, Shared, weather};
use axum::{
    Json,
    extract::{Query, State},
};
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::America::Chicago;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::f64::consts::PI;
use std::time::Duration as StdDuration;

/// Astronomy doesn't change and the star rating only drifts with the weather;
/// an hour is plenty and matches the other feeds' order of magnitude.
const FISHING_TTL: StdDuration = StdDuration::from_secs(60 * 60);
/// Computed once, then sliced to the caller's `days`.
const MAX_DAYS: i64 = 14;
const DEFAULT_DAYS: i64 = 7;

/// Half-width of a major period (upper/lower transit ± this) — ~2 h total.
const MAJOR_HALF: i64 = 60;
/// Half-width of a minor period (moonrise/moonset ± this) — ~1 h total.
const MINOR_HALF: i64 = 30;

/// Apparent altitude of the moon's centre at rise/set: mean horizontal parallax
/// (+0.951°) less refraction (−0.567°) less the mean semidiameter (−0.259°).
/// Meeus, ch. 15.
const MOONRISE_ALT: f64 = 0.125;

const DEG: f64 = PI / 180.0;

// ─────────────────────────── moon position (Meeus ch. 47) ───────────────────────────

fn julian_centuries(jd: f64) -> f64 {
    (jd - 2_451_545.0) / 36_525.0
}

struct Ecliptic {
    lon_deg: f64,
    lat_deg: f64,
    /// Earth–moon distance in km. Not used by the forecast itself — only the
    /// angular position drives rise/set/transit — but computed alongside for
    /// the sanity test and any future perigee weighting.
    #[cfg_attr(not(test), allow(dead_code))]
    dist_km: f64,
}

/// Periodic terms for the moon's longitude (Σl, 1e-6 °) and distance
/// (Σr, 1e-3 km): argument multiples of `D, M, M', F`. Meeus table 47.A,
/// truncated — the terms below the last one kept contribute under ~0.3′.
#[rustfmt::skip]
const TERMS_LON_DIST: &[(i8, i8, i8, i8, f64, f64)] = &[
    (0, 0, 1, 0, 6_288_774.0, -20_905_355.0),
    (2, 0, -1, 0, 1_274_027.0, -3_699_111.0),
    (2, 0, 0, 0, 658_314.0, -2_955_968.0),
    (0, 0, 2, 0, 213_618.0, -569_925.0),
    (0, 1, 0, 0, -185_116.0, 48_888.0),
    (0, 0, 0, 2, -114_332.0, -3_149.0),
    (2, 0, -2, 0, 58_793.0, 246_158.0),
    (2, -1, -1, 0, 57_066.0, -152_138.0),
    (2, 0, 1, 0, 53_322.0, -170_733.0),
    (2, -1, 0, 0, 45_758.0, -204_586.0),
    (0, 1, -1, 0, -40_923.0, -129_620.0),
    (1, 0, 0, 0, -34_720.0, 108_743.0),
    (0, 1, 1, 0, -30_383.0, 104_755.0),
    (2, 0, 0, -2, 15_327.0, 10_321.0),
    (0, 0, 1, 2, -12_528.0, 0.0),
    (0, 0, 1, -2, 10_980.0, 79_661.0),
    (4, 0, -1, 0, 10_675.0, -34_782.0),
    (0, 0, 3, 0, 10_034.0, -23_210.0),
    (4, 0, -2, 0, 8_548.0, -21_636.0),
    (2, 1, -1, 0, -7_888.0, 24_208.0),
    (2, 1, 0, 0, -6_766.0, 30_824.0),
    (1, 0, -1, 0, -5_163.0, -8_379.0),
    (1, 1, 0, 0, 4_987.0, -16_675.0),
    (2, -1, 1, 0, 4_036.0, -12_831.0),
    (2, 0, 2, 0, 3_994.0, -10_445.0),
    (4, 0, 0, 0, 3_861.0, -11_650.0),
    (2, 0, -3, 0, 3_665.0, 14_403.0),
    (0, 1, -2, 0, -2_689.0, -7_003.0),
    (2, 0, -1, 2, -2_602.0, 0.0),
    (2, -1, -2, 0, 2_390.0, 10_056.0),
    (1, 0, 1, 0, -2_348.0, 6_322.0),
    (2, -2, 0, 0, 2_236.0, -9_884.0),
    (0, 1, 2, 0, -2_120.0, 5_751.0),
    (0, 2, 0, 0, -2_069.0, 0.0),
    (2, -2, -1, 0, 2_048.0, -4_950.0),
    (2, 0, 1, -2, -1_773.0, 4_130.0),
    (2, 0, 0, 2, -1_595.0, 0.0),
    (4, -1, -1, 0, 1_215.0, -3_958.0),
    (0, 0, 2, 2, -1_110.0, 0.0),
    (3, 0, -1, 0, -892.0, 3_258.0),
    (2, 1, 1, 0, -810.0, 2_616.0),
    (4, -1, -2, 0, 759.0, -1_897.0),
    (0, 2, -1, 0, -713.0, -2_117.0),
    (2, 2, -1, 0, -700.0, 2_354.0),
    (2, 1, -2, 0, 691.0, 0.0),
    (2, -1, 0, -2, 596.0, 0.0),
    (4, 0, 1, 0, 549.0, -1_423.0),
    (0, 0, 4, 0, 537.0, -1_117.0),
    (4, -1, 0, 0, 520.0, -1_571.0),
    (1, 0, -2, 0, -487.0, -1_739.0),
    (2, 1, 0, -2, -399.0, 0.0),
    (0, 0, 2, -2, -381.0, -4_421.0),
    (1, 1, 1, 0, 351.0, 0.0),
    (3, 0, -2, 0, -340.0, 0.0),
    (4, 0, -3, 0, 330.0, 0.0),
    (2, -1, 2, 0, 327.0, 0.0),
    (0, 2, 1, 0, -323.0, 1_165.0),
    (1, 1, -1, 0, 299.0, 0.0),
    (2, 0, 3, 0, 294.0, 0.0),
];

/// Periodic terms for the moon's latitude (Σb, 1e-6 °). Meeus table 47.B,
/// truncated.
#[rustfmt::skip]
const TERMS_LAT: &[(i8, i8, i8, i8, f64)] = &[
    (0, 0, 0, 1, 5_128_122.0),
    (0, 0, 1, 1, 280_602.0),
    (0, 0, 1, -1, 277_693.0),
    (2, 0, 0, -1, 173_237.0),
    (2, 0, -1, 1, 55_413.0),
    (2, 0, -1, -1, 46_271.0),
    (2, 0, 0, 1, 32_573.0),
    (0, 0, 2, 1, 17_198.0),
    (2, 0, 1, -1, 9_266.0),
    (0, 0, 2, -1, 8_822.0),
    (2, -1, 0, -1, 8_216.0),
    (2, 0, -2, -1, 4_324.0),
    (2, 0, 1, 1, 4_200.0),
    (2, 1, 0, -1, -3_359.0),
    (2, -1, -1, 1, 2_463.0),
    (2, -1, 0, 1, 2_211.0),
    (2, -1, -1, -1, 2_065.0),
    (0, 1, -1, -1, -1_870.0),
    (4, 0, -1, -1, 1_828.0),
    (0, 1, 0, 1, -1_794.0),
    (0, 0, 0, 3, -1_749.0),
    (0, 1, -1, 1, -1_565.0),
    (1, 0, 0, 1, -1_491.0),
    (0, 1, 1, 1, -1_475.0),
    (0, 1, 1, -1, -1_410.0),
    (0, 1, 0, -1, -1_344.0),
    (1, 0, 0, -1, -1_335.0),
    (0, 0, 3, 1, 1_107.0),
    (4, 0, 0, -1, 1_021.0),
    (4, 0, -1, 1, 833.0),
    (0, 0, 1, -3, 777.0),
    (4, 0, -2, 1, 671.0),
    (2, 0, 0, -3, 607.0),
    (2, 0, 2, -1, 596.0),
    (2, -1, 1, -1, 491.0),
    (2, 0, -2, 1, -451.0),
    (0, 0, 3, -1, 439.0),
    (2, 0, 2, 1, 422.0),
    (2, 0, -3, -1, 421.0),
];

/// Geocentric ecliptic position of the moon at Julian date `jd`.
fn moon_ecliptic(jd: f64) -> Ecliptic {
    let t = julian_centuries(jd);
    let t2 = t * t;
    let t3 = t2 * t;
    let t4 = t3 * t;

    // Mean elements (Meeus 47.1–47.5), degrees.
    let lp = (218.316_447_7 + 481_267.881_234_21 * t - 0.001_578_6 * t2 + t3 / 538_841.0
        - t4 / 65_194_000.0)
        .rem_euclid(360.0);
    let d = (297.850_192_1 + 445_267.111_403_4 * t - 0.001_881_9 * t2 + t3 / 545_868.0
        - t4 / 113_065_000.0)
        .rem_euclid(360.0);
    let m = (357.529_109_2 + 35_999.050_290_9 * t - 0.000_153_6 * t2 + t3 / 24_490_000.0)
        .rem_euclid(360.0);
    let mp = (134.963_396_4 + 477_198.867_505_5 * t + 0.008_741_4 * t2 + t3 / 69_699.0
        - t4 / 14_712_000.0)
        .rem_euclid(360.0);
    let f = (93.272_095_0 + 483_202.017_523_3 * t - 0.003_653_9 * t2 - t3 / 3_526_000.0
        + t4 / 863_310_000.0)
        .rem_euclid(360.0);

    let a1 = (119.75 + 131.849 * t).rem_euclid(360.0);
    let a2 = (53.09 + 479_264.290 * t).rem_euclid(360.0);
    let a3 = (313.45 + 481_266.484 * t).rem_euclid(360.0);
    // Earth's orbital eccentricity correction; applied once per |M|, squared for |M|=2.
    let e = 1.0 - 0.002_516 * t - 0.000_007_4 * t2;

    let ecc = |cm: i8| match cm.abs() {
        1 => e,
        2 => e * e,
        _ => 1.0,
    };

    let mut sum_l = 0.0;
    let mut sum_r = 0.0;
    for &(cd, cm, cmp, cf, sl, sr) in TERMS_LON_DIST {
        let arg =
            (f64::from(cd) * d + f64::from(cm) * m + f64::from(cmp) * mp + f64::from(cf) * f) * DEG;
        sum_l += sl * ecc(cm) * arg.sin();
        sum_r += sr * ecc(cm) * arg.cos();
    }
    let mut sum_b = 0.0;
    for &(cd, cm, cmp, cf, sb) in TERMS_LAT {
        let arg =
            (f64::from(cd) * d + f64::from(cm) * m + f64::from(cmp) * mp + f64::from(cf) * f) * DEG;
        sum_b += sb * ecc(cm) * arg.sin();
    }

    // Additive terms from Venus, Jupiter and Earth's flattening (Meeus, p. 342).
    sum_l +=
        3_958.0 * (a1 * DEG).sin() + 1_962.0 * ((lp - f) * DEG).sin() + 318.0 * (a2 * DEG).sin();
    sum_b += -2_235.0 * (lp * DEG).sin()
        + 382.0 * (a3 * DEG).sin()
        + 175.0 * ((a1 - f) * DEG).sin()
        + 175.0 * ((a1 + f) * DEG).sin()
        + 127.0 * ((lp - mp) * DEG).sin()
        - 115.0 * ((lp + mp) * DEG).sin();

    Ecliptic {
        lon_deg: (lp + sum_l / 1_000_000.0).rem_euclid(360.0),
        lat_deg: sum_b / 1_000_000.0,
        dist_km: 385_000.56 + sum_r / 1_000.0,
    }
}

/// Right ascension and declination (degrees) of the moon at `jd`, mean equinox
/// of date. Nutation (~17″) is below our precision and skipped.
fn moon_equatorial(jd: f64) -> (f64, f64) {
    let p = moon_ecliptic(jd);
    let t = julian_centuries(jd);
    let eps = (23.439_291 - 0.013_004_2 * t) * DEG;
    let lon = p.lon_deg * DEG;
    let lat = p.lat_deg * DEG;

    let ra = (lon.sin() * eps.cos() - lat.tan() * eps.sin()).atan2(lon.cos());
    let dec = (lat.sin() * eps.cos() + lat.cos() * eps.sin() * lon.sin()).asin();
    (ra.to_degrees().rem_euclid(360.0), dec.to_degrees())
}

/// Greenwich mean sidereal time (degrees) at `jd`. Meeus 12.4.
fn gmst_deg(jd: f64) -> f64 {
    let t = julian_centuries(jd);
    (280.460_618_37 + 360.985_647_366_29 * (jd - 2_451_545.0) + 0.000_387_933 * t * t
        - t * t * t / 38_710_000.0)
        .rem_euclid(360.0)
}

/// Moon altitude (degrees) and local hour angle (degrees, 0–360) at `jd` for
/// the fishing grounds. Geocentric — topocentric parallax shifts rise/set by a
/// minute or two at most, inside our tolerance.
fn altitude_and_hour_angle(jd: f64) -> (f64, f64) {
    let (ra, dec) = moon_equatorial(jd);
    let lst = (gmst_deg(jd) + FISHING_LON).rem_euclid(360.0);
    let ha = (lst - ra).rem_euclid(360.0);
    let sin_alt = (FISHING_LAT * DEG).sin() * (dec * DEG).sin()
        + (FISHING_LAT * DEG).cos() * (dec * DEG).cos() * (ha * DEG).cos();
    (sin_alt.clamp(-1.0, 1.0).asin().to_degrees(), ha)
}

// ─────────────────────────── rise / set / transit ───────────────────────────

#[derive(Default)]
struct MoonEvents {
    moonrise: Option<DateTime<Utc>>,
    moonset: Option<DateTime<Utc>>,
    upper_transit: Option<DateTime<Utc>>,
    lower_transit: Option<DateTime<Utc>>,
}

fn to_utc(naive: NaiveDateTime) -> DateTime<Utc> {
    Chicago
        .from_local_datetime(&naive)
        .earliest()
        .or_else(|| Chicago.from_local_datetime(&naive).latest())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| Utc.from_utc_datetime(&naive))
}

fn lerp_time(a: DateTime<Utc>, b: DateTime<Utc>, frac: f64) -> DateTime<Utc> {
    let span = (b - a).num_milliseconds() as f64;
    a + Duration::milliseconds((span * frac.clamp(0.0, 1.0)) as i64)
}

/// Sweeps the local calendar day at two-minute steps, catching the horizon
/// crossings (rise/set) and the meridian crossings (upper/lower transit) whose
/// interpolated instant lands inside the day.
///
/// Hour angle grows monotonically (~14.5°/h net), so unwrapping it and watching
/// for passages of `k·180°` gives the transits: even `k` is the upper (culmination),
/// odd `k` the lower.
fn moon_events(date: NaiveDate) -> MoonEvents {
    let day_start = to_utc(date.and_hms_opt(0, 0, 0).expect("valid midnight"));
    let day_end = to_utc(
        (date + Duration::days(1))
            .and_hms_opt(0, 0, 0)
            .expect("valid midnight"),
    );
    let step = Duration::minutes(2);

    let mut ev = MoonEvents::default();
    let mut prev: Option<(DateTime<Utc>, f64, f64)> = None; // (t, altitude, unwrapped HA)

    let mut t = day_start - step;
    while t <= day_end + step {
        let (alt, ha_raw) = altitude_and_hour_angle(weather::to_julian(t));

        let ha_unwrapped = match prev {
            None => ha_raw,
            Some((_, _, prev_ha)) => {
                let base = prev_ha.rem_euclid(360.0);
                let mut delta = ha_raw - base;
                if delta < -180.0 {
                    delta += 360.0;
                } else if delta > 180.0 {
                    delta -= 360.0;
                }
                prev_ha + delta
            }
        };

        if let Some((pt, palt, prev_ha)) = prev {
            let within = |x: DateTime<Utc>| x >= day_start && x < day_end;

            // Horizon crossings.
            if palt < MOONRISE_ALT && alt >= MOONRISE_ALT {
                let when = lerp_time(pt, t, (MOONRISE_ALT - palt) / (alt - palt));
                if within(when) && ev.moonrise.is_none() {
                    ev.moonrise = Some(when);
                }
            } else if palt >= MOONRISE_ALT && alt < MOONRISE_ALT {
                let when = lerp_time(pt, t, (palt - MOONRISE_ALT) / (palt - alt));
                if within(when) && ev.moonset.is_none() {
                    ev.moonset = Some(when);
                }
            }

            // Meridian crossings: does (prev_ha, ha_unwrapped) span a k·180°?
            let k_prev = (prev_ha / 180.0).floor() as i64;
            let k_now = (ha_unwrapped / 180.0).floor() as i64;
            if k_now > k_prev {
                let target = f64::from((k_prev + 1) as i32) * 180.0;
                let when = lerp_time(pt, t, (target - prev_ha) / (ha_unwrapped - prev_ha));
                if within(when) {
                    if (k_prev + 1).rem_euclid(2) == 0 {
                        ev.upper_transit.get_or_insert(when);
                    } else {
                        ev.lower_transit.get_or_insert(when);
                    }
                }
            }
        }

        prev = Some((t, alt, ha_unwrapped));
        t += step;
    }

    ev
}

// ─────────────────────────── star rating ───────────────────────────

/// Baseline from the moon phase, per the spec: the syzygies score highest, the
/// quarters lowest, the shoulders in between.
fn phase_baseline(phase: &str) -> i32 {
    match phase {
        "New Moon" | "Full Moon" => 5,
        "First Quarter" | "Last Quarter" => 3,
        _ => 4, // waxing/waning crescent and gibbous
    }
}

fn rating_label(stars: i32) -> &'static str {
    match stars {
        5 => "Excellent",
        4 => "Good",
        3 => "Fair",
        _ => "Poor",
    }
}

fn star_rating(phase: &str, pressure_mod: i32, wind_mod: i32) -> i32 {
    (phase_baseline(phase) + pressure_mod + wind_mod).clamp(1, 5)
}

// ─────────────────────────── HTTP ───────────────────────────

#[derive(Deserialize)]
pub struct ForecastParams {
    days: Option<i64>,
}

/// `GET /api/fishing-forecast?days=7` — public, no auth. Default seven days
/// (today plus the next six).
pub async fn fishing_forecast(
    State(state): State<Shared>,
    Query(params): Query<ForecastParams>,
) -> ApiResult<Json<Value>> {
    let days = params.days.unwrap_or(DEFAULT_DAYS).clamp(1, MAX_DAYS);

    if let Some(cached) = weather::cached(&state.cache.fishing, FISHING_TTL).await {
        return Ok(Json(slice_days(cached, days)));
    }

    let full = build_forecast(&state).await.map_err(AppError::Internal)?;
    weather::store(&state.cache.fishing, full.clone()).await;
    Ok(Json(slice_days(full, days)))
}

/// The cached payload always holds `MAX_DAYS`; trim it to what was asked for.
fn slice_days(mut payload: Value, days: i64) -> Value {
    if let Some(arr) = payload["days"].as_array_mut() {
        arr.truncate(days as usize);
    }
    payload
}

async fn build_forecast(state: &Shared) -> anyhow::Result<Value> {
    let now = Utc::now();
    let today = now.with_timezone(&Chicago).date_naive();

    // Weather inputs are best-effort: the bite windows stand on their own, and a
    // missing feed just means the rating falls back to the phase alone.
    let observations = weather::recent_observations(state)
        .await
        .unwrap_or_default();
    let (trend, _change) = weather::pressure_trend(&observations);
    let pressure_mod = match trend {
        "falling" => 1,
        "rising" => -1,
        _ => 0,
    };
    let live_wind_mph = observations
        .first()
        .and_then(|o| o.wind_kmh)
        .map(weather::kmh_to_mph);
    let forecast_wind = forecast_wind_by_day(state).await.unwrap_or_default();

    let mut days = Vec::with_capacity(MAX_DAYS as usize);
    for offset in 0..MAX_DAYS {
        let date = today + Duration::days(offset);
        // Sample the phase at local noon, as the lunar widget does.
        let noon = to_utc(date.and_hms_opt(12, 0, 0).expect("valid noon"));
        let (phase, emoji) = weather::phase_name(weather::moon_fraction(noon));

        let events = moon_events(date);

        // Wind: the live reading for today (freshest), the forecast otherwise.
        let wind_mph = if offset == 0 {
            live_wind_mph.or_else(|| forecast_wind.get(&date).copied())
        } else {
            forecast_wind.get(&date).copied()
        };
        let wind_mod = if wind_mph.is_some_and(|w| w > 15.0) {
            -1
        } else {
            0
        };
        // The barometric trend is a *now* signal; only today gets it.
        let pressure_mod = if offset == 0 { pressure_mod } else { 0 };
        let stars = star_rating(phase, pressure_mod, wind_mod);

        days.push(json!({
            "date": date,
            "stars": stars,
            "rating_label": rating_label(stars),
            "moon_phase": phase,
            "moon_emoji": emoji,
            "major_periods": periods(&[events.upper_transit, events.lower_transit], MAJOR_HALF),
            "minor_periods": periods(&[events.moonrise, events.moonset], MINOR_HALF),
        }));
    }

    Ok(json!({
        "location": "Cocodrie estuary, Louisiana",
        "timezone": "America/Chicago",
        "generated_at": now,
        "disclaimer": "Based on solunar theory — a fun guide, not a guarantee!",
        "days": days,
    }))
}

/// `{start, end}` in local `HH:MM` for each present centre, sorted by start.
fn periods(centers: &[Option<DateTime<Utc>>], half: i64) -> Vec<Value> {
    let mut spans: Vec<(DateTime<Utc>, DateTime<Utc>)> = centers
        .iter()
        .flatten()
        .map(|&c| (c - Duration::minutes(half), c + Duration::minutes(half)))
        .collect();
    spans.sort_by_key(|(start, _)| *start);
    spans
        .into_iter()
        .map(|(start, end)| json!({ "start": hhmm(start), "end": hhmm(end) }))
        .collect()
}

fn hhmm(dt: DateTime<Utc>) -> String {
    dt.with_timezone(&Chicago).format("%H:%M").to_string()
}

/// Max sustained wind (mph) the daytime forecast gives for each date. NOAA
/// phrases it as `"10 mph"` or `"10 to 15 mph"`; take the larger number.
async fn forecast_wind_by_day(state: &Shared) -> anyhow::Result<HashMap<NaiveDate, f64>> {
    let points = weather::fetch_json(
        state,
        &format!("https://api.weather.gov/points/{FISHING_LAT},{FISHING_LON}"),
    )
    .await?;
    let url = points["properties"]["forecast"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("no forecast url in points response"))?;
    let forecast = weather::fetch_json(state, url).await?;

    let mut out = HashMap::new();
    for p in forecast["properties"]["periods"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if !p["isDaytime"].as_bool().unwrap_or(true) {
            continue;
        }
        let Some(date) = p["startTime"]
            .as_str()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Chicago).date_naive())
        else {
            continue;
        };
        if let Some(w) = p["windSpeed"].as_str().and_then(parse_wind_mph) {
            out.insert(date, w);
        }
    }
    Ok(out)
}

fn parse_wind_mph(text: &str) -> Option<f64> {
    text.split(|c: char| !c.is_ascii_digit())
        .filter_map(|piece| piece.parse::<f64>().ok())
        .fold(None, |acc, n| Some(acc.map_or(n, |m: f64| m.max(n))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    /// Local (America/Chicago) minute-of-day for a UTC instant.
    fn local_minutes(dt: DateTime<Utc>) -> i64 {
        let l = dt.with_timezone(&Chicago);
        i64::from(l.hour()) * 60 + i64::from(l.minute())
    }

    fn parse_hm(hm: &str) -> i64 {
        let (h, m) = hm.split_once(':').unwrap();
        h.parse::<i64>().unwrap() * 60 + m.parse::<i64>().unwrap()
    }

    /// Assert `got` is within `tol` minutes of a local `HH:MM`, tolerant of the
    /// midnight wrap.
    #[track_caller]
    fn assert_local_near(got: Option<DateTime<Utc>>, want_local: &str, tol: i64) {
        let got = got.unwrap_or_else(|| panic!("expected an event near {want_local}, got none"));
        let g = local_minutes(got);
        let w = parse_hm(want_local);
        let diff = [(g - w).abs(), (g - w + 1440).abs(), (g - w - 1440).abs()]
            .into_iter()
            .min()
            .unwrap();
        assert!(
            diff <= tol,
            "expected ~{want_local} local, got {} (off by {diff} min)",
            got.with_timezone(&Chicago).format("%H:%M")
        );
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    // Reference values: U.S. Naval Observatory rise/set/transit tables
    // (aa.usno.navy.mil) for 29.245 N, 90.662 W — the Cocodrie estuary. Local
    // civil time (CST in January, CDT otherwise). Truncating the Meeus series
    // and using a geocentric position costs a few minutes at the horizon, so
    // rise/set allow ±10 min and the transit ±6.

    #[test]
    fn moon_events_winter_2026_01_15() {
        let ev = moon_events(date(2026, 1, 15));
        assert_local_near(ev.moonrise, "04:35", 5);
        assert_local_near(ev.upper_transit, "09:36", 4);
        assert_local_near(ev.moonset, "14:36", 5);
    }

    #[test]
    fn moon_events_summer_2026_06_15() {
        let ev = moon_events(date(2026, 6, 15));
        assert_local_near(ev.moonrise, "06:17", 5);
        assert_local_near(ev.upper_transit, "13:45", 4);
        assert_local_near(ev.moonset, "21:11", 5);
    }

    #[test]
    fn moon_events_autumn_2026_09_08() {
        let ev = moon_events(date(2026, 9, 8));
        assert_local_near(ev.moonrise, "03:58", 5);
        assert_local_near(ev.upper_transit, "10:58", 4);
        assert_local_near(ev.moonset, "17:51", 5);
    }

    /// Full moon, and a day whose upper transit falls just after midnight — the
    /// sweep must still catch it, plus the evening rise and the morning set.
    #[test]
    fn moon_events_full_moon_2026_09_26() {
        let ev = moon_events(date(2026, 9, 26));
        assert_local_near(ev.upper_transit, "00:29", 4);
        assert_local_near(ev.moonset, "06:46", 5);
        assert_local_near(ev.moonrise, "18:50", 5);
    }

    /// The two majors of a lunar day sit a half lunar day apart (~12 h 25 m),
    /// and the lower transit is below the horizon.
    #[test]
    fn transits_are_internally_consistent() {
        let ev = moon_events(date(2026, 6, 15));
        let upper = ev.upper_transit.expect("upper transit");
        let lower = ev.lower_transit.expect("lower transit");
        let gap = (upper - lower).num_minutes().abs();
        assert!(
            (720..=780).contains(&gap),
            "majors {gap} min apart, expected ~745"
        );
        let (alt, _) = altitude_and_hour_angle(weather::to_julian(lower));
        assert!(
            alt < 0.0,
            "lower transit should be below the horizon, alt {alt}"
        );
    }

    /// Cross-check against a *published solunar table* (not just USNO): the
    /// Cocodrie major/minor windows for 2026-09-08 from fishingreminder.com
    /// were 09:54–11:54 & 22:28–00:28 (majors) and centred on moonrise 03:58 /
    /// moonset 17:51 (minors). Ours should line up within a few minutes.
    #[test]
    fn matches_published_solunar_table_2026_09_08() {
        let ev = moon_events(date(2026, 9, 8));

        let majors = periods(&[ev.upper_transit, ev.lower_transit], MAJOR_HALF);
        assert_eq!(majors.len(), 2);
        // First major brackets the ~10:56 transit — table had 09:54–11:54.
        let start0 = majors[0]["start"].as_str().unwrap();
        assert!(start0.starts_with("09:5"), "major start was {start0}");
        let end0 = majors[0]["end"].as_str().unwrap();
        assert!(end0.starts_with("11:5"), "major end was {end0}");

        // Minors are centred on rise and set — table had them on 03:58 / 17:51.
        let minors = periods(&[ev.moonrise, ev.moonset], MINOR_HALF);
        assert_eq!(minors.len(), 2);
        assert_local_near(ev.moonrise, "03:58", 5);
        assert_local_near(ev.moonset, "17:51", 5);
    }

    #[test]
    #[ignore = "diagnostic: prints deviation from USNO reference"]
    fn print_accuracy() {
        let cases: &[(NaiveDate, &str, &str, &str)] = &[
            (date(2026, 1, 15), "04:35", "09:36", "14:36"),
            (date(2026, 6, 15), "06:17", "13:45", "21:11"),
            (date(2026, 9, 8), "03:58", "10:58", "17:51"),
            (date(2026, 9, 26), "18:50", "00:29", "06:46"),
        ];
        for (d, rise, tr, set) in cases {
            let ev = moon_events(*d);
            let diff = |got: Option<DateTime<Utc>>, want: &str| {
                let g = local_minutes(got.unwrap());
                let w = parse_hm(want);
                [(g - w).abs(), (g - w + 1440).abs(), (g - w - 1440).abs()]
                    .into_iter()
                    .min()
                    .unwrap()
            };
            eprintln!(
                "{d}: rise {:+} min, upper transit {:+} min, set {:+} min",
                diff(ev.moonrise, rise),
                diff(ev.upper_transit, tr),
                diff(ev.moonset, set),
            );
        }
    }

    #[test]
    fn phase_baseline_scores() {
        assert_eq!(phase_baseline("New Moon"), 5);
        assert_eq!(phase_baseline("Full Moon"), 5);
        assert_eq!(phase_baseline("First Quarter"), 3);
        assert_eq!(phase_baseline("Last Quarter"), 3);
        assert_eq!(phase_baseline("Waxing Crescent"), 4);
        assert_eq!(phase_baseline("Waning Gibbous"), 4);
    }

    #[test]
    fn star_rating_clamps_and_combines() {
        // New moon, pressure falling, calm: 5 + 1 + 0 clamps to 5.
        assert_eq!(star_rating("New Moon", 1, 0), 5);
        // Full moon, pressure rising, blowing 20: 5 - 1 - 1 = 3.
        assert_eq!(star_rating("Full Moon", -1, -1), 3);
        // Quarter, rising, windy: 3 - 1 - 1 = 1 (floor).
        assert_eq!(star_rating("First Quarter", -1, -1), 1);
        // Quarter, rising glass, windy can't go below 1.
        assert_eq!(star_rating("Last Quarter", -1, -1), 1);
        // Crescent baseline, everything neutral.
        assert_eq!(star_rating("Waxing Crescent", 0, 0), 4);
    }

    #[test]
    fn rating_labels_map() {
        assert_eq!(rating_label(5), "Excellent");
        assert_eq!(rating_label(4), "Good");
        assert_eq!(rating_label(3), "Fair");
        assert_eq!(rating_label(2), "Poor");
        assert_eq!(rating_label(1), "Poor");
    }

    #[test]
    fn wind_parsing() {
        assert_eq!(parse_wind_mph("10 mph"), Some(10.0));
        assert_eq!(parse_wind_mph("10 to 15 mph"), Some(15.0));
        assert_eq!(parse_wind_mph("calm"), None);
        assert_eq!(parse_wind_mph("5 to 20 mph"), Some(20.0));
    }

    /// The moon's distance stays inside its real perigee/apogee envelope, a
    /// coarse guard that the series isn't wildly off.
    #[test]
    fn moon_distance_is_plausible() {
        for jd_offset in 0..28 {
            let jd = weather::to_julian(Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap())
                + f64::from(jd_offset);
            let d = moon_ecliptic(jd).dist_km;
            assert!(
                (356_000.0..=407_000.0).contains(&d),
                "moon distance {d} km out of range"
            );
        }
    }
}
