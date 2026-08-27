-- Forward-only migration, generated from the field spec.
create table items (
  id          uuid        not null,
  owner_id    uuid        not null,
  name        text        not null check (length(btrim(name)) > 0),
  created_at  timestamptz not null,

  constraint items_pk
    primary key (id)
);

create index items_owner_id_idx
  on items (owner_id);
