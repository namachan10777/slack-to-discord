-- Add migration script here
-- Add migration script here

DROP TABLE posts;

CREATE TABLE IF NOT EXISTS posts (
    id VARCHAR(20) PRIMARY KEY NOT NULL,
    slack_channel_id VARCHAR(9) NOT NULL,
    discord_channel_id VARCHAR(20) NOT NULL,
    slack_ts TEXT NOT NULL,
    discord_thread_id TEXT
);

CREATE INDEX posts_index ON posts (slack_ts);