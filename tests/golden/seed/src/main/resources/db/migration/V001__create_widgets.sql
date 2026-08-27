-- Forward-only migration, generated from the field spec.
create table widgets (
  id    uuid not null,
  name  text not null check (length(btrim(name)) > 0),

  constraint widgets_pk
    primary key (id)
);
