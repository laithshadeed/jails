alter table conversation_assignments
  add constraint conversation_assignments_assignment_conversation_fk
  foreign key (workspace_id, conversation_id) references conversations (workspace_id, id)
  on update no action on delete no action
  deferrable initially deferred;
