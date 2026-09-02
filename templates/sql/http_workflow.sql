create table {{table}}_runs (
  id uuid primary key,
  seed_url text not null,
  origin_scheme text not null,
  origin_host text not null,
  origin_port integer not null,
  status text not null check (status in ('QUEUED','RUNNING','SUCCEEDED','FAILED','CANCELLED')),
  max_pages integer not null check (max_pages > 0),
  max_depth integer not null check (max_depth >= 0),
  pages_visited integer not null default 0 check (pages_visited >= 0),
  robots_rules text,
  cancel_requested boolean not null default false,
  last_error text,
  created_at timestamptz not null,
  started_at timestamptz,
  finished_at timestamptz
);

create table {{table}}_frontier (
  run_id uuid not null references {{table}}_runs(id) on delete cascade,
  url text not null,
  depth integer not null check (depth >= -1),
  kind text not null check (kind in ('POLICY','PAGE')),
  state text not null check (state in ('PENDING','RUNNING','SUCCEEDED','FAILED','CANCELLED')),
  attempts integer not null default 0 check (attempts >= 0),
  max_attempts integer not null check (max_attempts > 0),
  next_attempt_at timestamptz not null,
  lease_until timestamptz,
  last_error text,
  primary key (run_id, url)
);

create index {{table}}_frontier_runnable_idx
  on {{table}}_frontier (state, next_attempt_at)
  where state in ('PENDING','RUNNING');

create table {{table}}_pages (
  run_id uuid not null references {{table}}_runs(id) on delete cascade,
  url text not null,
  depth integer not null check (depth >= 0),
  status_code integer not null,
  content_type text not null,
  discovered_at timestamptz not null,
  primary key (run_id, url)
);
