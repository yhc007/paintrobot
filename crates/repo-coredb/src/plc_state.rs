//! 엣지별 현재 PLC/카메라 상태 — paintrobot.plc_state.
//!
//! 왜 따로 두는가: 엣지는 폴링할 때마다 PLC 모델을 보낸다. 그걸 전부 `jobs`에
//! 넣으면 상태가 바뀌지 않아도 하루 7천여 행이 쌓인다(실측 1,108:1 중복).
//! 여기는 엣지당 한 행만 두고 계속 덮어쓴다 — CoreDB는 같은 PK INSERT가
//! upsert라 행이 늘지 않는다. `jobs`에는 실제 전환만 남긴다.
//!
//! `/api/v1/plc/current`도 이 테이블을 읽는다. 예전에는 오늘치 전 행을 훑어
//! 최신값을 골랐다.

use crate::{
    check_identifier, decode_i64, decode_text_opt, quote_text, CoreDbClient, HttpTransport,
    RepoError,
};

#[derive(Debug, Clone)]
pub struct PlcStateRow {
    pub edge_id: String,
    pub model_no: Option<String>,
    pub plc_ts: Option<i64>,
    pub camera_model_no: Option<String>,
    pub camera_ts: Option<i64>,
    pub updated_at: i64,
}

fn decode_state(row: &crate::RawRow) -> Result<PlcStateRow, RepoError> {
    let cols = &row.columns;
    let get = |name: &str| {
        cols.get(name)
            .ok_or_else(|| RepoError::Decode(format!("column {name} missing")))
    };
    let ts = |name: &str| -> Result<Option<i64>, RepoError> {
        match cols.get(name) {
            Some(v) => Ok(Some(decode_i64(v)?).filter(|t| *t > 0)),
            None => Ok(None),
        }
    };
    Ok(PlcStateRow {
        edge_id: decode_text_opt(get("edge_id")?)?.unwrap_or_default(),
        model_no: decode_text_opt(get("model_no")?)?.filter(|s| !s.is_empty()),
        plc_ts: ts("plc_ts")?,
        camera_model_no: decode_text_opt(get("camera_model_no")?)?.filter(|s| !s.is_empty()),
        camera_ts: ts("camera_ts")?,
        updated_at: cols.get("updated_at").map(decode_i64).transpose()?.unwrap_or(0),
    })
}

const COLS: &str = "edge_id, model_no, plc_ts, camera_model_no, camera_ts, updated_at";

impl<T: HttpTransport> CoreDbClient<T> {
    pub async fn get_plc_state(&self, edge_id: &str) -> Result<Option<PlcStateRow>, RepoError> {
        check_identifier(edge_id)?;
        let cql = format!(
            "SELECT {COLS} FROM {ks}.plc_state WHERE edge_id={e}",
            ks = self.keyspace,
            e = quote_text(edge_id),
        );
        let rows = self.execute(&cql).await?;
        rows.first().map(decode_state).transpose()
    }

    /// 엣지 전체의 현재 상태. 엣지 수만큼만 있으므로 스캔이 싸다.
    pub async fn scan_plc_states(&self, limit: u32) -> Result<Vec<PlcStateRow>, RepoError> {
        let cql = format!(
            "SELECT {COLS} FROM {ks}.plc_state LIMIT {n}",
            ks = self.keyspace,
            n = limit,
        );
        let rows = self.execute(&cql).await?;
        rows.iter().map(decode_state).collect()
    }

    /// 같은 PK INSERT = upsert. 행이 늘지 않는다.
    pub async fn upsert_plc_state(&self, s: &PlcStateRow) -> Result<(), RepoError> {
        check_identifier(&s.edge_id)?;
        if let Some(m) = &s.model_no {
            check_identifier(m)?;
        }
        if let Some(m) = &s.camera_model_no {
            check_identifier(m)?;
        }
        let cql = format!(
            "INSERT INTO {ks}.plc_state ({COLS}) VALUES ({e}, {m}, {pts}, {cm}, {cts}, {up})",
            ks = self.keyspace,
            e = quote_text(&s.edge_id),
            m = quote_text(s.model_no.as_deref().unwrap_or("")),
            pts = s.plc_ts.unwrap_or(0),
            cm = quote_text(s.camera_model_no.as_deref().unwrap_or("")),
            cts = s.camera_ts.unwrap_or(0),
            up = s.updated_at,
        );
        self.execute(&cql).await?;
        Ok(())
    }
}
