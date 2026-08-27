-- Forward-only migration, generated from the field spec.
create table notes (
  id           bigint generated always as identity not null,
  author_id    bigint not null,
  body         text   not null check (length(trim(body)) > 0),
  sender_type  text   not null,

  constraint notes_sender_type_allowed
    check (sender_type in ('CUSTOMER', 'ADMIN')),

  constraint notes_pk
    primary key (id)
);
