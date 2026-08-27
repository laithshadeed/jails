-- Forward-only migration, generated from the field spec.
create table people (
  id          uuid        not null,
  email       text        not null check (length(trim(email)) > 0),
  created_at  timestamptz not null,

  constraint people_pk
    primary key (id)
);

-- Unique regardless of case: `A@b.com` and `a@b.com` are one account.
create unique index people_email_key
  on people (lower(email));
