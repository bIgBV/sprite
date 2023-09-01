-- Add migration script here
CREATE TABLE IF NOT EXISTS TIMERS (
    ID          INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    UNIQUE_ID   INT NOT NULL,
    START_TIME  INTEGER DEFAULT (cast(strftime('%s', 'now') as int)) NOT NULL, -- Unix epoch of timer start
    IS_CURRENT  INT NOT NULL, -- Boolean value 0 true 1 false
    DURATION    INTEGER   -- The number of seconds this timer lasted for
);
