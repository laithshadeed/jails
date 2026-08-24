-- Forward-only migration, generated from the field spec.
create table conversation_assignments (
  id               uuid        not null,
  workspace_id     uuid        not null,
  conversation_id  uuid        not null unique,
  member_id        uuid        not null,
  status           text        not null,
  version          bigint      not null check (version >= 0),
  assigned_at      timestamptz not null,
  created_at       timestamptz not null,
  updated_at       timestamptz not null,

  constraint conversation_assignments_pk
    primary key (id)
);

create index conversation_assignments_workspace_id_idx
  on conversation_assignments (workspace_id);

create index conversation_assignments_conversation_id_idx
  on conversation_assignments (conversation_id);

create index conversation_assignments_member_id_idx
  on conversation_assignments (member_id);

create index conversation_assignments_status_idx
  on conversation_assignments (status);

create index conversation_assignments_idx1
  on conversation_assignments (workspace_id, member_id, status);
