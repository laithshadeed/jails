-- Forward-only migration, generated from the field spec.
create table workspaces (
  id          uuid        not null,
  name        text        not null unique,
  created_at  timestamptz not null,
  updated_at  timestamptz not null,

  constraint workspaces_pk
    primary key (id)
);
