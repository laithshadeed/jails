create unique index if not exists conversations_workspace_id_id_association_key
  on conversations (workspace_id, id);

alter table messages
  add constraint messages_message_conversation_fk
  foreign key (workspace_id, conversation_id) references conversations (workspace_id, id)
  on update no action on delete no action
  deferrable initially deferred;
