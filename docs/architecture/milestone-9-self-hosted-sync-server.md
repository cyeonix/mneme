# Milestone 9: Self-hosted Multi-device Sync Server

## Scope

Milestone 9 adds a self-hosted `mneme-server` boundary for personal multi-device sync. The server stores registered devices, per-device sync cursors, append-only synced Operation Ledger events, LWW object metadata projections, backup manifests, and remote overview counts in its own SQLite database.

The milestone also adds reusable sync domain records in `mneme-kernel` and a `mneme-sync` HTTP client for register/push/pull/ack event flows. Mneme remains local-first: each device keeps its local daemon/kernel data plane, while the self-hosted server is a rendezvous point for syncing records between devices.

## Device Registration

Devices register with `POST /api/v1/devices/register` using a display name and optional public key fingerprint. Registration creates a `DeviceRecord`, initializes a `SyncCursorRecord` at server sequence `0`, and returns a device token once to the caller.

Authenticated sync requests include `device_id` and `device_token`. The M9 token check is a server-side boundary for personal self-hosting, not a multi-user account system. Device tokens are not written to Operation Ledger, local SQLite projections, frontend storage, or docs examples.

Documentation and examples must use literal sample values such as `<device-id>` and `<device-token>` when a token-shaped value is needed.

## Append-only Event Sync

Operation Ledger sync is append-only. Devices push `SyncedEventRecord` batches to `POST /api/v1/sync/push-events`; the server inserts unseen event ids into `synced_events` and reports accepted and duplicate counts. Events are never rewritten in place.

Devices pull events from `POST /api/v1/sync/pull-events` by sending their last observed server sequence. Results are ordered by ascending `server_sequence` and capped by request limit. After applying pulled events locally, devices acknowledge progress with `POST /api/v1/sync/ack-events`, which monotonically advances their cursor.

## Object Metadata Sync and LWW Projections

M9 also syncs visible object metadata through `POST /api/v1/sync/push-objects` and `POST /api/v1/sync/pull-objects`. These records are projections for user-visible objects such as runs, artifacts, context bundles, knowledge sources, document chunks, memories, retrieval traces, worker jobs, and tool calls.

Visible object conflicts use LWW projections with updated_at and origin_device_id. When two records share the same `(object_kind, object_id)`, the server keeps the record with the later `updated_at`; if timestamps tie, the lexicographically larger `origin_device_id` wins. This is a deterministic projection rule and does not change the append-only event sync rule.

## Backup and Restore

`POST /api/v1/backups/export` writes a `BackupRecord` and returns a JSON payload containing devices, cursors, synced events, and synced objects. The backup record includes event count, object count, creation time, exporting device id, and payload SHA-256.

`POST /api/v1/backups/import` accepts a previously exported payload, upserts devices and cursors, preserves append-only event ids with duplicate-safe inserts, applies the same LWW object rule for objects, and records a new backup import manifest. `GET /api/v1/backups` lists recorded backup manifests without exposing device tokens.

## Remote Overview

`GET /api/v1/server/overview` returns remote state counts and recency metadata: device count, synced event count, synced object count, latest server sequence, latest event receive time, latest backup creation time, and backup records. This is a remote overview of the self-hosted sync server, not a remote Workbench console.

## Authentication Boundary

M9 authentication is scoped to device-token checks on sync routes. Registration returns the token once; subsequent push, pull, and ack requests must present the matching token for the device id. Unauthorized token checks return an HTTP unauthorized response.

Device tokens are transport credentials. They must be redacted from debug output and must not appear in documentation examples, Operation Ledger payloads, local projections, or frontend storage.

## Non-goals

M9 does not implement CRDTs, OAuth, mobile apps, official cloud hosting, or end-to-end encrypted sync.

M9 also does not implement a remote Web console, multi-user account management, final production token rotation, public hosted infrastructure, conflict-free text/document merging, streaming token UI, tool execution, or a full Agent Runtime.

## Verification

Required verification for this milestone documentation refresh:

```bash
git diff --check
python3 - <<'PY'
from pathlib import Path
paths=[Path('README.md'), *Path('docs/architecture').glob('*.md')]
needles=['TB'+'D','TO'+'DO','place'+'holder','FIX'+'ME','?'+'?']
hits=[]
for path in paths:
    text=path.read_text()
    for needle in needles:
        if needle in text:
            hits.append((str(path), needle))
print('docs scan hits:', hits)
raise SystemExit(1 if hits else 0)
PY
pnpm --dir apps/desktop test -- src/app/App.test.tsx
. "$HOME/.cargo/env" && pnpm check
git status --short
```

Server and sync crate focused checks are:

```bash
. "$HOME/.cargo/env" && cargo test -p mneme-server
. "$HOME/.cargo/env" && cargo test -p mneme-sync
```

## Phase 1 Closeout

With M9, Phase 1 spans the local Kernel, daemon/API, Workbench, Tauri desktop shell, and self-hosted sync server. The foundation now covers local-first run and memory records, desktop provider configuration, syncable event/object metadata, backup/restore, and a reusable HTTP sync client while preserving explicit non-goals for later milestones.
