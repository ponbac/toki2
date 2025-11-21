CREATE TABLE IF NOT EXISTS user_avatars
(
    user_id   INTEGER PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    image     BYTEA   NOT NULL,
    mime_type TEXT    NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO user_avatars (user_id, image, mime_type)
SELECT id, avatar_image, COALESCE(avatar_image_mime_type, 'image/webp')
FROM users
WHERE avatar_image IS NOT NULL
ON CONFLICT (user_id) DO UPDATE
SET image = EXCLUDED.image,
    mime_type = EXCLUDED.mime_type,
    created_at = now();

ALTER TABLE users
    DROP COLUMN IF EXISTS avatar_image,
    DROP COLUMN IF EXISTS avatar_image_mime_type;

