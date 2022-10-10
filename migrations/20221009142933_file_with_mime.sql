-- Add migration script here
DROP TABLE files;

CREATE TABLE files (
    url TEXT NOT NULL PRIMARY KEY,
    inner BLOB NOT NULL,
    mime TEXT NOT NULL
);