//! 하루치 집계를 미리 계산해 담아두는 테이블 — paintrobot.daily_rollup.
//!
//! 대시보드가 읽는 값은 전부 일별 집계다. 그런데 `jobs`의 PK가 `event_id`라
//! 행 하나가 파티션 하나이고, `WHERE work_date=...` 스캔은 수십만 번의 개별
//! 파티션 읽기가 된다 (실측 282초). 집계는 하루에 한 번만 바뀌면 되는 값이라
//! 미리 계산해 날짜당 한 행으로 두면 조회가 단일 키 조회가 된다.
//!
//! `daily_rollup`은 하루 한 행이라 1년치를 전부 훑어도 수백 행이다.

use crate::{
    check_identifier, decode_i64, decode_text, quote_text, CoreDbClient, HttpTransport, RepoError,
};

#[derive(Debug, Clone)]
pub struct RollupRow {
    pub work_date: String,
    /// 집계 결과 JSON. 스키마를 바꾸지 않고 항목을 늘릴 수 있게 문자열로 둔다.
    pub payload: String,
    pub updated_at: i64,
}

fn decode_rollup(row: &crate::RawRow) -> Result<RollupRow, RepoError> {
    let cols = &row.columns;
    let get = |name: &str| {
        cols.get(name)
            .ok_or_else(|| RepoError::Decode(format!("column {name} missing")))
    };
    Ok(RollupRow {
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
    pub async fn get_rollup(&self, work_date: &str) -> Result<Option<RollupRow>, RepoError> {
        check_identifier(work_date)?;
        let cql = format!(
            "SELECT work_date, payload, updated_at FROM {ks}.daily_rollup WHERE work_date={d}",
            ks = self.keyspace,
            d = quote_text(work_date),
        );
        let rows = self.execute(&cql).await?;
        rows.first().map(decode_rollup).transpose()
    }

    /// 구간 조회. 날짜당 한 행이라 스캔이 싸다.
    pub async fn scan_rollups(
        &self,
        from: &str,
        to: &str,
        limit: u32,
    ) -> Result<Vec<RollupRow>, RepoError> {
        check_identifier(from)?;
        check_identifier(to)?;
        let cql = format!(
            "SELECT work_date, payload, updated_at FROM {ks}.daily_rollup \
             WHERE work_date>={f} AND work_date<={t} LIMIT {n}",
            ks = self.keyspace,
            f = quote_text(from),
            t = quote_text(to),
            n = limit,
        );
        let rows = self.execute(&cql).await?;
        rows.iter().map(decode_rollup).collect()
    }

    pub async fn all_rollup_dates(&self, limit: u32) -> Result<Vec<String>, RepoError> {
        let cql = format!(
            "SELECT work_date FROM {ks}.daily_rollup LIMIT {n}",
            ks = self.keyspace,
            n = limit,
        );
        let rows = self.execute(&cql).await?;
        rows.iter()
            .map(|r| {
                decode_text(
                    r.columns
                        .get("work_date")
                        .ok_or_else(|| RepoError::Decode("work_date missing".into()))?,
                )
            })
            .collect()
    }

    /// 같은 PK INSERT = upsert.
    ///
    /// `payload`에는 `check_identifier`를 걸지 않는다 — JSON이라 통과할 수 없다.
    /// 대신 `quote_text`가 작은따옴표를 이스케이프해 CQL 문자열을 벗어나지
    /// 못하게 한다.
    pub async fn upsert_rollup(&self, r: &RollupRow) -> Result<(), RepoError> {
        check_identifier(&r.work_date)?;
        let cql = format!(
            "INSERT INTO {ks}.daily_rollup (work_date, payload, updated_at) \
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
