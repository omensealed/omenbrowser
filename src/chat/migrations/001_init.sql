CREATE TABLE IF NOT EXISTS saved_servers(
  server_id TEXT PRIMARY KEY,
  destination TEXT NOT NULL,
  lxmf_destination TEXT,
  display_name TEXT NOT NULL,
  descriptor_json TEXT,
  descriptor_revision INTEGER,
  last_connected_at INTEGER,
  created_at INTEGER
);

CREATE TABLE IF NOT EXISTS rooms(
  server_id TEXT NOT NULL,
  room_id INTEGER NOT NULL,
  name TEXT NOT NULL,
  topic TEXT,
  mode_bits INTEGER NOT NULL DEFAULT 0,
  room_revision INTEGER NOT NULL DEFAULT 0,
  joined INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(server_id, room_id)
);

CREATE TABLE IF NOT EXISTS users(
  server_id TEXT NOT NULL,
  user_id INTEGER NOT NULL,
  display_name TEXT NOT NULL,
  role_bits INTEGER NOT NULL DEFAULT 0,
  status_bits INTEGER NOT NULL DEFAULT 0,
  profile_revision INTEGER NOT NULL DEFAULT 0,
  lxmf_available INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(server_id, user_id)
);

CREATE TABLE IF NOT EXISTS room_userlist(
  server_id TEXT NOT NULL,
  room_id INTEGER NOT NULL,
  user_id INTEGER NOT NULL,
  role_bits INTEGER NOT NULL DEFAULT 0,
  joined_at INTEGER,
  PRIMARY KEY(server_id, room_id, user_id)
);

CREATE TABLE IF NOT EXISTS room_events(
  server_id TEXT NOT NULL,
  room_id INTEGER NOT NULL,
  event_id INTEGER NOT NULL,
  event_kind INTEGER NOT NULL,
  actor_user_id INTEGER,
  actor_display_name TEXT,
  at INTEGER NOT NULL,
  payload TEXT,
  deleted INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(server_id, room_id, event_id)
);

CREATE TABLE IF NOT EXISTS history_ranges(
  server_id TEXT NOT NULL,
  room_id INTEGER NOT NULL,
  first_event_id INTEGER NOT NULL,
  last_event_id INTEGER NOT NULL,
  complete_before INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(server_id, room_id, first_event_id, last_event_id)
);

CREATE TABLE IF NOT EXISTS drafts(
  server_id TEXT NOT NULL,
  room_id INTEGER NOT NULL,
  body TEXT NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY(server_id, room_id)
);
