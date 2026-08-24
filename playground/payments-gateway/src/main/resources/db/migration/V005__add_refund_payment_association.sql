alter table refunds
  add constraint refunds_refund_payment_fk
  foreign key (payment_id) references payments (id)
  on update no action on delete no action
  deferrable initially deferred;
