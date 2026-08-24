alter table contacts
  add constraint contacts_contact_workspace_fk
  foreign key (workspace_id) references workspaces (id)
  on update no action on delete no action
  deferrable initially deferred;
