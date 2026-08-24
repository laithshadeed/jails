-- Forward-only migration, generated from the field spec.
create table refunds (
  id            uuid        not null,
  merchant_id   uuid        not null,
  payment_id    uuid        not null,
  amount_minor  bigint      not null check (amount_minor > 0),
  reason        text,
  created_at    timestamptz not null,
  updated_at    timestamptz not null,

  constraint refunds_pk
    primary key (id)
);

create index refunds_merchant_id_idx
  on refunds (merchant_id);

create index refunds_payment_id_idx
  on refunds (payment_id);

create index refunds_idx1
  on refunds (payment_id, created_at);
