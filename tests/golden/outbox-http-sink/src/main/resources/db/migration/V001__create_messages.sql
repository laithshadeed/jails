-- Forward-only migration, generated from the field spec.
create table messages (
  id          uuid        not null,
  body        text        not null,
  created_at  timestamptz not null,

  constraint messages_pk
    primary key (id)
);
