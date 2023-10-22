-- Add migration script here
CREATE TABLE IF NOT EXISTS TIMERS (
    id          INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    unique_id   TEXT NOT NULL,
    start_time  INTEGER NOT NULL, -- Unix epoch of timer start in UTC
    is_current  BOOLEAN NOT NULL CHECK (is_current IN (0, 1)), -- Boolean value 0 false 1 true
    duration    INTEGER NOT NULL DEFAULT 0,   -- The number of seconds this timer lasted for
    project_id  INTEGER NOT NULL,
    FOREIGN KEY (project_id) -- Foreign key to projects
        REFERENCES PROJECTS (id)
        ON DELETE CASCADE
);

-- A project contains 0 or more timers
CREATE TABLE IF NOT EXISTS PROJECTS (
    id          INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    name        TEXT NOT NULL,
    unique_id   TEXT NOT NULL,
    is_current  BOOLEAN NOT NULL CHECK (is_current IN (0, 1)) -- Boolean value 0 false 1 true
);