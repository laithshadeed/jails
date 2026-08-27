-- Forward-only migration, generated from the field spec.
create table authors (
  id     bigint generated always as identity not null,
  email  text   not null check (length(trim(email)) > 0),

  constraint authors_pk
    primary key (id)
);

-- Unique regardless of case: `A@b.com` and `a@b.com` are one account.
create unique index authors_email_key
  on authors (lower(email));
