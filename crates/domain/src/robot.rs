//! R1 로봇(MPX2600) 인터페이스 스냅샷에서 의미 있는 것만 뽑아낸다.
//!
//! 스냅샷을 그대로 쌓아두면 쓸모가 적다. 실제로 쓰이는 것은 **직전 스냅샷과
//! 비교해 나오는 전이(edge)** 와 **HMI 차종 ↔ 실제 차체 차종의 불일치** 다.
//! 스펙은 `plc_r.md` §4.3 / §4.4.
//!
//! 전부 순수 함수다. 시각은 호출자가 넘긴다 — 엣지 PLC에는 믿을 시계가 없어
//! 페이로드에 시각 필드가 없고, 수신 시각은 서버가 찍는다.

use paintrobot_schema::{BitMap, JigStation, RobotIn};

/// 전이 한 건. `detail`은 사람이 읽을 한 줄이고, 기계가 쓰는 값은 나머지다.
#[derive(Debug, Clone, PartialEq)]
pub struct RobotEvent {
    pub kind: RobotEventKind,
    pub ts_ms: i64,
    /// JIG 번호(1~5). 스테이션과 무관한 이벤트는 None.
    pub jig_no: Option<i64>,
    /// 이 이벤트가 가리키는 차종번호.
    pub model_no: Option<i64>,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RobotEventKind {
    /// n번 JIG 차체 정보가 로봇으로 넘어감 (`send_to_robot` false→true)
    JigHandoff,
    /// 전송 완료 (`send_done` false→true)
    JigHandoffDone,
    /// 로봇이 JOB 착수 (`di.job_in_progress` false→true)
    RobotJobStarted,
    /// 로봇 JOB 종료 = 1사이클 (`di.job_in_progress` true→false)
    RobotJobEnded,
    /// 도장 1사이클 완료 (`jig.completed.work_id` 값 변화)
    CycleCompleted,
    /// 알람 발생
    RobotFault,
    /// 비상정지
    EStop,
    /// HMI 입력과 실제 차체 불일치 — 오도장 위험
    WorkIdMismatch,
    /// 내부 릴레이와 출력 접점이 연속으로 어긋남 — 배선/출력 모듈 의심
    IoDisagree,
}

impl RobotEventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RobotEventKind::JigHandoff => "jig_handoff",
            RobotEventKind::JigHandoffDone => "jig_handoff_done",
            RobotEventKind::RobotJobStarted => "robot_job_started",
            RobotEventKind::RobotJobEnded => "robot_job_ended",
            RobotEventKind::CycleCompleted => "cycle_completed",
            RobotEventKind::RobotFault => "robot_fault",
            RobotEventKind::EStop => "estop",
            RobotEventKind::WorkIdMismatch => "work_id_mismatch",
            RobotEventKind::IoDisagree => "io_disagree",
        }
    }

    /// 화면에서 눈에 띄어야 하는가. 오도장/비상정지/알람만 참.
    pub fn is_alert(self) -> bool {
        matches!(
            self,
            RobotEventKind::WorkIdMismatch
                | RobotEventKind::EStop
                | RobotEventKind::RobotFault
                | RobotEventKind::IoDisagree
        )
    }
}

/// `command`와 `do`에서 같은 이름으로 오는 쌍. 정상이면 값이 같아야 한다.
/// 내부 릴레이는 ON인데 출력 접점이 OFF면 출력 모듈/배선 고장 의심.
pub const IO_PAIRS: [&str; 5] = [
    "external_start",
    "servo_on",
    "play_mode_select",
    "teach_mode_select",
    "call_master_job",
];

/// 스캔 타이밍 차이로 한두 스냅샷 어긋나는 것은 정상이다. 이만큼 연속으로
/// 어긋나야 경보한다 (스펙 §4.4).
pub const IO_DISAGREE_STREAK: u32 = 5;

/// `false → true` 전이인가. **`None`에서 온 것은 전이가 아니다** — 읽기
/// 실패로 값을 몰랐다가 이제 알게 된 것뿐이라, 가짜 이벤트가 된다.
fn rose(prev: Option<bool>, next: Option<bool>) -> bool {
    prev == Some(false) && next == Some(true)
}

fn fell(prev: Option<bool>, next: Option<bool>) -> bool {
    prev == Some(true) && next == Some(false)
}

fn bit(m: &BitMap, k: &str) -> Option<bool> {
    m.get(k).copied().flatten()
}

/// 지금 로봇에 넘어가는 중인 JIG. 전송 중 > JOB 시작 > 워크가 실린 마지막 칸
/// 순으로 고른다. 어느 것도 아니면 None(= 라인에 차체가 없음).
pub fn active_station(snap: &RobotIn) -> Option<&JigStation> {
    let st = &snap.jig.stations;
    st.iter()
        .find(|s| s.send_to_robot == Some(true))
        .or_else(|| st.iter().find(|s| s.job_start == Some(true)))
        .or_else(|| {
            st.iter()
                .rev()
                .find(|s| s.work_in.is_some_and(|w| w != 0) && s.work_id.is_some_and(|w| w != 0))
        })
}

/// 오도장 위험 판정. HMI 차종번호와 실제 차체에 붙어 시프트되는 WORK ID가
/// 다른 구간을 잡아낸다.
///
/// 이 엔드포인트의 핵심 가치다. 작업자가 HMI에서 차종을 바꿔도 컨베이어 위
/// 차체는 한동안 이전 차종이며, 그 사이에 새 레시피로 도장되면 어긋난다.
///
/// 어느 한쪽이라도 모르면(None) 판정하지 않는다 — 모름을 "정상"으로
/// 내보내면 안 된다. 그래서 반환이 `Option<bool>`이다.
pub fn work_id_mismatch(snap: &RobotIn) -> Option<bool> {
    let hmi = snap.model_no?;
    let st = active_station(snap)?;
    let work_id = st.work_id?;
    // 0은 "워크 없음"이라 비교 대상이 아니다.
    if work_id == 0 || hmi == 0 {
        return None;
    }
    Some(hmi != work_id)
}

/// `command` ↔ `do` 가 어긋난 키 목록. 한쪽이라도 모르면 그 키는 건너뛴다.
pub fn io_disagreements(snap: &RobotIn) -> Vec<&'static str> {
    IO_PAIRS
        .iter()
        .copied()
        .filter(|k| match (bit(&snap.robot.command, k), bit(&snap.robot.out, k)) {
            (Some(a), Some(b)) => a != b,
            _ => false,
        })
        .collect()
}

/// 알람/비상정지 계열 중 지금 켜져 있는 것. **`robot.alarm` 그룹만 본다.**
///
/// 스펙 §4.3은 `robot.di.fault`도 알람 판정에 넣으라고 하지만, 현장 실측에서
/// 로봇이 홈 위치·서보 OFF·TEACH 모드로 정상 대기 중인데 `di.cp_estop`과
/// `di.pp_estop`이 둘 다 ON이었다. 같은 순간 PLC 쪽 `alarm.panel_estop`/
/// `pendant_estop`은 전부 OFF였다. 비상정지 접점은 보통 NC(평상시 닫힘,
/// fail-safe)라 회로가 멀쩡하면 ON으로 읽히는데, 그 모양과 정확히 맞는다.
///
/// P 영역 접점의 극성이 현장에서 확인될 때까지 여기서 빼둔다. 관제 화면에
/// 오경보를 띄우는 것은 아무것도 안 띄우는 것보다 나쁘다. `alarm` 그룹은
/// PLC 내부 릴레이라 래더가 세우는 값이고, 스펙 §3.3이 이름부터 "로봇 알람"
/// 이라 극성이 분명하다.
///
/// 확인되면 `FAULT_GROUPS`에 `di`를 도로 넣으면 된다.
pub fn active_faults(snap: &RobotIn) -> Vec<String> {
    let mut out = Vec::new();
    for grp in FAULT_GROUPS {
        let m = match *grp {
            "alarm" => &snap.robot.alarm,
            "di" => &snap.robot.di,
            _ => continue,
        };
        for (k, v) in m.iter() {
            if *v == Some(true) && (k.contains("estop") || k.contains("fault")) {
                out.push(format!("{grp}.{k}"));
            }
        }
    }
    out
}

/// 알람으로 인정하는 비트 묶음. `active_faults` 머리말 참고 — 현장에서 P 영역
/// 접점 극성이 확인되면 `"di"`를 도로 넣는다.
pub const FAULT_GROUPS: &[&str] = &["alarm"];

/// 데이터 품질. `read_errors`가 있으면 그 스냅샷은 전이 검출에서 빠진다.
pub fn is_degraded(snap: &RobotIn) -> bool {
    !snap.read_errors.is_empty()
}

/// 직전 스냅샷과 비교해 파생 이벤트를 뽑는다 (스펙 §4.3).
///
/// `prev`가 없으면(첫 수신) 전이는 만들지 않는다 — 기준이 없는데 전이를
/// 만들면 재시작할 때마다 가짜 이벤트가 쏟아진다. 다만 상태로 판정되는
/// 오도장은 첫 수신에도 낸다.
///
/// `io_streak`는 호출자가 들고 다니는 연속 불일치 횟수다. 갱신된 값을 같이
/// 돌려주므로 그대로 저장했다가 다음 호출에 넘기면 된다.
pub fn detect_events(
    prev: Option<&RobotIn>,
    next: &RobotIn,
    ts_ms: i64,
    io_streak: u32,
) -> (Vec<RobotEvent>, u32) {
    let mut out = Vec::new();

    // §6.1 — 읽기 실패가 섞인 스냅샷은 전이 검출에서 제외한다. null→false를
    // 가짜 이벤트로 잡지 않도록. 연속 실패 횟수도 늘리지 않는다.
    if is_degraded(next) {
        return (out, io_streak);
    }

    let ev = |kind: RobotEventKind, jig_no, model_no, detail: String| RobotEvent {
        kind,
        ts_ms,
        jig_no,
        model_no,
        detail,
    };

    // 상태로 판정되는 것 — prev 없이도 낸다.
    if work_id_mismatch(next) == Some(true) {
        let st = active_station(next);
        out.push(ev(
            RobotEventKind::WorkIdMismatch,
            st.map(|s| s.no),
            next.model_no,
            format!(
                "HMI 차종 {} ↔ JIG {} 차체 {} — 오도장 위험",
                next.model_no.unwrap_or_default(),
                st.map(|s| s.no).unwrap_or_default(),
                st.and_then(|s| s.work_id).unwrap_or_default(),
            ),
        ));
    }

    // command ↔ do 불일치는 연속 N회일 때만 (§4.4).
    let disagree = io_disagreements(next);
    let streak = if disagree.is_empty() { 0 } else { io_streak + 1 };
    if !disagree.is_empty() && streak == IO_DISAGREE_STREAK {
        out.push(ev(
            RobotEventKind::IoDisagree,
            None,
            next.model_no,
            format!(
                "내부 릴레이와 출력 접점이 {}회 연속 불일치: {}",
                streak,
                disagree.join(", ")
            ),
        ));
    }

    let Some(prev) = prev else {
        return (out, streak);
    };
    // 직전이 품질 불량이었으면 그걸 기준으로 전이를 재지 않는다.
    if is_degraded(prev) {
        return (out, streak);
    }

    for (i, st) in next.jig.stations.iter().enumerate() {
        let Some(p) = prev.jig.stations.get(i) else {
            continue;
        };
        if rose(p.send_to_robot, st.send_to_robot) {
            out.push(ev(
                RobotEventKind::JigHandoff,
                Some(st.no),
                st.work_id,
                format!(
                    "JIG {} 차체 {} 정보를 로봇으로 전송 시작",
                    st.no,
                    st.work_id.unwrap_or_default()
                ),
            ));
        }
        if rose(p.send_done, st.send_done) {
            out.push(ev(
                RobotEventKind::JigHandoffDone,
                Some(st.no),
                st.work_id,
                format!("JIG {} 전송 완료", st.no),
            ));
        }
    }

    let pj = bit(&prev.robot.di, "job_in_progress");
    let nj = bit(&next.robot.di, "job_in_progress");
    if rose(pj, nj) {
        out.push(ev(
            RobotEventKind::RobotJobStarted,
            active_station(next).map(|s| s.no),
            next.model_no,
            "로봇 JOB 착수".into(),
        ));
    }
    if fell(pj, nj) {
        out.push(ev(
            RobotEventKind::RobotJobEnded,
            active_station(next).map(|s| s.no),
            next.model_no,
            "로봇 JOB 종료 (1사이클)".into(),
        ));
    }

    // 완료 WORK ID가 바뀌면 1사이클 완료. 0으로 떨어지는 것은 리셋이라 뺀다.
    if prev.jig.completed.work_id != next.jig.completed.work_id {
        if let Some(w) = next.jig.completed.work_id.filter(|w| *w != 0) {
            out.push(ev(
                RobotEventKind::CycleCompleted,
                None,
                Some(w),
                format!("차종 {w} 도장 1사이클 완료"),
            ));
        }
    }

    // 같은 이유로 전이도 `alarm` 그룹만 본다. 극성이 뒤집혀 있으면 비상정지
    // *해제*가 "비상정지 발생"으로 기록돼 로그가 거꾸로 쌓인다.
    for grp in FAULT_GROUPS {
        let (pm, nm) = match *grp {
            "alarm" => (&prev.robot.alarm, &next.robot.alarm),
            "di" => (&prev.robot.di, &next.robot.di),
            _ => continue,
        };
        for (k, v) in nm.iter() {
            if !rose(bit(pm, k), *v) {
                continue;
            }
            let kind = if k.contains("estop") {
                RobotEventKind::EStop
            } else if k.contains("fault") {
                RobotEventKind::RobotFault
            } else {
                continue;
            };
            out.push(ev(kind, None, next.model_no, format!("{grp}.{k} 발생")));
        }
    }

    (out, streak)
}

#[cfg(test)]
mod tests {
    use super::*;
    use paintrobot_schema::{JigCompleted, JigIn, JigStation, RobotIo};

    fn bits(pairs: &[(&str, Option<bool>)]) -> BitMap {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    fn station(no: i64, work_id: i64, work_in: i64) -> JigStation {
        JigStation {
            no,
            work_id: Some(work_id),
            work_in: Some(work_in),
            shift_dist: Some(0),
            job_start_dist: Some(150),
            send_to_robot: Some(false),
            job_start: Some(false),
            send_done: Some(false),
        }
    }

    fn snap(model_no: Option<i64>) -> RobotIn {
        RobotIn {
            edge_id: "edge-line-01".into(),
            robot_model: Some("MPX2600".into()),
            model_no,
            robot: RobotIo {
                di: bits(&[("job_in_progress", Some(false)), ("fault", Some(false))]),
                alarm: bits(&[("panel_estop", Some(false)), ("fault", Some(false))]),
                ..Default::default()
            },
            jig: JigIn {
                stations: (1..=5).map(|n| station(n, 0, 0)).collect(),
                completed: JigCompleted {
                    work_id: Some(0),
                    work_in: Some(0),
                },
                ..Default::default()
            },
            read_errors: vec![],
        }
    }

    #[test]
    fn handoff_is_a_rising_edge_only() {
        let prev = snap(Some(8));
        let mut next = prev.clone();
        next.jig.stations[1].send_to_robot = Some(true);
        let (evs, _) = detect_events(Some(&prev), &next, 100, 0);
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].kind, RobotEventKind::JigHandoff);
        assert_eq!(evs[0].jig_no, Some(2));

        // 이미 true인 상태가 이어지면 다시 내지 않는다.
        let (evs, _) = detect_events(Some(&next), &next, 200, 0);
        assert!(evs.is_empty());
    }

    #[test]
    fn null_to_false_is_not_an_edge() {
        // 읽기 실패로 몰랐다가 false로 확정된 것은 전이가 아니다.
        let mut prev = snap(Some(8));
        prev.jig.stations[0].send_done = None;
        let mut next = prev.clone();
        next.jig.stations[0].send_done = Some(false);
        let (evs, _) = detect_events(Some(&prev), &next, 100, 0);
        assert!(evs.is_empty());
    }

    #[test]
    fn degraded_snapshot_is_skipped() {
        let prev = snap(Some(8));
        let mut next = prev.clone();
        next.jig.stations[1].send_to_robot = Some(true);
        next.read_errors = vec!["%PW12 x10words: connect refused".into()];
        let (evs, streak) = detect_events(Some(&prev), &next, 100, 3);
        assert!(evs.is_empty());
        assert_eq!(streak, 3, "품질 불량 스냅샷은 연속 횟수도 건드리지 않는다");
    }

    #[test]
    fn work_id_mismatch_needs_both_sides_known() {
        let mut s = snap(Some(8));
        s.jig.stations[1] = station(2, 8, 1);
        s.jig.stations[1].send_to_robot = Some(true);
        assert_eq!(work_id_mismatch(&s), Some(false));

        // HMI에서 7로 바꿨는데 컨베이어 위 차체는 아직 8
        s.model_no = Some(7);
        assert_eq!(work_id_mismatch(&s), Some(true));

        // 차종을 모르면 "정상"이 아니라 "판정 불가"
        s.model_no = None;
        assert_eq!(work_id_mismatch(&s), None);

        // 워크 없음(0)도 판정 대상이 아니다
        s.model_no = Some(7);
        s.jig.stations[1].work_id = Some(0);
        assert_eq!(work_id_mismatch(&s), None);
    }

    #[test]
    fn mismatch_is_reported_without_a_previous_snapshot() {
        let mut s = snap(Some(7));
        s.jig.stations[1] = station(2, 8, 1);
        s.jig.stations[1].send_to_robot = Some(true);
        let (evs, _) = detect_events(None, &s, 100, 0);
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].kind, RobotEventKind::WorkIdMismatch);
        assert_eq!(evs[0].jig_no, Some(2));
    }

    #[test]
    fn io_disagreement_fires_only_after_a_streak() {
        let mut s = snap(Some(8));
        s.robot.command = bits(&[("servo_on", Some(true))]);
        s.robot.out = bits(&[("servo_on", Some(false))]);
        assert_eq!(io_disagreements(&s), vec!["servo_on"]);

        let mut streak = 0;
        for i in 1..IO_DISAGREE_STREAK {
            let (evs, st) = detect_events(Some(&s), &s, 100, streak);
            streak = st;
            assert!(evs.is_empty(), "{i}회째에는 아직 경보하지 않는다");
        }
        let (evs, streak) = detect_events(Some(&s), &s, 100, streak);
        assert_eq!(streak, IO_DISAGREE_STREAK);
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].kind, RobotEventKind::IoDisagree);

        // 한쪽이 null이면 불일치로 세지 않는다
        s.robot.out = bits(&[("servo_on", None)]);
        assert!(io_disagreements(&s).is_empty());
    }

    #[test]
    fn job_cycle_and_completion() {
        let prev = snap(Some(8));
        let mut next = prev.clone();
        next.robot.di.insert("job_in_progress".into(), Some(true));
        let (evs, _) = detect_events(Some(&prev), &next, 100, 0);
        assert_eq!(evs[0].kind, RobotEventKind::RobotJobStarted);

        let mut done = next.clone();
        done.robot.di.insert("job_in_progress".into(), Some(false));
        done.jig.completed.work_id = Some(8);
        let (evs, _) = detect_events(Some(&next), &done, 200, 0);
        let kinds: Vec<_> = evs.iter().map(|e| e.kind).collect();
        assert!(kinds.contains(&RobotEventKind::RobotJobEnded));
        assert!(kinds.contains(&RobotEventKind::CycleCompleted));
        assert_eq!(
            evs.iter()
                .find(|e| e.kind == RobotEventKind::CycleCompleted)
                .unwrap()
                .model_no,
            Some(8)
        );
    }

    #[test]
    fn estop_and_fault_are_separate_kinds() {
        let prev = snap(Some(8));
        let mut next = prev.clone();
        next.robot.alarm.insert("panel_estop".into(), Some(true));
        next.robot.alarm.insert("fault".into(), Some(true));
        let (evs, _) = detect_events(Some(&prev), &next, 100, 0);
        let kinds: Vec<_> = evs.iter().map(|e| e.kind).collect();
        assert!(kinds.contains(&RobotEventKind::EStop));
        assert!(kinds.contains(&RobotEventKind::RobotFault));
        assert!(RobotEventKind::EStop.is_alert());
    }

    #[test]
    fn p_area_contacts_do_not_raise_alarms() {
        // 극성 미확인 구간. 현장 실측에서 정상 대기 중에도 di.cp_estop과
        // di.pp_estop이 ON이었다 — NC 접점으로 보인다. 확인 전까지는
        // 경보도 전이도 내지 않는다.
        let mut s = snap(Some(8));
        s.robot.di.insert("cp_estop".into(), Some(true));
        s.robot.di.insert("pp_estop".into(), Some(true));
        assert!(active_faults(&s).is_empty());

        let prev = snap(Some(8));
        let (evs, _) = detect_events(Some(&prev), &s, 100, 0);
        assert!(evs.is_empty());

        // PLC 내부 알람 릴레이는 그대로 잡는다.
        let mut alarmed = snap(Some(8));
        alarmed.robot.alarm.insert("panel_estop".into(), Some(true));
        assert_eq!(active_faults(&alarmed), vec!["alarm.panel_estop"]);
    }

    #[test]
    fn active_station_prefers_the_one_being_sent() {
        let mut s = snap(Some(8));
        s.jig.stations[0] = station(1, 8, 1);
        s.jig.stations[3] = station(4, 7, 1);
        s.jig.stations[3].send_to_robot = Some(true);
        assert_eq!(active_station(&s).map(|x| x.no), Some(4));

        // 전송 중인 것이 없으면 워크가 실린 마지막 칸
        s.jig.stations[3].send_to_robot = Some(false);
        assert_eq!(active_station(&s).map(|x| x.no), Some(4));

        // 라인이 비면 None
        let empty = snap(Some(8));
        assert!(active_station(&empty).is_none());
    }
}
