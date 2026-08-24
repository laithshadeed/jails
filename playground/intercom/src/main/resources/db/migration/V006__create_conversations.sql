-- Forward-only migration, generated from the field spec.
create table conversations (
  id               uuid        not null,
  workspace_id     uuid        not null,
  contact_id       uuid        not null,
  inbox_id         uuid        not null,
  status           text        not null,
  last_message_at  timestamptz not null,
  version          bigint      not null check (version >= 0),
  created_at       timestamptz not null,
  updated_at       timestamptz not null,

  constraint conversations_pk
    primary key (id)
);

create index conversations_workspace_id_idx
  on conversations (workspace_id);

create index conversations_contact_id_idx
  on conversations (contact_id);

create index conversations_inbox_id_idx
  on conversations (inbox_id);

create index conversations_status_idx
  on conversations (status);

create index conversations_idx1
  on conversations (workspace_id, status, last_message_at desc);
