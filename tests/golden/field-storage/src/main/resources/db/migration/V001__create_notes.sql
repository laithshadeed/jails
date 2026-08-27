-- Forward-only migration, generated from the field spec.
create table notes (
  id     uuid not null,
  title  text not null check (length(trim(title)) > 0),

  constraint notes_pk
    primary key (id)
);
