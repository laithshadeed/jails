-- Full-text search over {{table}}, as a generated column.
--
-- `generated always as (...) stored`, not a trigger. A trigger is the older
-- recipe and it has one silent failure: somebody adds an UPDATE path that
-- forgets it, the row's text changes, the tsvector does not, and the row stops
-- matching a search it used to match. Nothing errors. A generated column
-- cannot drift from its inputs because PostgreSQL maintains it.
--
-- `coalesce(x, '')` around every column is not defensive noise: `||` with a
-- NULL operand yields NULL, so one null column would blank the whole vector
-- and the row would match nothing at all.
--
-- The text search configuration is named here rather than left to
-- `default_text_search_config`, so the stemming a row was indexed under does
-- not change when a session or a server setting does.
alter table {{table}}
    add column {{column}} tsvector
    generated always as ({{expression}}) stored;

-- GIN, not GiST: GIN is slower to build and faster to search, which is the
-- right trade for a column written once per row change and read on every query.
create index {{table}}_{{column}}_idx on {{table}} using gin ({{column}});
