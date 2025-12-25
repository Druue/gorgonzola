CREATE TABLE IF NOT EXISTS "user" (discord_id VARCHAR(255) PRIMARY KEY);

CREATE TABLE IF NOT EXISTS civ_discord_user_map (
  civ_user_name VARCHAR(255) PRIMARY KEY,
  discord_id VARCHAR(255) REFERENCES "user" (discord_id) ON DELETE CASCADE
);