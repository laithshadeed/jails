-- Forward-only migration, generated from the field spec.
create table tickets (
  id       bigint generated always as identity not null,
  subject  text   not null check (length(btrim(subject)) > 0),

  constraint tickets_pk
    primary key (id)
);
