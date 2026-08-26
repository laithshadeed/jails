-- Two deliberate choices, stated because neither is the obvious one.
--
-- `deferrable initially deferred`: the check happens at commit rather
-- than at the statement, so a transaction may insert a child before its
-- parent and a batch may load in any order. What it costs is where the
-- error surfaces -- at `commit`, naming this constraint rather than the
-- statement that broke it. Say `not deferrable` here if you would rather
-- pay for insert order and get the statement back.
--
-- `on delete no action`: deleting a parent row is a decision about the
-- child rows, and jails cannot see enough of this domain to make it.
-- `cascade` deletes them and `restrict` refuses the delete outright --
-- note `restrict` is never deferred, so choosing it also gives up the
-- line above.
alter table items
  add constraint items_item_owner_fk
  foreign key (owner_id) references owners (id)
  on update no action on delete no action
  deferrable initially deferred;
