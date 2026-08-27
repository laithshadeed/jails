-- Forward-only migration, generated from the field spec.
create table topics (
  id       bigint generated always as identity not null,
  user_id  bigint not null,
  subject  text   not null check (length(trim(subject)) > 0),
  version  bigint not null,

  constraint topics_pk
    primary key (id)
);
