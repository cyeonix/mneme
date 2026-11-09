create table if not exists operation_events (
  sequence integer primary key autoincrement,
  event_id text not null unique,
  run_id text not null,
  occurred_at text not null,
  kind_type text not null,
  payload_json text not null
);

create index if not exists idx_operation_events_run_id_sequence
  on operation_events(run_id, sequence);

create table if not exists runs (
  run_id text primary key,
  status text not null,
  instruction text not null,
  started_at text not null,
  completed_at text,
  failed_at text,
  summary text,
  error text,
  updated_at text not null
);

create index if not exists idx_runs_updated_at
  on runs(updated_at desc);
