-- Forward-only migration, generated from the field spec.
create table owners (
  id          uuid        not null,
  email       text        not null check (length(trim(email)) > 0),
  created_at  timestamptz not null,

  constraint owners_pk
    primary key (id)
);

-- Unique regardless of case: `A@b.com` and `a@b.com` are one account.
create unique index owners_email_key
  on owners (lower(email));
