-- Durable, leased, at-least-once work.
create table crawl_dispatcher_jobs (
  id uuid not null,
  seed_url text not null,
state text not null check (state in ('PENDING', 'RUNNING', 'SUCCEEDED', 'FAILED')),
attempts integer not null check (attempts >= 0),
max_attempts integer not null check (max_attempts > 0),
next_attempt_at timestamptz not null,
lease_until timestamptz,
last_error text,
created_at timestamptz not null,
completed_at timestamptz,
constraint crawl_dispatcher_jobs_pk primary key (id)
);

create index crawl_dispatcher_jobs_runnable_idx
on crawl_dispatcher_jobs (state, next_attempt_at)
where state in ('PENDING', 'RUNNING');
