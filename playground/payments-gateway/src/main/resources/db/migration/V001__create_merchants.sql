-- Forward-only migration, generated from the field spec.
create table merchants (
  id            uuid        not null,
  reference     text        not null unique,
  display_name  text        not null,
  created_at    timestamptz not null,
  updated_at    timestamptz not null,

  constraint merchants_pk
    primary key (id)
);
