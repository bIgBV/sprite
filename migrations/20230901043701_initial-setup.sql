-- Add migration script here
CREATE TABLE IF NOT EXISTS TIMERS (
    ID          INT PRIMARY KEY     NOT NULL,
    UNIQUEID    INT                 NOT NULL,
    STARTTIME   INT                 NOT NULL, -- Unix epoch of timer start
    ISCURRENT   INT                 NOT NULL, -- Boolean value 0 true 1 false
    ENDTIME     INT                 NOT NULL  -- Unix epoch of timer end
);
