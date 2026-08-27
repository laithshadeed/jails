-- Forward-only migration, generated from the field spec.
create table tickets (
  id        bigint generated always as identity not null,
  status    text   not null check (length(trim(status)) > 0),
  category  text,

  constraint tickets_pk
    primary key (id)
);
