alter table conversation_assignments
  add constraint conversation_assignments_assignment_member_fk
  foreign key (workspace_id, member_id) references members (workspace_id, id)
  on update no action on delete no action
  deferrable initially deferred;
