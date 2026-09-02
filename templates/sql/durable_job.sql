-- Durable work queue: the item survives the process that accepted it.
--
-- The payload is one JSON column rather than a column per command field. A
-- work queue is this application's own bookkeeping, not the reader's schema,
-- so nothing queries it by payload field -- and a column per field would make
-- every field added to the command a migration for work that has not run yet.
create table {{table}} (
  id uuid primary key,
  payload jsonb not null,
  state text not null check (state in ('PENDING', 'RUNNING', 'SUCCEEDED', 'FAILED')),
  attempts integer not null check (attempts >= 0),
  max_attempts integer not null check (max_attempts > 0),
  next_attempt_at timestamptz not null,
  lease_until timestamptz,
  last_error text,
  created_at timestamptz not null,
  completed_at timestamptz
);

-- The worker claims the oldest runnable item; nothing reads a terminal one
-- again.
create index {{table}}_runnable_idx on {{table}} (state, next_attempt_at)
  where state in ('PENDING', 'RUNNING');
