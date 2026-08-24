create unique index if not exists inboxes_workspace_id_id_association_key
  on inboxes (workspace_id, id);

alter table conversations
  add constraint conversations_conversation_inbox_fk
  foreign key (workspace_id, inbox_id) references inboxes (workspace_id, id)
  on update no action on delete no action
  deferrable initially deferred;
