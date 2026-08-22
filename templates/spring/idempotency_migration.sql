-- Receipts for at-most-once execution with a replayable result.
--
-- The primary key is (scope, idempotency_key): scoped, because keys from two
-- callers must not collide, and one global namespace is how they do.
--
-- `response_body` is null while the first attempt is still running. That is
-- what lets a concurrent retry be told to wait rather than handed a
-- half-written answer, and it is why the column is nullable rather than
-- defaulted to the empty string.
create table {{table}} (
  scope text not null,
  idempotency_key text not null,
  request_hash text not null,
  status integer not null,
  response_body text,
  created_at timestamptz not null default now(),
  constraint {{table}}_pk
    primary key (scope, idempotency_key)
);

-- Receipts are not kept forever; this is the index a retention job scans.
create index {{table}}_created_at_idx
  on {{table}} (created_at);
