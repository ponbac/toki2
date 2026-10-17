-- Timer buddies: each user's buddy profile, their buddies, the XP they earn and the
-- battles they fight.
-- Dates are local calendar dates in the `ai_usage.time_zone` setting, the company's
-- time zone. A user enrols by creating a profile with a lead buddy (slot 1); the
-- partner (slot 2) unlocks later and is never removed, only hidden. XP is not stored
-- per buddy: a user's total XP is the XP of their logged-hours days plus a fixed
-- amount per battle they started, and a partner earns from `xp_base`, the user's
-- total XP when it was first set.

CREATE TABLE buddy_profiles
(
    user_id                 INTEGER     PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    -- Whether the buddy is shown and can battle and be battled.
    enabled                 BOOLEAN     NOT NULL,
    -- Whether the user gets a push notification when a colleague battles them.
    battle_notifications    BOOLEAN     NOT NULL DEFAULT true,
    -- The local date the profile was created. Logged hours count from this day on.
    xp_since                DATE        NOT NULL,
    -- When logged hours were last read from time tracking, NULL before the first read.
    hours_synced_at         TIMESTAMPTZ,
    -- When a read of logged hours last started, whether or not it worked, NULL before
    -- the first. Reads are spaced out by this, so a failing one is not retried at once.
    hours_sync_attempted_at TIMESTAMPTZ,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE buddies
(
    user_id    INTEGER     NOT NULL REFERENCES buddy_profiles (user_id) ON DELETE CASCADE,
    -- 1 is the lead, 2 the partner.
    slot       SMALLINT    NOT NULL,
    -- A national dex number. The app only offers species with sprites and battle data.
    species_id SMALLINT    NOT NULL,
    -- A hidden partner is not shown but still fights.
    visible    BOOLEAN     NOT NULL DEFAULT true,
    -- The user's total XP when the buddy was first set; it earns the XP above this.
    -- Always 0 for the lead.
    xp_base    BIGINT      NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT buddies_pkey PRIMARY KEY (user_id, slot),
    CONSTRAINT buddies_slot_check CHECK (slot IN (1, 2)),
    CONSTRAINT buddies_species_id_check CHECK (species_id BETWEEN 1 AND 649),
    CONSTRAINT buddies_xp_base_check CHECK (xp_base >= 0)
);

-- The hours a user logged on one local day and the XP they give. Days are read again
-- while they are recent, so edits made later in time tracking replace them.
CREATE TABLE buddy_xp_days
(
    user_id      INTEGER          NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    day          DATE             NOT NULL,
    logged_hours DOUBLE PRECISION NOT NULL,
    xp           INTEGER          NOT NULL,
    updated_at   TIMESTAMPTZ      NOT NULL DEFAULT now(),
    CONSTRAINT buddy_xp_days_pkey PRIMARY KEY (user_id, day),
    CONSTRAINT buddy_xp_days_logged_hours_check CHECK (logged_hours >= 0),
    CONSTRAINT buddy_xp_days_xp_check CHECK (xp >= 0)
);

-- A battle, resolved once when it was started. Both teams are stored as snapshots of
-- the buddies at that moment, with the seed, the engine version and the events, so a
-- replay never depends on later level-ups or rule changes. A colleague battle has a
-- defender; a battle against the daily trainer has none and names the trainer.
CREATE TABLE buddy_battles
(
    id               SERIAL PRIMARY KEY,
    kind             TEXT        NOT NULL,
    challenger_id    INTEGER     NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    defender_id      INTEGER REFERENCES users (id) ON DELETE CASCADE,
    -- The local date the battle was started.
    battle_day       DATE        NOT NULL,
    -- The engine's u32 seed.
    seed             BIGINT      NOT NULL,
    engine_version   SMALLINT    NOT NULL,
    -- The winning side, NULL for a draw.
    winner           TEXT,
    challenger_team  JSONB       NOT NULL,
    defender_team    JSONB       NOT NULL,
    events           JSONB       NOT NULL,
    trainer_name     TEXT,
    -- When the defender first saw the battle, NULL until then and for trainer battles.
    defender_seen_at TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT buddy_battles_kind_check CHECK (kind IN ('colleague', 'trainer')),
    CONSTRAINT buddy_battles_winner_check CHECK (winner IN ('challenger', 'defender')),
    CONSTRAINT buddy_battles_seed_check CHECK (seed BETWEEN 0 AND 4294967295),
    CONSTRAINT buddy_battles_defender_check CHECK ((kind = 'trainer') = (defender_id IS NULL)),
    CONSTRAINT buddy_battles_trainer_name_check
        CHECK ((kind = 'trainer') = (trainer_name IS NOT NULL)),
    CONSTRAINT buddy_battles_self_check CHECK (challenger_id <> defender_id)
);

CREATE INDEX buddy_battles_challenger_idx ON buddy_battles (challenger_id, created_at DESC);
CREATE INDEX buddy_battles_defender_idx ON buddy_battles (defender_id, created_at DESC);

-- A user battles the same colleague at most once per local day, and the daily trainer
-- once per local day.
CREATE UNIQUE INDEX buddy_battles_colleague_daily_key
    ON buddy_battles (challenger_id, defender_id, battle_day)
    WHERE kind = 'colleague';
CREATE UNIQUE INDEX buddy_battles_trainer_daily_key
    ON buddy_battles (challenger_id, battle_day)
    WHERE kind = 'trainer';
