-- Add migration script here
CREATE TABLE users
(
    id           SERIAL PRIMARY KEY,
    email     TEXT NOT NULL UNIQUE,
    full_name TEXT NOT NULL,
    picture TEXT NOT NULL,
    access_token TEXT NOT NULL
);