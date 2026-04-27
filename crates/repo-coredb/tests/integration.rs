//! Integration tests against a live CoreDB HTTP API.
//!
//! Set COREDB_URL=http://127.0.0.1:9043 to run, otherwise tests are skipped.
//! The paintrobot keyspace and tables must exist (see migrations/).

use chrono::{DateTime, FixedOffset};
use paintrobot_repo_coredb::{CoreDbClient, ReqwestTransport, WeatherRow};
use paintrobot_schema::{JobIn, MatchStatus};
use uuid::Uuid;

fn client() -> Option<CoreDbClient<ReqwestTransport>> {
    let url = std::env::var("COREDB_URL").ok()?;
    Some(CoreDbClient::new(ReqwestTransport::new(), url, "paintrobot"))
}

fn parse_ts(s: &str) -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339(s).expect("rfc3339 ts")
}

#[tokio::test]
async fn roundtrip_matched_job() {
    let Some(c) = client() else {
        eprintln!("COREDB_URL not set, skipping");
        return;
    };

    let event_id = format!("it-{}", Uuid::now_v7());
    let job = JobIn {
        event_id: event_id.clone(),
        edge_id: "edge-test-01".into(),
        plc_model_no: Some("HD-A120".into()),
        camera_model_no: Some("HD-A120".into()),
        plc_ts: Some(parse_ts("2026-04-23T09:12:04.321+09:00")),
        camera_ts: Some(parse_ts("2026-04-23T09:12:06.880+09:00")),
        confidence: Some(0.97),
        image_ref: None,
    };

    c.insert_job(&job, "2026-04-23", MatchStatus::Matched, 1713830003000)
        .await
        .expect("insert");

    let got = c.get_job(&event_id).await.expect("get").expect("row");
    assert_eq!(got.event_id, event_id);
    assert_eq!(got.plc_model_no.as_deref(), Some("HD-A120"));
    assert_eq!(got.camera_model_no.as_deref(), Some("HD-A120"));
    assert_eq!(got.match_status, "matched");
    assert_eq!(got.work_date, "2026-04-23");
    assert_eq!(got.confidence, Some(0.97));
}

#[tokio::test]
async fn scan_by_work_date_sees_insert() {
    let Some(c) = client() else {
        eprintln!("COREDB_URL not set, skipping");
        return;
    };

    let date = "2026-04-24";
    let event_id = format!("it-{}", Uuid::now_v7());
    let job = JobIn {
        event_id: event_id.clone(),
        edge_id: "edge-test-01".into(),
        plc_model_no: Some("HD-Z999".into()),
        camera_model_no: Some("HD-Z999".into()),
        plc_ts: Some(parse_ts("2026-04-24T00:00:00+09:00")),
        camera_ts: Some(parse_ts("2026-04-24T00:00:02+09:00")),
        confidence: Some(0.88),
        image_ref: None,
    };
    c.insert_job(&job, date, MatchStatus::Matched, 1713830003000)
        .await
        .unwrap();

    let rows = c.scan_jobs_for_date(date, 1000).await.unwrap();
    assert!(
        rows.iter().any(|r| r.event_id == event_id),
        "inserted row not returned by scan"
    );
}

#[tokio::test]
async fn aggregate_today() {
    let Some(c) = client() else {
        eprintln!("COREDB_URL not set, skipping");
        return;
    };

    let date = "2026-04-25";
    // Insert 3 rows: 2 matched A, 1 mismatch B/C
    for (plc, cam, status) in [
        ("HD-A120", "HD-A120", MatchStatus::Matched),
        ("HD-A120", "HD-A120", MatchStatus::Matched),
        ("HD-B200", "HD-B999", MatchStatus::Mismatch),
    ] {
        let event_id = format!("it-{}", Uuid::now_v7());
        let job = JobIn {
            event_id,
            edge_id: "edge-test-02".into(),
            plc_model_no: Some(plc.into()),
            camera_model_no: Some(cam.into()),
            plc_ts: Some(parse_ts("2026-04-25T09:00:00+09:00")),
            camera_ts: Some(parse_ts("2026-04-25T09:00:02+09:00")),
            confidence: Some(0.9),
            image_ref: None,
        };
        c.insert_job(&job, date, status, 1713830003000).await.unwrap();
    }

    let agg_rows = c.agg_rows_for_date(date, 1000).await.unwrap();
    let stats = paintrobot_domain::aggregate(date.to_string(), agg_rows);
    // Counts are "at least" because CoreDB has no DELETE, so prior test runs may add more.
    assert!(stats.total_jobs >= 3);
    assert!(stats.mismatch_jobs >= 1);
    let model_a = stats.models.iter().find(|m| m.model_no == "HD-A120").unwrap();
    assert!(model_a.job_count >= 2);
}

#[tokio::test]
async fn weather_roundtrip() {
    let Some(c) = client() else {
        eprintln!("COREDB_URL not set, skipping");
        return;
    };
    // observed_at must pass check_identifier — keep to safe chars
    let observed_at = format!("2026-04-25T00:00:{:02}Z", (Uuid::now_v7().as_u128() % 60) as u32);
    let w = WeatherRow {
        observed_at: observed_at.clone(),
        temperature_c: 17.2,
        humidity_pct: 58.0,
        source: "owm".into(),
    };
    c.insert_weather(&w).await.unwrap();
    let got = c.get_weather(&observed_at).await.unwrap().unwrap();
    assert_eq!(got.observed_at, observed_at);
    assert!((got.temperature_c - 17.2).abs() < 1e-6);
    assert!((got.humidity_pct - 58.0).abs() < 1e-6);
    assert_eq!(got.source, "owm");
}
