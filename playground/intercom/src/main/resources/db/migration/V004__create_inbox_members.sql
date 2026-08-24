-- Forward-only migration, generated from the field spec.
create table inbox_members (
  id            uuid        not null,
  workspace_id  uuid        not null,
  inbox_id      uuid        not null,
  member_id     uuid        not null,
  created_at    timestamptz not null,
  updated_at    timestamptz not null,

  constraint inbox_members_pk
    primary key (id)
);

create index inbox_members_workspace_id_idx
  on inbox_members (workspace_id);

create index inbox_members_inbox_id_idx
  on inbox_members (inbox_id);

create index inbox_members_member_id_idx
  on inbox_members (member_id);

create index inbox_members_idx1
  on inbox_members (workspace_id, inbox_id);

create index inbox_members_idx2
  on inbox_members (workspace_id, member_id);
