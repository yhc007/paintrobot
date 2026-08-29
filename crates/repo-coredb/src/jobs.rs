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

    /// 이미 저장된 행을 새 판정으로 덮어쓴다 (지연 상관 배치용).
    ///
    /// CoreDB는 UPDATE를 지원하지 않는다. 같은 PRIMARY KEY로 INSERT하면 LSM
    /// 셀 타임스탬프가 갱신되어 upsert가 되므로, 읽어온 행을 그대로 되쓰면서
    /// `plc_model_no`와 `match_status`만 바꾼다. 나머지 필드는 손대지 않는다.
    pub async fn rewrite_job_match(
        &self,
        row: &JobRow,
        plc_model_no: &str,
        match_status: MatchStatus,
    ) -> Result<(), RepoError> {
        check_identifier(&row.event_id)?;
        check_identifier(&row.edge_id)?;
        check_identifier(plc_model_no)?;
        check_identifier(&row.work_date)?;
        if let Some(m) = &row.camera_model_no {
            check_identifier(m)?;
        }

        let cql = format!(
            "INSERT INTO {ks}.jobs \
             (event_id, edge_id, plc_model_no, camera_model_no, plc_ts, camera_ts, \
              confidence, match_status, work_date, image_ref, created_at) \
             VALUES ({event_id}, {edge_id}, {plc}, {cam}, {pts}, {cts}, \
                     {conf}, {status}, {wd}, {img}, {ca})",
            ks = self.keyspace,
            event_id = quote_text(&row.event_id),
            edge_id = quote_text(&row.edge_id),
            plc = quote_text(plc_model_no),
            cam = quote_text(row.camera_model_no.as_deref().unwrap_or("")),
            pts = row.plc_ts.unwrap_or(0),
            cts = row.camera_ts.unwrap_or(0),
            conf = fmt_double(row.confidence.unwrap_or(0.0)),
            status = quote_text(match_status.as_str()),
            wd = quote_text(&row.work_date),
            img = quote_text(row.image_ref.as_deref().unwrap_or("")),
            ca = row.created_at,
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

    /// Aggregate rows for a whole date range in a single query.
    ///
    /// CoreDB scans the full `jobs` table for every statement regardless of the
    /// `work_date` predicate, so one ranged read costs the same as one single-day
    /// read — issuing N per-day queries multiplied that scan by N. Callers bucket
    /// the returned rows by their `work_date`.
    pub async fn agg_rows_for_range(
        &self,
        from: &str,
        to: &str,
        limit: u32,
    ) -> Result<Vec<(String, AggRow)>, RepoError> {
        check_identifier(from)?;
        check_identifier(to)?;
        let cql = format!(
            "SELECT work_date, plc_model_no, camera_model_no, match_status FROM {ks}.jobs \
             WHERE work_date>={f} AND work_date<={t} LIMIT {n}",
            ks = self.keyspace,
            f = quote_text(from),
            t = quote_text(to),
            n = limit,
        );
        let rows = self.execute(&cql).await?;
        rows.iter()
            .map(|row| {
                let cols = &row.columns;
                let work_date = decode_text(
                    cols.get("work_date")
                        .ok_or_else(|| RepoError::Decode("work_date missing".into()))?,
                )?;
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
                Ok((
                    work_date,
                    AggRow {
                        model_no,
                        match_status: status,
                    },
                ))
            })
            .collect()
    }

    /// Earliest and latest `work_date` that carry at least one counted job.
    ///
    /// `plc_only` rows are skipped here for the same reason `domain::aggregate`
    /// skips them — a day of pure PLC chatter shows 0 on the dashboard, so it
    /// must not be offered as a day that "has data".
    pub async fn job_date_bounds(&self, limit: u32) -> Result<Option<(String, String)>, RepoError> {
        let cql = format!(
            "SELECT work_date, match_status FROM {ks}.jobs LIMIT {n}",
            ks = self.keyspace,
            n = limit,
        );
        let rows = self.execute(&cql).await?;
        let mut lo: Option<String> = None;
        let mut hi: Option<String> = None;
        for row in &rows {
            let cols = &row.columns;
            let status = decode_text(
                cols.get("match_status")
                    .ok_or_else(|| RepoError::Decode("match_status missing".into()))?,
            )?;
            if status == "plc_only" {
                continue;
            }
            let d = decode_text(
                cols.get("work_date")
                    .ok_or_else(|| RepoError::Decode("work_date missing".into()))?,
            )?;
            if lo.as_ref().is_none_or(|c| d < *c) {
                lo = Some(d.clone());
            }
            if hi.as_ref().is_none_or(|c| d > *c) {
                hi = Some(d);
            }
        }
        Ok(lo.zip(hi))
    }

    /// Same scan as `scan_jobs_for_date`, but decoded into the slim AggRow that
    /// `paintrobot-domain::aggregate` consumes.
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
