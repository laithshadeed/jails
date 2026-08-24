-- Transactional outbox: business writes and event staging share one commit.
create table authorise_payment_outbox (
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

create index authorise_payment_outbox_runnable_idx on authorise_payment_outbox (state, next_attempt_at)
where state in ('PENDING', 'RUNNING');
