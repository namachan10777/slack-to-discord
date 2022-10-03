-- Add migration script here
CREATE TABLE IF NOT EXISTS files (
    url TEXT NOT NULL PRIMARY KEY,
    inner BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS posts (
    slack_id TEXT NOT NULL PRIMARY KEY,
    discord_id TEXT NOT NULL
);