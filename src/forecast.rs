//! The weather, from Open-Meteo (open-meteo.com): free, no key, no account.
//! One request brings the current readings, 7 days of hourly forecast and
//! the daily highs, lows and sun times. Parsing is pure and unit-tested;
//! `fetch` is the only part that touches the network.

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Units {
    #[default]
    Metric,
    Imperial,
}

impl Units {
    pub fn temp(self) -> &'static str {
        match self {
            Units::Metric => "C",
            Units::Imperial => "F",
        }
    }

    pub fn speed(self) -> &'static str {
        match self {
            Units::Metric => "km/h",
            Units::Imperial => "mph",
        }
    }

    pub fn rain(self) -> &'static str {
        match self {
            Units::Metric => "mm",
            Units::Imperial => "in",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Units::Metric => "metric",
            Units::Imperial => "imperial",
        }
    }

    pub fn wind_max(self) -> f32 {
        match self {
            Units::Metric => 80.,
            Units::Imperial => 50.,
        }
    }
}

/// Where to ask about: typed by the user as a place name or `lat,lon`.
#[derive(Clone, Debug, PartialEq)]
pub enum Location {
    Place(String),
    Coords(f64, f64),
}

impl Location {
    pub fn parse(s: &str) -> Option<Location> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        if let Some((a, b)) = s.split_once(',')
            && let (Ok(lat), Ok(lon)) = (a.trim().parse::<f64>(), b.trim().parse::<f64>())
        {
            return ((-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon)).then_some(Location::Coords(lat, lon));
        }
        Some(Location::Place(s.to_string()))
    }
}

/// A place resolved to coordinates, with a name to show.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
}

/// Everything the station shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Forecast {
    pub place: String,
    pub units: Units,
    /// Local wall time of the reading, `YYYY-MM-DDTHH:MM`.
    pub time: String,
    pub temp: f32,
    pub feels: f32,
    pub humidity: f32,
    /// Sea-level pressure, hPa.
    pub pressure: f32,
    pub wind: f32,
    /// Degrees the wind blows *from*.
    pub wind_dir: f32,
    pub code: u32,
    /// Cloud cover, 0–100.
    pub cloud: f32,
    /// Hourly, from the start of today, 7 days (168 entries when complete).
    pub hours: Vec<Hour>,
    pub days: Vec<Day>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hour {
    pub time: String,
    pub temp: f32,
    /// Chance of precipitation, 0–100.
    pub chance: f32,
    /// Precipitation in the hour, mm or in.
    pub amount: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Day {
    pub date: String,
    pub high: f32,
    pub low: f32,
    pub rain: f32,
    pub code: u32,
    pub sunrise: String,
    pub sunset: String,
}

impl Forecast {
    /// The index of the current hour in `hours`.
    pub fn now_index(&self) -> usize {
        let hour = &self.time[..self.time.len().min(13)];
        self.hours.iter().position(|h| h.time.starts_with(hour)).unwrap_or(0)
    }

    /// The next `n` hours from now.
    pub fn next(&self, n: usize) -> &[Hour] {
        let i = self.now_index().min(self.hours.len());
        &self.hours[i..(i + n).min(self.hours.len())]
    }

    /// The week as 7 rows of 24 temperatures, each scaled 0–1 across the
    /// week's own range, for the heatmap. Missing hours read as 0.
    pub fn week_grid(&self) -> Vec<Vec<f32>> {
        let temps: Vec<f32> = self.hours.iter().map(|h| h.temp).collect();
        let (lo, hi) = temps.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &t| (lo.min(t), hi.max(t)));
        let span = (hi - lo).max(1.);
        (0..7)
            .map(|d| (0..24).map(|h| temps.get(d * 24 + h).map(|t| ((t - lo) / span).clamp(0.02, 1.)).unwrap_or(0.)).collect())
            .collect()
    }

    /// How far through today's daylight the reading is, 0–1 (0 before
    /// sunrise, 1 after sunset).
    pub fn daylight(&self) -> Option<f32> {
        let day = self.days.first()?;
        let (rise, set, now) = (minutes(&day.sunrise)?, minutes(&day.sunset)?, minutes(&self.time)?);
        (set > rise).then(|| ((now - rise) as f32 / (set - rise) as f32).clamp(0., 1.))
    }

    pub fn description(&self) -> &'static str {
        describe(self.code)
    }
}

/// Minutes since midnight of an ISO local time, `YYYY-MM-DDTHH:MM`.
pub fn minutes(iso: &str) -> Option<i32> {
    let (_, t) = iso.split_once('T')?;
    let (h, m) = t.split_once(':')?;
    Some(h.parse::<i32>().ok()? * 60 + m.get(..2)?.parse::<i32>().ok()?)
}

/// `HH:MM` of an ISO local time.
pub fn clock(iso: &str) -> &str {
    iso.split_once('T').map(|(_, t)| t.get(..5).unwrap_or(t)).unwrap_or(iso)
}

/// WMO weather interpretation codes, as Open-Meteo reports them.
pub fn describe(code: u32) -> &'static str {
    match code {
        0 => "clear sky",
        1 => "mainly clear",
        2 => "partly cloudy",
        3 => "overcast",
        45 | 48 => "fog",
        51 | 53 | 55 => "drizzle",
        56 | 57 => "freezing drizzle",
        61 => "light rain",
        63 => "rain",
        65 => "heavy rain",
        66 | 67 => "freezing rain",
        71 => "light snow",
        73 => "snow",
        75 => "heavy snow",
        77 => "snow grains",
        80..=82 => "rain showers",
        85 | 86 => "snow showers",
        95 => "thunderstorm",
        96 | 99 => "thunderstorm, hail",
        _ => "unknown",
    }
}

/// A reading rounded for display: never "-0".
pub fn whole(v: f32) -> i32 {
    v.round() as i32
}

/// A compass point for a bearing: 0 → N, 90 → E.
pub fn compass(deg: f32) -> &'static str {
    const POINTS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    POINTS[((deg.rem_euclid(360.) + 22.5) / 45.) as usize % 8]
}

fn num(v: &Value) -> f32 {
    v.as_f64().unwrap_or(0.) as f32
}

fn nums(v: &Value, key: &str) -> Vec<f32> {
    v[key].as_array().map(|a| a.iter().map(num).collect()).unwrap_or_default()
}

fn strs(v: &Value, key: &str) -> Vec<String> {
    v[key].as_array().map(|a| a.iter().map(|s| s.as_str().unwrap_or_default().to_string()).collect()).unwrap_or_default()
}

/// Parse Open-Meteo's forecast response.
pub fn parse(body: &str, place: &str, units: Units) -> Result<Forecast, String> {
    let v: Value = serde_json::from_str(body).map_err(|_| "Open-Meteo sent something that isn't JSON".to_string())?;
    if v["error"].as_bool() == Some(true) {
        return Err(format!("Open-Meteo: {}", v["reason"].as_str().unwrap_or("request refused")));
    }
    let c = &v["current"];
    let time = c["time"].as_str().ok_or("Open-Meteo's response has no current reading")?.to_string();

    let h = &v["hourly"];
    let (times, temps, chances, amounts) = (strs(h, "time"), nums(h, "temperature_2m"), nums(h, "precipitation_probability"), nums(h, "precipitation"));
    let hours = times
        .into_iter()
        .enumerate()
        .map(|(i, time)| Hour {
            time,
            temp: temps.get(i).copied().unwrap_or(0.),
            chance: chances.get(i).copied().unwrap_or(0.),
            amount: amounts.get(i).copied().unwrap_or(0.),
        })
        .collect();

    let d = &v["daily"];
    let (dates, highs, lows, rains, codes, rises, sets) = (
        strs(d, "time"),
        nums(d, "temperature_2m_max"),
        nums(d, "temperature_2m_min"),
        nums(d, "precipitation_sum"),
        nums(d, "weather_code"),
        strs(d, "sunrise"),
        strs(d, "sunset"),
    );
    let days = dates
        .into_iter()
        .enumerate()
        .map(|(i, date)| Day {
            date,
            high: highs.get(i).copied().unwrap_or(0.),
            low: lows.get(i).copied().unwrap_or(0.),
            rain: rains.get(i).copied().unwrap_or(0.),
            code: codes.get(i).copied().unwrap_or(0.) as u32,
            sunrise: rises.get(i).cloned().unwrap_or_default(),
            sunset: sets.get(i).cloned().unwrap_or_default(),
        })
        .collect();

    Ok(Forecast {
        place: place.to_string(),
        units,
        time,
        temp: num(&c["temperature_2m"]),
        feels: num(&c["apparent_temperature"]),
        humidity: num(&c["relative_humidity_2m"]),
        pressure: num(&c["pressure_msl"]),
        wind: num(&c["wind_speed_10m"]),
        wind_dir: num(&c["wind_direction_10m"]),
        code: c["weather_code"].as_u64().unwrap_or(0) as u32,
        cloud: num(&c["cloud_cover"]),
        hours,
        days,
    })
}

/// Parse Open-Meteo's geocoding response: the best match, or why there's none.
pub fn parse_place(body: &str, query: &str) -> Result<Place, String> {
    let v: Value = serde_json::from_str(body).map_err(|_| "the geocoder sent something that isn't JSON".to_string())?;
    let r = v["results"].get(0).ok_or_else(|| format!("no place called \"{query}\""))?;
    let name = r["name"].as_str().unwrap_or(query);
    let name = match r["country_code"].as_str() {
        Some(cc) => format!("{name}, {cc}"),
        None => name.to_string(),
    };
    Ok(Place { name, lat: r["latitude"].as_f64().ok_or("the geocoder gave no latitude")?, lon: r["longitude"].as_f64().ok_or("the geocoder gave no longitude")? })
}

fn get(req: ureq::RequestBuilder<ureq::typestate::WithoutBody>) -> Result<String, String> {
    let mut resp = req.call().map_err(|e| match e {
        ureq::Error::StatusCode(code) => format!("Open-Meteo returned HTTP {code}"),
        _ => "couldn't reach Open-Meteo".to_string(),
    })?;
    resp.body_mut().read_to_string().map_err(|_| "couldn't read Open-Meteo's response".to_string())
}

/// Turn what the user typed into coordinates (a network call for a name).
pub fn resolve(location: &Location) -> Result<Place, String> {
    match location {
        Location::Coords(lat, lon) => Ok(Place { name: format!("{lat:.2}, {lon:.2}"), lat: *lat, lon: *lon }),
        Location::Place(q) => {
            // "Fargo, US" → search "Fargo"; the geocoder doesn't take a country suffix.
            let name = q.split(',').next().unwrap_or(q).trim();
            let body = get(ureq::get("https://geocoding-api.open-meteo.com/v1/search").query("name", name).query("count", "1"))?;
            parse_place(&body, q)
        }
    }
}

pub fn fetch(place: &Place, units: Units) -> Result<Forecast, String> {
    let mut req = ureq::get("https://api.open-meteo.com/v1/forecast")
        .query("latitude", place.lat.to_string())
        .query("longitude", place.lon.to_string())
        .query(
            "current",
            "temperature_2m,relative_humidity_2m,apparent_temperature,weather_code,cloud_cover,pressure_msl,wind_speed_10m,wind_direction_10m",
        )
        .query("hourly", "temperature_2m,precipitation_probability,precipitation")
        .query("daily", "temperature_2m_max,temperature_2m_min,precipitation_sum,weather_code,sunrise,sunset")
        .query("timezone", "auto")
        .query("forecast_days", "7");
    if units == Units::Imperial {
        req = req.query("temperature_unit", "fahrenheit").query("wind_speed_unit", "mph").query("precipitation_unit", "inch");
    }
    parse(&get(req)?, &place.name, units)
}

/// A made-up but plausible week, for `--demo` and for before a location is
/// set: a daily temperature swing, a front with rain on day two, a storm on
/// day five.
pub fn demo(date: chrono::NaiveDate, hour: u32, units: Units) -> Forecast {
    use chrono::Duration;
    let conv = |c: f32| if units == Units::Imperial { c * 9. / 5. + 32. } else { c };
    let rain_at = |i: usize| -> f32 {
        let i = i as f32;
        let bump = |center: f32, width: f32, peak: f32| peak * (-((i - center) / width).powi(2)).exp();
        bump(hour as f32 + 5., 2.5, 0.9) + bump(30., 4., 0.7) + bump(100., 5., 1.)
    };
    let hours: Vec<Hour> = (0..168)
        .map(|i| {
            let t = 9. + 7. * (((i % 24) as f32 - 9.) / 24. * std::f32::consts::TAU).sin() - (i as f32 / 40.) + 3. * (i as f32 / 70.).cos();
            let r = rain_at(i).min(1.);
            Hour {
                time: format!("{}T{:02}:00", (date + Duration::days(i as i64 / 24)).format("%Y-%m-%d"), i % 24),
                temp: conv(t),
                chance: (r * 100.).round(),
                amount: if r > 0.3 { (r - 0.3) * if units == Units::Imperial { 0.12 } else { 3. } } else { 0. },
            }
        })
        .collect();
    let days = (0..7)
        .map(|d| {
            let day = &hours[d * 24..d * 24 + 24];
            let (lo, hi) = day.iter().fold((f32::MAX, f32::MIN), |(lo, hi), h| (lo.min(h.temp), hi.max(h.temp)));
            let rain: f32 = day.iter().map(|h| h.amount).sum();
            Day {
                date: (date + Duration::days(d as i64)).format("%Y-%m-%d").to_string(),
                high: hi,
                low: lo,
                rain,
                code: if rain > 2. { 63 } else if rain > 0. { 61 } else { 2 },
                sunrise: format!("{}T07:31", (date + Duration::days(d as i64)).format("%Y-%m-%d")),
                sunset: format!("{}T18:52", (date + Duration::days(d as i64)).format("%Y-%m-%d")),
            }
        })
        .collect();
    let now = &hours[hour as usize];
    Forecast {
        place: "Demo".into(),
        units,
        time: format!("{}T{hour:02}:00", date.format("%Y-%m-%d")),
        temp: now.temp,
        feels: now.temp - 1.5,
        humidity: 64.,
        pressure: 1012.,
        wind: if units == Units::Imperial { 9. } else { 14. },
        wind_dir: 290.,
        code: 2,
        cloud: 45.,
        hours,
        days,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "latitude": 46.88, "longitude": -96.79, "utc_offset_seconds": -18000,
        "current": {"time": "2026-10-07T14:15", "temperature_2m": 12.4, "relative_humidity_2m": 58,
                    "apparent_temperature": 10.9, "weather_code": 3, "cloud_cover": 88, "pressure_msl": 1016.2,
                    "wind_speed_10m": 18.7, "wind_direction_10m": 300},
        "hourly": {"time": ["2026-10-07T00:00", "2026-10-07T01:00", "2026-10-07T14:00", "2026-10-07T15:00"],
                   "temperature_2m": [6.1, 5.8, 12.4, 12.0],
                   "precipitation_probability": [0, 5, 40, null],
                   "precipitation": [0.0, 0.0, 0.3, 1.2]},
        "daily": {"time": ["2026-10-07"], "temperature_2m_max": [13.0], "temperature_2m_min": [4.9],
                  "precipitation_sum": [1.5], "weather_code": [61],
                  "sunrise": ["2026-10-07T07:38"], "sunset": ["2026-10-07T19:01"]}
    }"#;

    #[test]
    fn parses_a_forecast() {
        let f = parse(SAMPLE, "Fargo, US", Units::Metric).unwrap();
        assert_eq!(f.place, "Fargo, US");
        assert_eq!(f.temp, 12.4);
        assert_eq!(f.humidity, 58.);
        assert_eq!(f.code, 3);
        assert_eq!(f.cloud, 88.);
        assert_eq!(f.description(), "overcast");
        assert_eq!(f.hours.len(), 4);
        assert_eq!(f.hours[3].chance, 0., "a null reads as zero");
        assert_eq!(f.days[0].sunset, "2026-10-07T19:01");
        assert_eq!(f.now_index(), 2, "14:15 falls in the 14:00 hour");
        assert_eq!(f.next(10).len(), 2);
        let d = f.daylight().unwrap();
        assert!(d > 0.5 && d < 0.7, "14:15 is past the middle of 07:38–19:01: {d}");
        assert_eq!(clock(&f.days[0].sunrise), "07:38");
    }

    #[test]
    fn reports_api_errors() {
        let err = parse(r#"{"error": true, "reason": "Latitude must be in range"}"#, "x", Units::Metric).unwrap_err();
        assert!(err.contains("Latitude"));
        assert!(parse("<html>", "x", Units::Metric).is_err());
        assert!(parse("{}", "x", Units::Metric).is_err());
    }

    #[test]
    fn parses_places() {
        let body = r#"{"results": [{"name": "Fargo", "latitude": 46.877, "longitude": -96.789, "country_code": "US"}]}"#;
        assert_eq!(parse_place(body, "Fargo").unwrap(), Place { name: "Fargo, US".into(), lat: 46.877, lon: -96.789 });
        assert!(parse_place(r#"{"generationtime_ms": 0.5}"#, "Nowhere").unwrap_err().contains("Nowhere"));
    }

    #[test]
    fn locations() {
        assert_eq!(Location::parse(" 46.88, -96.79 "), Some(Location::Coords(46.88, -96.79)));
        assert_eq!(Location::parse("Fargo, US"), Some(Location::Place("Fargo, US".into())));
        assert_eq!(Location::parse("95,10"), None, "latitude out of range");
        assert_eq!(Location::parse("  "), None);
    }

    #[test]
    fn compass_points() {
        assert_eq!(compass(0.), "N");
        assert_eq!(compass(359.), "N");
        assert_eq!(compass(90.), "E");
        assert_eq!(compass(300.), "NW");
        assert_eq!(compass(-90.), "W");
        assert_eq!(format!("{}", whole(-0.3)), "0");
    }

    #[test]
    fn week_grid_is_seven_by_twenty_four() {
        let f = demo(chrono::NaiveDate::from_ymd_opt(2026, 10, 7).unwrap(), 14, Units::Metric);
        let g = f.week_grid();
        assert_eq!(g.len(), 7);
        assert!(g.iter().all(|r| r.len() == 24));
        assert!(g.iter().flatten().all(|v| (0.0..=1.0).contains(v)));
        assert!(g.iter().flatten().any(|v| *v == 1.), "the week's warmest hour is full");
        assert_eq!(f.days.len(), 7);
        assert_eq!(f.now_index(), 14);
    }
}
