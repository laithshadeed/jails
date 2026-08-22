-- Forward-only migration generated for a new record component.
alter table notes
add column created_at timestamptz default current_timestamp not null;

-- The default only backfilled rows that pre-date this field.
alter table notes
alter column created_at drop default;
