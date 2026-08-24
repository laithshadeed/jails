-- Forward-only migration, generated from the field spec.
create table crawl_runs (
  id             uuid        not null,
  seed_url       text        not null,
  status         text        not null,
  pages_visited  bigint      not null check (pages_visited >= 0),
  started_at     timestamptz,
  finished_at    timestamptz,
  created_at     timestamptz not null,
  updated_at     timestamptz not null,

  constraint crawl_runs_pk
    primary key (id)
);

create index crawl_runs_status_idx
  on crawl_runs (status);

create index crawl_runs_idx1
  on crawl_runs (status, id);
