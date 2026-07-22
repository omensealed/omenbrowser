CREATE TABLE IF NOT EXISTS server_config (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS identities (
  id INTEGER PRIMARY KEY,
  label TEXT NOT NULL,
  identity_path TEXT NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS users (
  user_id INTEGER PRIMARY KEY,
  rns_identity_hash BLOB NOT NULL UNIQUE,
  display_name TEXT NOT NULL,
  role_bits INTEGER NOT NULL DEFAULT 0,
  status_bits INTEGER NOT NULL DEFAULT 0,
  profile_revision INTEGER NOT NULL DEFAULT 0,
  lxmf_destination TEXT,
  lxmf_visibility TEXT NOT NULL DEFAULT 'on_request',
  first_seen_at INTEGER NOT NULL,
  last_seen_at INTEGER
);

CREATE TABLE IF NOT EXISTS rooms (
  room_id INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  topic TEXT,
  mode_bits INTEGER NOT NULL DEFAULT 0,
  room_revision INTEGER NOT NULL DEFAULT 0,
  created_by_user_id INTEGER,
  created_at INTEGER NOT NULL,
  archived INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS room_events (
  room_id INTEGER NOT NULL,
  event_id INTEGER NOT NULL,
  event_kind INTEGER NOT NULL,
  actor_user_id INTEGER,
  target_user_id INTEGER,
  at INTEGER NOT NULL,
  payload BLOB,
  deleted INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(room_id, event_id)
);

CREATE TABLE IF NOT EXISTS upload_files (
  resource_id TEXT PRIMARY KEY,
  room_id INTEGER NOT NULL,
  actor_user_id INTEGER NOT NULL,
  filename TEXT NOT NULL,
  content_type TEXT,
  byte_len INTEGER NOT NULL,
  path TEXT NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_upload_files_actor_created
ON upload_files(actor_user_id, created_at, resource_id);

CREATE TABLE IF NOT EXISTS durable_mutation_results (
  identity_hash BLOB NOT NULL,
  client_instance_id BLOB NOT NULL CHECK(length(client_instance_id) = 16),
  mutation_id BLOB NOT NULL CHECK(length(mutation_id) = 16),
  request_hash BLOB NOT NULL CHECK(length(request_hash) = 32),
  result_frame BLOB NOT NULL,
  retained_bytes INTEGER NOT NULL CHECK(retained_bytes >= 0),
  created_at INTEGER NOT NULL,
  last_seen_at INTEGER NOT NULL,
  PRIMARY KEY(identity_hash, client_instance_id, mutation_id)
);

CREATE INDEX IF NOT EXISTS idx_durable_mutation_results_created
ON durable_mutation_results(
  created_at,
  identity_hash,
  client_instance_id,
  mutation_id
);

CREATE TABLE IF NOT EXISTS durable_mutation_clients (
  identity_hash BLOB NOT NULL,
  client_instance_id BLOB NOT NULL CHECK(length(client_instance_id) = 16),
  first_seen_at INTEGER NOT NULL,
  last_seen_at INTEGER NOT NULL,
  retired_at INTEGER,
  PRIMARY KEY(identity_hash, client_instance_id)
);

CREATE INDEX IF NOT EXISTS idx_durable_mutation_clients_retired
ON durable_mutation_clients(retired_at, identity_hash, client_instance_id);

CREATE TABLE IF NOT EXISTS room_members (
  room_id INTEGER NOT NULL,
  user_id INTEGER NOT NULL,
  role_bits INTEGER NOT NULL DEFAULT 0,
  status_bits INTEGER NOT NULL DEFAULT 0,
  joined_at INTEGER NOT NULL,
  last_seen_at INTEGER NOT NULL,
  PRIMARY KEY(room_id, user_id)
);

CREATE TABLE IF NOT EXISTS audit_log (
  audit_id INTEGER PRIMARY KEY AUTOINCREMENT,
  at INTEGER NOT NULL,
  actor_user_id INTEGER,
  action_kind INTEGER NOT NULL,
  room_id INTEGER,
  target_user_id INTEGER,
  payload TEXT
);
