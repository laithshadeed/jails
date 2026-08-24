alter table inbox_members
  add constraint inbox_members_inbox_member_inbox_fk
  foreign key (workspace_id, inbox_id) references inboxes (workspace_id, id)
  on update no action on delete no action
  deferrable initially deferred;
