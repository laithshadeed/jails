-- Forward-only migration, generated from the field spec.
create table owners (
  id          uuid        not null,
  name        text        not null,
  created_at  timestamptz not null,

  constraint owners_pk
    primary key (id)
);
