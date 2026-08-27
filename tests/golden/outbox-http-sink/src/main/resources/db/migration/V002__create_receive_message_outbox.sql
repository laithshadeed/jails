-- Transactional outbox: business writes and event staging share one commit.
create table receive_message_outbox (
id uuid primary key,
payload jsonb not null,
state text not null check (state in ('PENDING', 'RUNNING', 'SUCCEEDED', 'FAILED')),
attempts integer not null check (attempts >= 0),
max_attempts integer not null check (max_attempts > 0),
next_attempt_at timestamptz not null,
lease_until timestamptz,
last_error text,
created_at timestamptz not null,
completed_at timestamptz,
-- Which sinks have already accepted this event. A row is only as
-- atomic as its worst sink, so a retry has to skip the ones that
-- succeeded or they see the event once per attempt.
delivered text[] not null default '{}'
);

create index receive_message_outbox_runnable_idx on receive_message_outbox (state, next_attempt_at)
where state in ('PENDING', 'RUNNING');
