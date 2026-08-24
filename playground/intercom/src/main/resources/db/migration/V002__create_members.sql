-- Forward-only migration, generated from the field spec.
create table members (
  id            uuid        not null,
  workspace_id  uuid        not null,
  email         text        not null,
  display_name  text        not null,
  role          text        not null,
  created_at    timestamptz not null,
  updated_at    timestamptz not null,

  constraint members_pk
    primary key (id)
);

create index members_workspace_id_idx
  on members (workspace_id);

create index members_role_idx
  on members (role);

create index members_idx1
  on members (workspace_id, email);
