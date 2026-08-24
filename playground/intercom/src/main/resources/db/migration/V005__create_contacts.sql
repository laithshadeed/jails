-- Forward-only migration, generated from the field spec.
create table contacts (
  id            uuid        not null,
  workspace_id  uuid        not null,
  email         text        not null,
  display_name  text,
  created_at    timestamptz not null,
  updated_at    timestamptz not null,

  constraint contacts_pk
    primary key (id)
);

create index contacts_workspace_id_idx
  on contacts (workspace_id);

create index contacts_idx1
  on contacts (workspace_id, email);
