create table if not exists devices (
  device_id text primary key,
  display_name text not null,
  public_key_fingerprint text,
  registered_at text not null,
  last_seen_at text not null
);

create table if not exists sync_cursors (
  device_id text primary key not null references devices(device_id) on delete cascade,
  last_server_sequence integer not null,
  updated_at text not null
);

create table if not exists synced_events (
  server_sequence integer primary key autoincrement,
  event_id text not null unique,
  origin_device_id text not null references devices(device_id),
  run_id text not null,
  occurred_at text not null,
  kind_type text not null,
  payload_json text not null,
  received_at text not null
);

create index if not exists idx_synced_events_origin_device_sequence
  on synced_events(origin_device_id, server_sequence);

create table if not exists synced_objects (
  object_kind text not null,
  object_id text not null,
  origin_device_id text not null references devices(device_id),
  run_id text not null,
  updated_at text not null,
  payload_json text not null,
  received_at text not null,
  primary key (object_kind, object_id)
);

create index if not exists idx_synced_objects_run_kind_updated
  on synced_objects(run_id, object_kind, updated_at desc);

create table if not exists backups (
  backup_id text primary key,
  device_id text not null references devices(device_id),
  created_at text not null,
  event_count integer not null,
  object_count integer not null,
  sha256 text not null
);
