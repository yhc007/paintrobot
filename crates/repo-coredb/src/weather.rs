//! Weather snapshot insert/lookup against paintrobot.weather_snapshots.

use crate::{
    check_identifier, decode_f64, decode_text, fmt_double, quote_text, CoreDbClient, HttpTransport,
    RepoError,
};

#[derive(Debug, Clone)]
pub struct WeatherRow {
    pub observed_at: String, // ISO8601
    pub temperature_c: f64,
    pub humidity_pct: f64,
    pub source: String,
}

impl<T: HttpTransport> CoreDbClient<T> {
    pub async fn insert_weather(&self, w: &WeatherRow) -> Result<(), RepoError> {
        check_identifier(&w.observed_at)?;
        check_identifier(&w.source)?;
        let cql = format!(
            "INSERT INTO {ks}.weather_snapshots \
             (observed_at, temperature_c, humidity_pct, source) \
             VALUES ({at}, {t}, {h}, {src})",
            ks = self.keyspace,
            at = quote_text(&w.observed_at),
            t = fmt_double(w.temperature_c),
            h = fmt_double(w.humidity_pct),
            src = quote_text(&w.source),
        );
        self.execute(&cql).await?;
        Ok(())
    }

    pub async fn get_weather(&self, observed_at: &str) -> Result<Option<WeatherRow>, RepoError> {
        check_identifier(observed_at)?;
        let cql = format!(
            "SELECT observed_at, temperature_c, humidity_pct, source \
             FROM {ks}.weather_snapshots WHERE observed_at={at}",
            ks = self.keyspace,
            at = quote_text(observed_at),
        );
        let rows = self.execute(&cql).await?;
        match rows.first() {
            Some(row) => {
                let cols = &row.columns;
                let get = |name: &str| {
                    cols.get(name)
                        .ok_or_else(|| RepoError::Decode(format!("column {name} missing")))
                };
                Ok(Some(WeatherRow {
                    observed_at: decode_text(get("observed_at")?)?,
                    temperature_c: decode_f64(get("temperature_c")?)?,
                    humidity_pct: decode_f64(get("humidity_pct")?)?,
                    source: decode_text(get("source")?)?,
                }))
            }
            None => Ok(None),
        }
    }
}
