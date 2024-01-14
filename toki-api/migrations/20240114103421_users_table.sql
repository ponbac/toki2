-- Add migration script here
CREATE TABLE users
(
    id           SERIAL PRIMARY KEY,
    username     TEXT NOT NULL UNIQUE,
    access_token TEXT NOT NULL
);