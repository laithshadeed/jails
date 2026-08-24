alter table payments
  add constraint payments_payment_merchant_fk
  foreign key (merchant_id) references merchants (id)
  on update no action on delete no action
  deferrable initially deferred;
