//! Coating measurements: insert/lookup/scan against paintrobot.coatings.

use crate::{
    check_identifier, decode_f64, decode_i64, decode_text, decode_text_opt, fmt_double,
    quote_text, CoreDbClient, HttpTransport, RepoError,
};

#[derive(Debug, Clone)]
pub struct CoatingRow {
    pub event_id: String,
    pub job_event_id: Option<String>,
    pub model_no: String,
    pub measured_um: f64,
    pub target_um: f64,
    pub temperature_c: f64,
    pub humidity_pct: f64,
    pub current_pressure: f64,
    pub recommended_pressure: f64,
    pub thickness_error: f64,
    pub control_factor: f64,
    pub temp_factor: f64,
    pub humidity_factor: f64,
    pub measured_at: i64,
    pub work_date: String,
}

impl<T: HttpTransport> CoreDbClient<T> {
    pub async fn insert_coating(&self, row: &CoatingRow) -> Result<(), RepoError> {
        check_identifier(&row.event_id)?;
        check_identifier(&row.model_no)?;
        check_identifier(&row.work_date)?;
        if let Some(j) = &row.job_event_id {
            check_identifier(j)?;
        }
        let cql = format!(
            "INSERT INTO {ks}.coatings \
             (event_id, job_event_id, model_no, measured_um, target_um, temperature_c, \
              humidity_pct, current_pressure, recommended_pressure, thickness_error, \
              control_factor, temp_factor, humidity_factor, measured_at, work_date) \
             VALUES ({eid}, {jid}, {mno}, {mu}, {tu}, {t}, {h}, {cp}, {rp}, {err}, {cf}, {tf}, {hf}, {ts}, {wd})",
            ks = self.keyspace,
            eid = quote_text(&row.event_id),
            jid = quote_text(row.job_event_id.as_deref().unwrap_or("")),
            mno = quote_text(&row.model_no),
            mu = fmt_double(row.measured_um),
            tu = fmt_double(row.target_um),
            t = fmt_double(row.temperature_c),
            h = fmt_double(row.humidity_pct),
            cp = fmt_double(row.current_pressure),
            rp = fmt_double(row.recommended_pressure),
            err = fmt_double(row.thickness_error),
            cf = fmt_double(row.control_factor),
            tf = fmt_double(row.temp_factor),
            hf = fmt_double(row.humidity_factor),
            ts = row.measured_at,
            wd = quote_text(&row.work_date),
        );
        self.execute(&cql).await?;
        Ok(())
    }

    pub async fn get_coating(&self, event_id: &str) -> Result<Option<CoatingRow>, RepoError> {
        check_identifier(event_id)?;
        let cql = format!(
            "SELECT event_id, job_event_id, model_no, measured_um, target_um, temperature_c, \
             humidity_pct, current_pressure, recommended_pressure, thickness_error, \
             control_factor, temp_factor, humidity_factor, measured_at, work_date \
             FROM {ks}.coatings WHERE event_id={id}",
            ks = self.keyspace,
            id = quote_text(event_id),
        );
        let rows = self.execute(&cql).await?;
        match rows.first() {
            Some(row) => Ok(Some(decode_coating(row)?)),
            None => Ok(None),
        }
    }

    pub async fn scan_coatings_for_date(
        &self,
        work_date: &str,
        limit: u32,
    ) -> Result<Vec<CoatingRow>, RepoError> {
        check_identifier(work_date)?;
        let cql = format!(
            "SELECT event_id, job_event_id, model_no, measured_um, target_um, temperature_c, \
             humidity_pct, current_pressure, recommended_pressure, thickness_error, \
             control_factor, temp_factor, humidity_factor, measured_at, work_date \
             FROM {ks}.coatings WHERE work_date={wd} LIMIT {n}",
            ks = self.keyspace,
            wd = quote_text(work_date),
            n = limit,
        );
        let rows = self.execute(&cql).await?;
        rows.iter().map(decode_coating).collect()
    }
}

fn decode_coating(row: &crate::RawRow) -> Result<CoatingRow, RepoError> {
    let cols = &row.columns;
    let get = |name: &str| {
        cols.get(name)
            .ok_or_else(|| RepoError::Decode(format!("column {name} missing")))
    };
    Ok(CoatingRow {
        event_id: decode_text(get("event_id")?)?,
        job_event_id: decode_text_opt(get("job_event_id")?)?.filter(|s| !s.is_empty()),
        model_no: decode_text(get("model_no")?)?,
        measured_um: decode_f64(get("measured_um")?)?,
        target_um: decode_f64(get("target_um")?)?,
        temperature_c: decode_f64(get("temperature_c")?)?,
        humidity_pct: decode_f64(get("humidity_pct")?)?,
        current_pressure: decode_f64(get("current_pressure")?)?,
        recommended_pressure: decode_f64(get("recommended_pressure")?)?,
        thickness_error: decode_f64(get("thickness_error")?)?,
        control_factor: decode_f64(get("control_factor")?)?,
        temp_factor: decode_f64(get("temp_factor")?)?,
        humidity_factor: decode_f64(get("humidity_factor")?)?,
        measured_at: decode_i64(get("measured_at")?)?,
        work_date: decode_text(get("work_date")?)?,
    })
}
