-- Add migration script here
ALTER TABLE users
ADD COLUMN roles TEXT[] NOT NULL DEFAULT ARRAY['User'];