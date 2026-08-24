create unique index if not exists contacts_workspace_id_id_association_key
  on contacts (workspace_id, id);

alter table conversations
  add constraint conversations_conversation_contact_fk
  foreign key (workspace_id, contact_id) references contacts (workspace_id, id)
  on update no action on delete no action
  deferrable initially deferred;
