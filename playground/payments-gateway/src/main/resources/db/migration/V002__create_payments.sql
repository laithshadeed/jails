-- Forward-only migration, generated from the field spec.
create table payments (
  id               uuid        not null,
  merchant_id      uuid        not null,
  idempotency_key  text        not null unique,
  amount_minor     bigint      not null check (amount_minor > 0),
  currency         text        not null,
  method           text        not null,
  status           text        not null,
  version          bigint      not null check (version >= 0),
  authorised_at    timestamptz,
  captured_at      timestamptz,
  created_at       timestamptz not null,
  updated_at       timestamptz not null,

  constraint payments_pk
    primary key (id)
);

create index payments_merchant_id_idx
  on payments (merchant_id);

create index payments_status_idx
  on payments (status);

create index payments_idx1
  on payments (merchant_id, created_at desc);

create index payments_idx2
  on payments (status, created_at);
