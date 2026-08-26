-- Forward-only migration, generated from the field spec.
create table owners (
  id          uuid        not null,
  name        text        not null check (length(btrim(name)) > 0),
  created_at  timestamptz not null,

  constraint owners_pk
    primary key (id)
);
