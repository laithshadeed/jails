-- Forward-only migration, generated from the field spec.
create table articles (
  id     uuid not null,
  title  text not null check (length(trim(title)) > 0),
  body   text not null,

  constraint articles_pk
    primary key (id)
);
