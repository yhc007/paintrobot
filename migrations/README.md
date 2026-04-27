# Paintrobot CQL Migrations

CoreDB(https://github.com/yhc007/coredb)는 CQL의 매우 제한된 서브셋만 파싱합니다.
이 스키마는 실측을 통해 확인된 허용 구문만 사용합니다.

## CoreDB 지원 범위 (2026-04-24 실측)

- ✅ `CREATE KEYSPACE name WITH REPLICATION = {'class': 'SimpleStrategy', 'replication_factor': N}` (IF NOT EXISTS 불가)
- ✅ `CREATE TABLE ks.t (col TYPE, ..., col TYPE PRIMARY KEY)` — **단일 PRIMARY KEY만**
- ✅ `INSERT INTO ks.t (cols) VALUES (vals)` — quoted strings OK
- ✅ `SELECT cols FROM ks.t [WHERE ...] [LIMIT n]` — `=, !=, >, <, >=, <=, LIKE, IN`
- ❌ `UPDATE`, `DELETE`, counter, composite PK, BATCH, `IF NOT EXISTS`

## 적용 방법

```bash
./scripts/apply_migrations.sh  # 순서대로 http://localhost:9043/query POST
```

멱등성 없음(CREATE IF NOT EXISTS 미지원). 최초 1회만 실행.
