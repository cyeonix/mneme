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

create table if not exists memory_records (
  memory_id text primary key,
  knowledge_source_id text,
  run_id text not null,
  kind text not null,
  title text not null,
  body text not null,
  confidence real not null,
  created_at text not null
);

create index if not exists idx_memory_records_source_created_at
  on memory_records(knowledge_source_id, created_at);

create table if not exists retrieval_traces (
  retrieval_trace_id text primary key,
  run_id text not null,
  query text not null,
  result_chunk_ids_json text not null,
  result_memory_ids_json text not null,
  explanation text not null,
  created_at text not null
);

create index if not exists idx_retrieval_traces_run_id_created_at
  on retrieval_traces(run_id, created_at);

create table if not exists worker_jobs (
  worker_job_id text primary key,
  worker_id text not null,
  run_id text not null,
  kind text not null,
  status text not null,
  model text,
  error text,
  started_at text not null,
  completed_at text
);

create index if not exists idx_worker_jobs_run_id_started_at
  on worker_jobs(run_id, started_at);
