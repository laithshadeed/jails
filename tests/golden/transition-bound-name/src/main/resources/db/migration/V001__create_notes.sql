-- Forward-only migration, generated from the field spec.
create table notes (
  id       bigint  generated always as identity not null,
  body     text    not null check (length(trim(body)) > 0),
  seen     boolean not null,
  version  bigint  not null,

  constraint notes_pk
    primary key (id)
);
