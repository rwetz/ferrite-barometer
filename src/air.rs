//! Open-Meteo/CAMS readings. Missing pollen never becomes a fake zero.
use crate::forecast::Place;
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct Reading {
    pub time: String,
    pub aqi: Option<f32>,
    pub pm25: Option<f32>,
    pub pm10: Option<f32>,
    pub pollen: Vec<(&'static str, Option<f32>)>,
}
fn value(v: &Value) -> Option<f32> {
    v.as_f64()
        .filter(|n| n.is_finite() && *n >= 0.)
        .map(|n| n as f32)
}
pub fn parse(json: &str) -> Result<Reading, String> {
    let v: Value = serde_json::from_str(json).map_err(|_| "Invalid air quality response")?;
    let current = &v["current"];
    let time = current["time"]
        .as_str()
        .ok_or("Missing air quality reading time")?;
    let hourly = &v["hourly"];
    let index = hourly["time"]
        .as_array()
        .and_then(|times| times.iter().position(|t| t.as_str() == Some(time)));
    let pollen = [
        ("Grass", "grass_pollen"),
        ("Birch", "birch_pollen"),
        ("Alder", "alder_pollen"),
        ("Ragweed", "ragweed_pollen"),
    ]
    .into_iter()
    .map(|(label, key)| {
        (
            label,
            index.and_then(|i| hourly[key].get(i)).and_then(value),
        )
    })
    .collect();
    Ok(Reading {
        time: time.into(),
        aqi: value(&current["us_aqi"]),
        pm25: value(&current["pm2_5"]),
        pm10: value(&current["pm10"]),
        pollen,
    })
}
pub fn fetch(place: &Place) -> Result<Reading, String> {
    let mut response = ureq::get("https://air-quality-api.open-meteo.com/v1/air-quality")
        .query("latitude", place.lat.to_string())
        .query("longitude", place.lon.to_string())
        .query("current", "us_aqi,pm2_5,pm10")
        .query(
            "hourly",
            "grass_pollen,birch_pollen,alder_pollen,ragweed_pollen",
        )
        .query("timezone", "auto")
        .query("forecast_days", "1")
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(20)))
        .build()
        .call()
        .map_err(|e| format!("Air quality unavailable: {e}"))?;
    parse(
        &response
            .body_mut()
            .with_config()
            .limit(256 * 1024)
            .read_to_string()
            .map_err(|_| "Couldn't read air quality")?,
    )
}
pub fn number(n: Option<f32>) -> String {
    n.map(|n| format!("{n:.1}"))
        .unwrap_or_else(|| "Unavailable".into())
}
pub fn category(aqi: Option<f32>) -> &'static str {
    match aqi {
        None => "Unavailable",
        Some(n) if n <= 50. => "Good",
        Some(n) if n <= 100. => "Moderate",
        Some(n) if n <= 150. => "Unhealthy for sensitive groups",
        Some(n) if n <= 200. => "Unhealthy",
        Some(n) if n <= 300. => "Very unhealthy",
        _ => "Hazardous",
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_pollen_is_not_zero_and_hours_match_current() {
        let reading = parse(r#"{"current":{"time":"2026-10-10T01:00","us_aqi":0,"pm2_5":1.5},"hourly":{"time":["2026-10-10T00:00","2026-10-10T01:00"],"grass_pollen":[10,null],"birch_pollen":[2,4]}}"#).unwrap();
        assert_eq!(reading.aqi, Some(0.));
        assert_eq!(reading.pollen[0].1, None);
        assert_eq!(reading.pollen[1].1, Some(4.));
        assert!(parse("{}").is_err());
    }
}
