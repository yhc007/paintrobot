//! Job insert/lookup/scan against paintrobot.jobs.

use crate::{
    check_identifier, decode_f64_opt, decode_i64, decode_text, decode_text_opt, fmt_double,
    quote_text, CoreDbClient, HttpTransport, RepoError,
};
use paintrobot_domain::AggRow;
use paintrobot_schema::{JobIn, MatchStatus};

#[derive(Debug, Clone)]
pub struct JobRow {
    pub event_id: String,
    pub edge_id: String,
    pub plc_model_no: Option<String>,
    pub camera_model_no: Option<String>,
    pub plc_ts: Option<i64>,
    pub camera_ts: Option<i64>,
    pub confidence: Option<f64>,
    pub match_status: String,
    pub work_date: String,
    pub image_ref: Option<String>,
    pub created_at: i64,
}

impl<T: HttpTransport> CoreDbClient<T> {
    /// Insert a completed job record.
    /// Callers are expected to pre-validate `work_date` (YYYY-MM-DD).
    pub async fn insert_job(
        &self,
        job: &JobIn,
        work_date: &str,
        match_status: MatchStatus,
        created_at_ms: i64,
    ) -> Result<(), RepoError> {
        check_identifier(&job.event_id)?;
        check_identifier(&job.edge_id)?;
        if let Some(m) = &job.plc_model_no {
            check_identifier(m)?;
        }
        if let Some(m) = &job.camera_model_no {
            check_identifier(m)?;
        }
        // work_date: YYYY-MM-DD
        check_identifier(work_date)?;

        let plc_ts = job.plc_ts.map(|t| t.timestamp_millis()).unwrap_or(0);
        let camera_ts = job.camera_ts.map(|t| t.timestamp_millis()).unwrap_or(0);
        let confidence = job.confidence.unwrap_or(0.0);
        let image_ref = job.image_ref.as_deref().unwrap_or("");

        let cql = format!(
            "INSERT INTO {ks}.jobs \
             (event_id, edge_id, plc_model_no, camera_model_no, plc_ts, camera_ts, \
              confidence, match_status, work_date, image_ref, created_at) \
             VALUES ({event_id}, {edge_id}, {plc}, {cam}, {pts}, {cts}, \
                     {conf}, {status}, {wd}, {img}, {ca})",
            ks = self.keyspace,
            event_id = quote_text(&job.event_id),
            edge_id = quote_text(&job.edge_id),
            plc = quote_text(job.plc_model_no.as_deref().unwrap_or("")),
            cam = quote_text(job.camera_model_no.as_deref().unwrap_or("")),
            pts = plc_ts,
            cts = camera_ts,
            conf = fmt_double(confidence),
            status = quote_text(match_status.as_str()),
            wd = quote_text(work_date),
            img = quote_text(image_ref),
            ca = created_at_ms,
        );
        self.execute(&cql).await?;
        Ok(())
    }

    /// Returns Some(row) if the event_id exists, None otherwise.
    pub async fn get_job(&self, event_id: &str) -> Result<Option<JobRow>, RepoError> {
        check_identifier(event_id)?;
        let cql = format!(
            "SELECT event_id, edge_id, plc_model_no, camera_model_no, plc_ts, camera_ts, \
             confidence, match_status, work_date, image_ref, created_at \
             FROM {ks}.jobs WHERE event_id={id}",
            ks = self.keyspace,
            id = quote_text(event_id),
        );
        let rows = self.execute(&cql).await?;
        match rows.first() {
            Some(row) => Ok(Some(decode_job_row(row)?)),
            None => Ok(None),
        }
    }

    /// Scan all jobs for a given work_date. Uses a full scan — CoreDB has no
    /// secondary index and no composite key support.
    pub async fn scan_jobs_for_date(
        &self,
        work_date: &str,
        limit: u32,
    ) -> Result<Vec<JobRow>, RepoError> {
        check_identifier(work_date)?;
        let cql = format!(
            "SELECT event_id, edge_id, plc_model_no, camera_model_no, plc_ts, camera_ts, \
             confidence, match_status, work_date, image_ref, created_at \
             FROM {ks}.jobs WHERE work_date={wd} LIMIT {n}",
            ks = self.keyspace,
            wd = quote_text(work_date),
            n = limit,
        );
        let rows = self.execute(&cql).await?;
        rows.iter().map(decode_job_row).collect()
    }

    /// Same scan, but decoded into the slim AggRow that `paintrobot-domain::aggregate` consumes.
    pub async fn agg_rows_for_date(
        &self,
        work_date: &str,
        limit: u32,
    ) -> Result<Vec<AggRow>, RepoError> {
        check_identifier(work_date)?;
        let cql = format!(
            "SELECT plc_model_no, camera_model_no, match_status FROM {ks}.jobs \
             WHERE work_date={wd} LIMIT {n}",
            ks = self.keyspace,
            wd = quote_text(work_date),
            n = limit,
        );
        let rows = self.execute(&cql).await?;
        rows.iter()
            .map(|row| {
                let cols = &row.columns;
                let plc = decode_text_opt(
                    cols.get("plc_model_no")
                        .ok_or_else(|| RepoError::Decode("plc_model_no missing".into()))?,
                )?;
                let cam = decode_text_opt(
                    cols.get("camera_model_no")
                        .ok_or_else(|| RepoError::Decode("camera_model_no missing".into()))?,
                )?;
                let status = decode_text(
                    cols.get("match_status")
                        .ok_or_else(|| RepoError::Decode("match_status missing".into()))?,
                )?;
                let model_no = plc
                    .filter(|s| !s.is_empty())
                    .or(cam.filter(|s| !s.is_empty()))
                    .unwrap_or_else(|| "(unknown)".to_string());
                Ok(AggRow {
                    model_no,
                    match_status: status,
                })
            })
            .collect()
    }
}

fn decode_job_row(row: &crate::RawRow) -> Result<JobRow, RepoError> {
    let cols = &row.columns;
    let get = |name: &str| {
        cols.get(name)
            .ok_or_else(|| RepoError::Decode(format!("column {name} missing")))
    };
    Ok(JobRow {
        event_id: decode_text(get("event_id")?)?,
        edge_id: decode_text(get("edge_id")?)?,
        plc_model_no: decode_text_opt(get("plc_model_no")?)?.filter(|s| !s.is_empty()),
        camera_model_no: decode_text_opt(get("camera_model_no")?)?.filter(|s| !s.is_empty()),
        plc_ts: Some(decode_i64(get("plc_ts")?)?).filter(|&n| n != 0),
        camera_ts: Some(decode_i64(get("camera_ts")?)?).filter(|&n| n != 0),
        confidence: decode_f64_opt(get("confidence")?).ok().flatten(),
        match_status: decode_text(get("match_status")?)?,
        work_date: decode_text(get("work_date")?)?,
        image_ref: decode_text_opt(get("image_ref")?)?.filter(|s| !s.is_empty()),
        created_at: decode_i64(get("created_at")?)?,
    })
}
