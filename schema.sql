CREATE TABLE IF NOT EXISTS files (
    id BIGSERIAL NOT NULL PRIMARY KEY,
    url TEXT NOT NULL,
    inner BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS posts (
    slack_id TEXT NOT NULL PRIMARY KEY,
    discord_id TEXT NOT NULL
);