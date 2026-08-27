-- Presence, shared: one row per (scope, member, node) while that node
-- believes the member is connected. A member seen by any node is
-- present, which is the answer a single process's memory cannot give.
create table room_presence (
  scope text not null check (length(btrim(scope)) > 0),
  member text not null check (length(btrim(member)) > 0),
  node text not null check (length(btrim(node)) > 0),
  seen_at timestamptz not null,
  primary key (scope, member, node)
);

-- The sweep deletes by age across every scope, and `present` reads one
-- scope by age. Both are this index.
create index room_presence_seen_at_idx on room_presence (seen_at);
