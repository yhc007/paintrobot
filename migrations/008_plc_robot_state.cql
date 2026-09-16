CREATE TABLE paintrobot.plc_robot_state (edge_id TEXT PRIMARY KEY, robot_model TEXT, model_no BIGINT, snapshot_json TEXT, read_errors TEXT, degraded BIGINT, io_streak BIGINT, received_at BIGINT)
