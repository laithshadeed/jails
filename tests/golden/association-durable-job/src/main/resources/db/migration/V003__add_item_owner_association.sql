alter table items
  add constraint items_item_owner_fk
  foreign key (owner_id) references owners (id)
  on update no action on delete no action
  deferrable initially deferred;
