CREATE TABLE settings (
    key      TEXT PRIMARY KEY,
    value    TEXT NOT NULL,
    updated  INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE bias_seeds (
    mac      BLOB PRIMARY KEY,
    serial   TEXT NOT NULL,
    bias_x   REAL NOT NULL,
    bias_y   REAL NOT NULL,
    bias_z   REAL NOT NULL,
    updated  INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE device_history (
    mac          BLOB PRIMARY KEY,
    serial       TEXT NOT NULL,
    kind         TEXT NOT NULL,
    last_seen    INTEGER NOT NULL,
    rotation_deg REAL NOT NULL DEFAULT 0
);

CREATE TABLE calibration (
    mac           BLOB PRIMARY KEY,
    accel_off_x   INTEGER NOT NULL,
    accel_off_y   INTEGER NOT NULL,
    accel_off_z   INTEGER NOT NULL,
    gyro_off_x    INTEGER NOT NULL,
    gyro_off_y    INTEGER NOT NULL,
    gyro_off_z    INTEGER NOT NULL,
    updated       INTEGER NOT NULL DEFAULT (unixepoch())
);
