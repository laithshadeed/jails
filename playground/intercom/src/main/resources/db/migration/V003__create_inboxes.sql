-- Forward-only migration, generated from the field spec.
create table inboxes (
  id            uuid        not null,
  workspace_id  uuid        not null,
  name          text        not null,
  channel       text        not null,
  created_at    timestamptz not null,
  updated_at    timestamptz not null,

  constraint inboxes_pk
    primary key (id)
);

create index inboxes_workspace_id_idx
  on inboxes (workspace_id);

create index inboxes_channel_idx
  on inboxes (channel);

create index inboxes_idx1
  on inboxes (workspace_id, name);
