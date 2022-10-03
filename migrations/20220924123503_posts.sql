-- Add migration script here
CREATE TABLE IF NOT EXISTS files (
    url TEXT NOT NULL PRIMARY KEY,
    inner BLOB NOT NULL
);

DROP TABLE posts;

CREATE TABLE IF NOT EXISTS posts (
    id BIGSERIAL NOT NULL PRIMARY KEY,
    slack_channel_id VARCHAR(9) NOT NULL,
    discord_channel_id VARCHAR(20) NOT NULL,
    slack_ts TEXT NOT NULL,
    discord_thread_id TEXT
);

CREATE INDEX posts_index ON posts (slack_ts);