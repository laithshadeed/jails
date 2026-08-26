-- Forward-only migration, generated from the field spec.
create table notes (
  id     uuid not null,
  title  text not null,

  constraint notes_pk
    primary key (id)
);
