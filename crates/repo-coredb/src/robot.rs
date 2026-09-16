//! R1 로봇 인터페이스 — paintrobot.plc_robot_state / plc_robot_day.
//!
//! 스펙(`plc_r.md` §4.1)은 스냅샷을 시계열로 append 하고 30일 롤오프를
//! 권한다. **CoreDB에는 DELETE가 없어 롤오프를 할 수 없고**, 권장 폴링
//! 주기(1~2초)면 하루 43,000~86,000행이 영구히 쌓인다. 이 저장소가 이미
//! 똑같이 당했다 — PLC 모델 폴링을 전부 `jobs`에 넣어 하루 7,758행이
//! 쌓였고 `/plc/current`가 94초까지 갔다.
//!
//! 그래서 저장 모양을 바꿨다:
//!
//! * `plc_robot_state` — **엣지당 한 행, 계속 덮어쓴다.** 화면이 읽는
//!   "지금 상태"다. 같은 PK INSERT가 upsert라 행이 늘지 않는다.
//! * `plc_robot_day` — **날짜당 한 행.** 파생 이벤트(§4.3)만 JSON 배열로
//!   append 한다. 이벤트는 희소해서 하루 수백 건이고, 단일 키 조회라 빠르다.
//!   `daily_rollup`이 이미 쓰는 방식과 같다.
//!
//! 잃는 것: 스냅샷 원본 이력. 남는 것은 현재 스냅샷 1건과 전이 이벤트다.
//! 사후에 "그때 그 순간 비트가 어땠나"를 되돌려 볼 수는 없다.

use crate::{
    check_identifier, decode_i64, decode_text, decode_text_opt, quote_text, CoreDbClient,
    HttpTransport, RepoError,
};

/// 하루치 이벤트 배열에 남기는 최대 건수. 넘치면 오래된 것부터 버린다.
/// 한 행이 CQL 문자열 하나로 나가므로 무한정 키울 수 없다.
pub const MAX_DAY_EVENTS: usize = 300;

#[derive(Debug, Clone)]
pub struct RobotStateRow {
    pub edge_id: String,
    pub robot_model: Option<String>,
    pub model_no: Option<i64>,
    /// 마지막 스냅샷 원본 JSON (`RobotIn` 그대로).
    pub snapshot_json: String,
    /// `read_errors` 배열의 JSON.
    pub read_errors: String,
    /// 읽기 실패가 섞였는가. 0/1.
    pub degraded: i64,
    /// `command` ↔ `do` 연속 불일치 횟수 (§4.4).
    pub io_streak: i64,
    pub received_at: i64,
}

const STATE_COLS: &str =
    "edge_id, robot_model, model_no, snapshot_json, read_errors, degraded, io_streak, received_at";

fn decode_state(row: &crate::RawRow) -> Result<RobotStateRow, RepoError> {
    let cols = &row.columns;
    let get = |name: &str| {
        cols.get(name)
            .ok_or_else(|| RepoError::Decode(format!("column {name} missing")))
    };
    let num = |name: &str| -> Result<i64, RepoError> {
        Ok(cols.get(name).map(decode_i64).transpose()?.unwrap_or(0))
    };
    Ok(RobotStateRow {
        edge_id: decode_text_opt(get("edge_id")?)?.unwrap_or_default(),
        robot_model: decode_text_opt(get("robot_model")?)?.filter(|s| !s.is_empty()),
        // model_no는 "없음"과 "0"이 다르다. 0은 HMI 미입력이라는 뜻이라
        // 저장할 때 없음을 i64::MIN으로 피신시켜 둔다.
        model_no: Some(num("model_no")?).filter(|v| *v != i64::MIN),
        snapshot_json: decode_text_opt(get("snapshot_json")?)?.unwrap_or_default(),
        read_errors: decode_text_opt(get("read_errors")?)?.unwrap_or_else(|| "[]".into()),
        degraded: num("degraded")?,
        io_streak: num("io_streak")?,
        received_at: num("received_at")?,
    })
}

#[derive(Debug, Clone)]
pub struct RobotDayRow {
    pub work_date: String,
    /// `{"events":[...],"counts":{...}}`
    pub payload: String,
    pub updated_at: i64,
}

fn decode_day(row: &crate::RawRow) -> Result<RobotDayRow, RepoError> {
    let cols = &row.columns;
    let get = |name: &str| {
        cols.get(name)
            .ok_or_else(|| RepoError::Decode(format!("column {name} missing")))
    };
    Ok(RobotDayRow {
        work_date: decode_text(get("work_date")?)?,
        payload: decode_text(get("payload")?)?,
        updated_at: cols
            .get("updated_at")
            .map(decode_i64)
            .transpose()?
            .unwrap_or(0),
    })
}

impl<T: HttpTransport> CoreDbClient<T> {
    pub async fn get_robot_state(
        &self,
        edge_id: &str,
    ) -> Result<Option<RobotStateRow>, RepoError> {
        check_identifier(edge_id)?;
        let cql = format!(
            "SELECT {STATE_COLS} FROM {ks}.plc_robot_state WHERE edge_id={e}",
            ks = self.keyspace,
            e = quote_text(edge_id),
        );
        let rows = self.execute(&cql).await?;
        rows.first().map(decode_state).transpose()
    }

    /// 엣지 수만큼만 있으므로 스캔이 싸다.
    pub async fn scan_robot_states(&self, limit: u32) -> Result<Vec<RobotStateRow>, RepoError> {
        let cql = format!(
            "SELECT {STATE_COLS} FROM {ks}.plc_robot_state LIMIT {n}",
            ks = self.keyspace,
            n = limit,
        );
        let rows = self.execute(&cql).await?;
        rows.iter().map(decode_state).collect()
    }

    /// 같은 PK INSERT = upsert. 폴링해도 행이 늘지 않는다.
    ///
    /// `snapshot_json`/`read_errors`에는 `check_identifier`를 걸지 않는다 —
    /// JSON이라 통과할 수 없다. `quote_text`가 작은따옴표를 이스케이프해
    /// CQL 문자열을 벗어나지 못하게 한다 (`daily_rollup.payload`와 같은 처리).
    pub async fn upsert_robot_state(&self, s: &RobotStateRow) -> Result<(), RepoError> {
        check_identifier(&s.edge_id)?;
        if let Some(m) = &s.robot_model {
            check_identifier(m)?;
        }
        let cql = format!(
            "INSERT INTO {ks}.plc_robot_state ({STATE_COLS}) \
             VALUES ({e}, {rm}, {mn}, {sj}, {re}, {dg}, {st}, {ts})",
            ks = self.keyspace,
            e = quote_text(&s.edge_id),
            rm = quote_text(s.robot_model.as_deref().unwrap_or("")),
            mn = s.model_no.unwrap_or(i64::MIN),
            sj = quote_text(&s.snapshot_json),
            re = quote_text(&s.read_errors),
            dg = s.degraded,
            st = s.io_streak,
            ts = s.received_at,
        );
        self.execute(&cql).await?;
        Ok(())
    }

    pub async fn get_robot_day(&self, work_date: &str) -> Result<Option<RobotDayRow>, RepoError> {
        check_identifier(work_date)?;
        let cql = format!(
            "SELECT work_date, payload, updated_at FROM {ks}.plc_robot_day WHERE work_date={d}",
            ks = self.keyspace,
            d = quote_text(work_date),
        );
        let rows = self.execute(&cql).await?;
        rows.first().map(decode_day).transpose()
    }

    /// 날짜당 한 행이라 구간 스캔도 싸다.
    pub async fn scan_robot_days(
        &self,
        from: &str,
        to: &str,
        limit: u32,
    ) -> Result<Vec<RobotDayRow>, RepoError> {
        check_identifier(from)?;
        check_identifier(to)?;
        let cql = format!(
            "SELECT work_date, payload, updated_at FROM {ks}.plc_robot_day \
             WHERE work_date>={f} AND work_date<={t} LIMIT {n}",
            ks = self.keyspace,
            f = quote_text(from),
            t = quote_text(to),
            n = limit,
        );
        let rows = self.execute(&cql).await?;
        rows.iter().map(decode_day).collect()
    }

    pub async fn upsert_robot_day(&self, r: &RobotDayRow) -> Result<(), RepoError> {
        check_identifier(&r.work_date)?;
        let cql = format!(
            "INSERT INTO {ks}.plc_robot_day (work_date, payload, updated_at) \
             VALUES ({d}, {p}, {u})",
            ks = self.keyspace,
            d = quote_text(&r.work_date),
            p = quote_text(&r.payload),
            u = r.updated_at,
        );
        self.execute(&cql).await?;
        Ok(())
    }
}
