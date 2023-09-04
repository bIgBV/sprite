-- Add migration script here
CREATE TABLE IF NOT EXISTS TIMERS (
    ID          INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    UNIQUE_ID   TEXT NOT NULL,
    START_TIME  INTEGER DEFAULT (cast(strftime('%s', 'now') as int)) NOT NULL, -- Unix epoch of timer start
    IS_CURRENT  BOOLEAN NOT NULL CHECK (IS_CURRENT IN (0, 1)), -- Boolean value 0 false 1 true
    DURATION    INTEGER   -- The number of seconds this timer lasted for
);
