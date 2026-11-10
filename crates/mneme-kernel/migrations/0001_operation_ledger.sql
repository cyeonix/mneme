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

create table if not exists artifacts (
  artifact_id text primary key,
  run_id text not null,
  title text not null,
  kind text not null,
  mime_type text not null,
  sha256 text not null,
  byte_len integer not null,
  created_at text not null
);

create index if not exists idx_artifacts_run_id_created_at
  on artifacts(run_id, created_at desc);

create table if not exists context_bundles (
  context_bundle_id text primary key,
  run_id text not null,
  title text not null,
  summary text not null,
  artifact_ids text not null,
  event_ids text not null,
  created_at text not null
);

create index if not exists idx_context_bundles_run_id_created_at
  on context_bundles(run_id, created_at desc);

create table if not exists knowledge_sources (
  knowledge_source_id text primary key,
  run_id text not null,
  source_kind text not null,
  artifact_id text,
  context_bundle_id text,
  title text not null,
  summary text not null,
  created_at text not null
);

create index if not exists idx_knowledge_sources_run_id_created_at
  on knowledge_sources(run_id, created_at);

create table if not exists document_chunks (
  document_chunk_id text primary key,
  knowledge_source_id text not null,
  run_id text not null,
  sequence integer not null,
  text text not null,
  char_start integer not null,
  char_end integer not null,
  created_at text not null
);

create index if not exists idx_document_chunks_source_sequence
  on document_chunks(knowledge_source_id, sequence);
