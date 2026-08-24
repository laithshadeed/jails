-- Forward-only migration, generated from the field spec.
create table messages (
  id               uuid        not null,
  workspace_id     uuid        not null,
  conversation_id  uuid        not null,
  direction        text        not null,
  body             text        not null,
  created_at       timestamptz not null,
  updated_at       timestamptz not null,

  constraint messages_pk
    primary key (id)
);

create index messages_workspace_id_idx
  on messages (workspace_id);

create index messages_conversation_id_idx
  on messages (conversation_id);

create index messages_idx1
  on messages (conversation_id, created_at);
