-- Forward-only migration, generated from the field spec.
create table crawled_pages (
  id             uuid        not null,
  crawl_run_id   uuid        not null,
  url            text        not null,
  status_code    integer     not null,
  discovered_at  timestamptz not null,
  created_at     timestamptz not null,
  updated_at     timestamptz not null,

  constraint crawled_pages_pk
    primary key (id)
);

create index crawled_pages_crawl_run_id_idx
  on crawled_pages (crawl_run_id);

create index crawled_pages_idx1
  on crawled_pages (crawl_run_id, discovered_at desc);
