alter table inboxes
  add constraint inboxes_inbox_workspace_fk
  foreign key (workspace_id) references workspaces (id)
  on update no action on delete no action
  deferrable initially deferred;
