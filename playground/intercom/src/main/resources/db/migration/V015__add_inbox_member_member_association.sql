create unique index if not exists members_workspace_id_id_association_key
  on members (workspace_id, id);

alter table inbox_members
  add constraint inbox_members_inbox_member_member_fk
  foreign key (workspace_id, member_id) references members (workspace_id, id)
  on update no action on delete no action
  deferrable initially deferred;
