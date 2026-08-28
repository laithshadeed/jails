# JDL v1 — implementation specification

Status: normative design; implementation is in progress.
The complete-document example is valid JDL v1. Smaller `jdl` blocks are valid
declaration fragments in the surrounding entity/workspace context unless the
text says they show a before/after transformation.

Implementation checkpoint (2026-08-28): `jails-model` now has a version-gated
`jdl 1` frontend with a lossless UTF-8 byte-span CST, retained comments/blank
lines/CRLF, stable `JDLxxxx` syntax diagnostics, local CST span replacement,
and an idempotent encoding-level formatter. The v1 parser lowers app, cap,
dep, prop, enum, entity, local and global `use`, field, table, primary/unique
constraint, relation, eject, all four operation kinds, and all 23 closed
component kinds directly into the typed linker boundary; it contains no TOML
rendering or TOML parser call.
Operation parameters, joins, assignments, resolutions, roles, emits, routes,
bindings, event partitions, component parameters, identity-bearing variants,
symbol references, projection arguments, selector membership, ordered relation
mappings, referential actions, composite keys, cardinality, and exact component
source paths are retained as explicit linked nodes. Projection prerequisites,
relation target keys/types/nullability, and required cascade cycles fail closed.
All 23 component kinds are reachable from familiar `jails g` commands through
one typed CST/model mutation path and can be removed at component scope. The
eight kinds already supported by emitters also derive temporary `SourceUnit`
compatibility views; the typed component remains authoritative. A `cases`
component captures its reader-owned source as an exact plan input, so changing
the brief after review refuses the entire apply. Familiar entity, field,
facet, operation, cap, dependency, property, component, destroy, and rename
commands use local CST edits; `jails model fmt`, `--check`, preview, and sealed
apply all use the same exact-plan boundary. Unversioned source continues
through the pre-v1 compatibility importer. The formatter now canonicalizes
JSON string encodings, HTTP method case, explicit ascending order, and
attribute rank as well as whitespace, newline shape, and comma-safe wrapping
to the 100-column target. It orders entity member classes, separates member
and top-level declaration groups, and removes only comment-free identical
`use` selections. The complete closed field-attribute vocabulary now lowers to
typed field semantics. Explicit scope-claim/default evidence is distinguished
from compiler-derived claims and defaults; UUID/integer primary keys and
versions receive typed derived defaults. Intrinsic field/type rules, unique
scope claims, routed-scope security, managed input/update roles, and explicit
if-match/version relationships fail closed in the linker. The compact familiar
field syntax now materializes scope, numeric, mapping, default, version, and
updated facts into JDL v1, and `--timestamps` writes `now()`/`@updated` rather
than replay-dependent shorthand. Primary `storage postgres|h2|sqlite` derives
its implementation support in the linked model without a redundant source
cap. Record and PostgreSQL emitters consume numeric constraints and typed
database defaults; required versioned transitions increment `@version` and set
`@updated` in the same statement. Create-command adapters omit database-owned
defaults from inserts, generate RFC 9562 UUIDv7 values in the application,
initialize `@updated` without exposing it in the command input, and return the
complete database row. The UUIDv7 support file is itself merge-managed and has
a generate-edit-generate E2E proof. Scoped entities now produce a non-ejectable,
merge-managed `ExecutionContext` ABI. Command, query, and transition ports carry
that context; HTTP adapters derive it from authenticated claims; create adapters
bind scoped columns from it; and query/transition predicates always bind every
scope column. A generate-edit-generate E2E preserves reader additions to the ABI,
and a real Maven test exercises authenticated context construction plus JDBC
tenant binding. Broader chosen/derived prerequisite semantics, the exhaustive
CLI equivalence matrix, convention-derived output roles, and direct rich-node
consumption by every emitter remain open. Command and transition database
emitters now consume rich constant `set` assignments directly, including
constant-only transitions; lookup `resolve`, conflict, join, and rich parameter
ABI lowering are still open. This checkpoint is not the ship claim in section
20.

The key words **MUST**, **MUST NOT**, **SHOULD**, and **MAY** have their usual
RFC 2119 meanings.

## 1. Decision

JDL is the human-authored, durable desired state of a Jails application. It is
not a transcript of generator commands and it is not migration history.

The language has seven declaration families:

1. one `app` block;
2. `cap`, `dep`, and `prop` declarations;
3. `entity` and `enum` domain declarations;
4. `use` rules selecting generated projections;
5. nested `command`, `query`, `transition`, and `event` operations;
6. a generic, typed `component <kind>` form for the remaining generators; and
7. explicit `eject` ownership transfers.

Relationships and database constraints live inside their entity. Stable
identity is inline as `@id(...)`. The syntax is closed: an unknown declaration,
attribute, projection, component kind, prop key, cap kind, or default
function is an error.

This produces a small language without losing Jails' breadth:

- common application structure is expressed semantically;
- generated names and locations come from one versioned convention registry;
- repetitive projections can use `for * except ...` selectors;
- uncommon generators share one `component` grammar and a typed registry;
- CLI commands edit the same source model and submit typed patch policy when a
  storage change needs confirmation; and
- append-only SQL remains append-only SQL rather than being hidden in desired
  state.

`.jails/model.jdl` is the only root document in v1. Includes, macros, user
defined attributes, arbitrary SQL attributes, and plugin-defined component
kinds are deliberately absent.

## 2. Goals and the coverage test

The goals, in priority order, are:

1. **Generic** — every durable result produced by Jails can be represented.
2. **Simple** — a small number of forms, with one meaning per token.
3. **Concise** — conventions cover the common case; exceptional facts are
   explicit.
4. **Readable** — declarations read as a model, not as serialized CLI flags.

The author chooses domain and behavior. Jails chooses the spelling and location
of their generated projections. A valid model therefore has one canonical
managed source tree; it is not one point in a matrix of equally valid naming
configurations.

"Covers Jails" has a testable meaning. Every current `ArtifactKind` MUST map to
exactly one of:

- an entity or enum declaration;
- an entity projection selected by `use`;
- a nested operation;
- a relation;
- a `component` kind; or
- a documented non-model action such as writing an append-only migration.

Every current capability MUST map to either `app.storage` or `cap`. Every
generation modifier MUST map to durable syntax, typed transient evidence, or a
documented convention-only refusal. The complete mappings are in sections
15–17; `--package` is the sole intentional refusal in v1 managed mode.

### 2.1 What belongs in JDL

| Concern | In JDL | Reason |
|---|---:|---|
| domain types, fields, constraints, relations | yes | durable semantic state |
| generated facets and standalone components | yes | durable projections |
| commands, queries, transitions, events, HTTP bindings | yes | durable application contract |
| caps, deps, props | yes | reproducible project intent |
| reader ownership/ejection | yes | affects every later compilation |
| retained resource state | yes | required to preserve and revive storage |
| exact backfill SQL and confirmations | no | evidence for one change, captured by its exact plan |
| Flyway/ordered migration files | no | immutable, append-only history |
| `new`, `new-cli`, adopted module/layout, exact tool versions | no | workspace bootstrap or reproducibility metadata |
| platform, build system, primary storage | yes | durable project shape selected by `app` |
| `test`, `run`, `doctor`, consoles, broker tools | no | operational commands, not desired state |
| reviewed plan bundles and receipts | no | execution protocol and audit evidence |

The absence of transient change evidence from JDL is intentional. For example,
`resource field nullability ... --required --backfill-file P` edits the final
field declaration and captures `P` as an exact plan input. The emitted forward
migration is then committed. A fresh clone needs the final JDL plus that sealed
migration, not the historical CLI flag.

## 3. Prior art and deliberate choices

This design borrows narrowly from two official language specifications:

- Prisma PSL's declaration attributes, optional `?`, logical/physical
  mapping, declarative defaults, and explicit relation metadata. See
  [Prisma's PSL contract syntax](https://www.prisma.io/docs/orm/contract-authoring/psl-syntax)
  and [Prisma schema attributes](https://docs.prisma.io/docs/orm/reference/prisma-schema-reference).
- JHipster JDL's set selectors, especially `*` and `except`. See
  [the JDL introduction](https://www.jhipster.tech/jdl/intro/),
  [JHipster JDL options](https://www.jhipster.tech/jdl/options/), and
  [JHipster relationships](https://www.jhipster.tech/jdl/relationships/).

The choices are:

- `@attribute` modifies the declaration on its line.
- entity-wide storage facts are ordinary members: `table`, `pk`, `unique`,
  and `index`.
- `@id(...)` always means Jails stable identity. Database identity is `@pk`.
- `?` means nullable/optional and has no second meaning.
- `!` is not legal; non-blank text is `@notBlank`.
- `:` means "has type" in fields and typed parameters.
- `=` assigns a literal or wire value.
- a keyword followed by a value is a property, such as `java 26` or
  `limit 100`.
- commas separate list items only. Semicolons are not legal.
- paths, URLs, versions, wire values, and reader file paths are quoted strings.
- operations nest in the entity they act on.
- a relation is an explicit field-to-field mapping, not an ORM object field.
- many-to-many storage uses an explicit join entity.

JDL v1 has no `@@` token. PSL needs a distinct sigil because model-level and
field-level constructs share its attribute syntax. Inside a JDL entity, the
member keyword already identifies the construct, so a second `@` carries no
information. For example, write `unique [tenantId, title]`, not a block
attribute. This also leaves one invariant for readers and implementers: `@`
always modifies the declaration on the same line.

The source vocabulary follows one economy rule: shorten a word only when it is
frequent and its abbreviation is conventional in Java/build tooling.

| Concept | JDL word | Why |
|---|---|---|
| application | `app` | common and unambiguous |
| capability | `cap` | common Jails/CLI term |
| dependency | `dep` | standard build-tool abbreviation |
| application property | `prop` | matches Java `.properties` |
| base package | `pkg` | standard Java abbreviation |
| repository projection | `repo` | already the Jails CLI word |

Longer semantic words such as `entity`, `relation`, `transition`, `component`,
`platform`, `build`, and `storage` remain unabbreviated. JDL v1 accepts only
the canonical words in the table; it does not maintain long and short aliases.

### 3.1 Convention boundary

JDL separates decisions that change the application from choices that merely
change generated spelling:

| Author states | Jails derives |
|---|---|
| app name, base `pkg`, Java release, platform, build, storage | layer packages and source roots |
| entity, field, operation, event, and component stems | Java type names, suffixes, filenames, and imports |
| types, validation, keys, relations, and behavior | SQL names, constraint names, and adapter names |
| selected projections and optional caps | prerequisite support, tests, and build scopes |
| an external wire/HTTP/physical contract when it is genuinely fixed | every unpinned wire value, route, table, and column |

The following are normative language rules:

1. A source name is a **semantic stem**, not a generated class name. For
   example, `component service Billing` generates `BillingService`;
   `component service BillingService` is rejected with the fix `Billing`.
2. There is no naming, pluralization, suffix, source-root, layer-package, test
   naming, migration naming, or route-style configuration in JDL v1.
3. The base `app.pkg` is the only author-selected package. All managed Java is
   placed below it by the closed layer table in section 9.7.
4. A convention is part of `jdl 1`. A compiler upgrade MUST NOT silently
   change it. A changed convention requires a new JDL major version and an
   explicit source/contract migration plan.
5. Collisions are errors. Jails never appends `2`, `Impl2`, or another
   counter to make two declarations fit.

Convention removes presentation choices, not business intent. Declaring an
`entity` does not silently add persistence or CRUD; the author still selects
`repo`, `scaffold`, or another behavior. Security, delivery, caching, and other
policy-bearing caps also remain explicit. Once selected, their implementation
shape is conventional.

An explicit physical or public name is a **contract pin**, not a styling
option. The pin forms are `table "..."`, `@map`, an enum wire value after `=`, an
explicit scope claim, a projection `path:`, an operation `route` that replaces
its derived route, a component `route` that replaces a derived route, and an
explicit binding wire name. They exist for an adopted database, a pre-existing
public API, or a contract that must survive a logical rename. Plans label every
pin and show the convention it replaces. An outbound `component client` route
is different: the remote endpoint defines that component's behavior and Jails
has no local value to derive.

Introducing or changing a pin is a guarded change. Apply requires typed
`PinEvidence { owner_id, role, value, reason, observed_digest? }`, where
`reason` is exactly `adopted`, `preserved`, or `external`. Import supplies
`adopted` evidence from the observed contract; rename supplies `preserved`
evidence from the accepted model; `jails model pin` supplies reviewed
`external` evidence. Evidence belongs to the sealed plan/receipt, not the JDL.
An unchanged pin in an already accepted model needs no new evidence. The
formatter never invents a pin.

Package placement is deliberately not pinnable. An importer that encounters a
non-conventional managed package must either plan a move to the canonical
layer or eject the affected implementation boundary. It MUST NOT reproduce a
per-generator `--package` choice in JDL. This is the one current generator knob
that v1 intentionally retires in order to produce a consistent source tree.

Unlike JHipster, JDL has no open custom-annotation bag. Unlike PSL, JDL is not
database-first: Java units, adapters, HTTP contracts, caps, properties,
and ownership are all first-class.

## 4. Complete example

This example targets a Spring application built by Maven with PostgreSQL as its
primary store.

```jdl
jdl 1

app Tasks {
  pkg com.example.tasks
  java 26
  platform spring
  build maven
  storage postgres
}

cap api
cap security
cap fake

dep org.example:audit-api @version("2.4.1")
prop management.endpoints.web.exposure.include = "health,info" @target(main)

use dto for * except User

enum TaskStatus {
  OPEN
  IN_PROGRESS
  DONE
}

entity User {
  use scaffold

  id:    uuid   @pk
  email: string @notBlank @unique
}

entity Task {
  use scaffold, factory, seed
  use search(fields: [title, description])

  id:          uuid       @pk
  tenantId:    uuid       @scope
  ownerId:     uuid
  title:       string     @notBlank @length(1..200)
  description: string?
  status:      TaskStatus
  version:     long       @version @nonnegative
  createdAt:   instant    @default(now())
  updatedAt:   instant    @default(now()) @updated

  unique [tenantId, title]
  index [tenantId, status, createdAt desc]

  relation owner to User {
    map ownerId -> id
    on delete restrict
    on update restrict
  }

  command Create(title, ownerId) {
    set status = OPEN
    emit TaskCreated
  }

  query Open(status?, owner.email? as ownerEmail) {
    join User as owner on ownerId -> owner.id
    order by [createdAt desc, id]
    limit 100
  }

  transition Complete(id, version) {
    select [id]
    set status = DONE
    if-match required
    emit TaskCompleted
  }

  event TaskCreated(
    id,
    tenantId,
    status,
    occurredAt: instant @default(now())
  ) {
    partition by id
  }

  event TaskCompleted(
    id,
    tenantId,
    status,
    occurredAt: instant @default(now())
  ) {
    partition by id
  }
}

component client Audit {
  on Task
  yields Task
  route POST "/v1/audit"
}

component integration-test TaskApi

eject Task.repo.fake
```

No generated name is hidden in that source. For example, Jails derives
`domain.Task`, `app.TaskRepository`, `adapters.JdbcTaskRepository`,
`web.TaskController`, table `tasks`, column `tenant_id`, and the operation
routes shown in section 12.6. `model plan` and `model explain` expose those
effective values without forcing the author to repeat them.

The formatter may wrap a declaration between matching `(`, `[` or `{`
delimiters. A newline inside those delimiters does not terminate the
declaration.

## 5. Lexical specification

### 5.1 Encoding and line endings

- A JDL file MUST be UTF-8 without a required byte-order mark.
- The parser MUST accept LF and CRLF. The formatter MUST write LF.
- Source locations are UTF-8 byte offsets plus one-based line and Unicode
  scalar column numbers.
- The first non-comment declaration MUST be `jdl 1`.

### 5.2 Comments and whitespace

- `//` starts a comment outside a string and runs to the end of the line.
- There are no block comments in v1.
- Spaces and tabs separate tokens. Tabs are accepted but formatted as spaces.
- A physical newline becomes an `NL` token when it terminates a non-trivia
  logical line, except inside `()`, `[]`, or a quoted string. Braces do not
  suppress newlines. Blank and comment-only lines remain CST trivia and do not
  emit `NL`.
- If the final non-trivia logical line has no physical line ending, the lexer
  emits its terminating `NL` immediately before `EOF`.
- Leading comments and blank lines attach to the following declaration;
  trailing comments attach to the declaration on their line.

### 5.3 Identifiers

| Kind | Form | Example |
|---|---|---|
| type/component/app | Java UpperCamel identifier | `Task`, `TaskApi` |
| field/relation/parameter/alias | Java lowerCamel identifier | `ownerId` |
| enum constant | Java UPPER_SNAKE identifier | `IN_PROGRESS` |
| stable ID | `[a-z][a-z0-9_]{0,127}` | `fld_task_title` |
| capability/component/projection kind | lowercase kebab case | `integration-test` |
| Java package | dot-separated Java identifiers | `com.example.tasks` |
| property key | `[A-Za-z0-9_.-]+` | `server.port` |
| dependency coordinate | `[A-Za-z0-9_.-]+:[A-Za-z0-9_.-]+` | `org.example:audit-api` |

JDL source identifiers are deliberately ASCII in v1:

```text
TYPE_IDENT  = [A-Z][A-Za-z0-9]*
FIELD_IDENT = [a-z][A-Za-z0-9]*
ENUM_IDENT  = [A-Z][A-Z0-9]*(?:_[A-Z0-9]+)*
KIND        = [a-z][a-z0-9]*(?:-[a-z0-9]+)*
IDENT       = [a-z][A-Za-z0-9_-]*
PACKAGE     = [a-z][a-z0-9_]*(?:\.[a-z][a-z0-9_]*)*
PROP_KEY    = [A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]+)*
COORDINATE  = [A-Za-z0-9_.-]+:[A-Za-z0-9_.-]+
```

These categories are contextual: `OPEN` is a type token in a type position and
an enum-constant token in a value position. Generated Java and wire values may
contain Unicode even though JDL declaration names do not.

Java keywords and public top-level `java.lang` simple names for the selected
Java release MUST be rejected where a new Java type would be declared.
Keywords are lowercase and case-sensitive.

### 5.4 Strings and literals

Strings use double quotes and JSON escapes: `\"`, `\\`, `\n`, `\r`, `\t`,
and `\uXXXX`. Valid surrogate pairs decode to one scalar; isolated surrogates
and unescaped control characters are errors.

`INT` is `-?(0|[1-9][0-9]*)`. `DECIMAL` is
`-?(0|[1-9][0-9]*)\.[0-9]+`. Leading plus signs, exponent notation, numeric
separators, `NaN`, and infinities are not accepted. The parser retains exact
decimal text and defers target-range checks to the type checker.

Literals are:

```text
string | signed integer | signed decimal | true | false | enum constant
```

`null` is not a literal. Optionality is represented by `?`; absence in a
request is not the same fact as a stored null.

## 6. Concrete grammar

This EBNF is normative. `NL` is the significant newline described above.
`IDENT`, `TYPE_IDENT`, `FIELD_IDENT`, `ENUM_IDENT`, `KIND`, `INT`, `DECIMAL`,
`STRING`, `PACKAGE`, `COORDINATE`, and `PROP_KEY` are lexer tokens validated
by section 5.

```ebnf
document          ::= "jdl" INT NL app_decl top_decl* EOF ;

app_decl          ::= "app" TYPE_IDENT attributes "{" NL
                      app_property* "}" NL ;
app_property      ::= "pkg" PACKAGE NL
                    | "java" INT NL
                    | "platform" platform NL
                    | "build" build_system NL
                    | "storage" storage NL ;
platform          ::= "spring" | "plain" ;
build_system      ::= "maven" | "gradle" ;
storage           ::= "postgres" | "h2" | "sqlite" | "none" ;

top_decl          ::= cap_decl
                    | dep_decl
                    | prop_decl
                    | use_decl
                    | enum_decl
                    | entity_decl
                    | top_event_decl
                    | component_decl
                    | eject_decl ;

cap_decl          ::= "cap" KIND [TYPE_IDENT] attributes NL ;
dep_decl          ::= "dep" COORDINATE attributes NL ;
prop_decl         ::= "prop" PROP_KEY "=" literal attributes NL ;
eject_decl        ::= "eject" boundary_ref attributes NL ;

use_decl          ::= "use" projection ("," projection)*
                      ["for" selector ["except" name_list]] NL ;
projection        ::= KIND ["(" named_args ")"] ;
selector          ::= "*" | name_list ;
name_list         ::= TYPE_IDENT ("," TYPE_IDENT)* ;

enum_decl         ::= "enum" TYPE_IDENT attributes
                      (empty_block | "{" NL enum_value* "}" NL) ;
enum_value        ::= ENUM_IDENT ["=" STRING] attributes NL ;

entity_decl       ::= "entity" TYPE_IDENT attributes
                      (empty_block | "{" NL entity_member* "}" NL) ;
entity_member     ::= use_decl
                    | field_decl
                    | table_decl
                    | constraint_decl
                    | relation_decl
                    | operation_decl ;

field_decl        ::= FIELD_IDENT ":" type_ref attributes NL ;
type_ref          ::= type_atom ["?"] ;
type_atom         ::= scalar_type
                    | TYPE_IDENT
                    | "list" "<" type_ref ">"
                    | "map" "<" type_ref "," type_ref ">" ;

table_decl        ::= "table" STRING NL ;
constraint_decl   ::= "pk" field_list attributes NL
                    | "unique" field_list attributes NL
                    | "index" order_list attributes NL ;

relation_decl     ::= "relation" FIELD_IDENT "to" TYPE_IDENT attributes "{" NL
                      relation_member+ "}" NL ;
relation_member   ::= "map" field_path "->" field_path NL
                    | "on" "delete" referential_action NL
                    | "on" "update" referential_action NL ;

operation_decl    ::= operation_head
                      (NL | empty_block | "{" NL operation_member* "}" NL) ;
operation_head    ::= operation_kind TYPE_IDENT "(" [param_list] ")" attributes ;
operation_kind    ::= "command" | "query" | "transition" | "event" ;
top_event_decl    ::= "event" TYPE_IDENT "(" [typed_param_list] ")" attributes
                      (NL | empty_block | "{" NL operation_member* "}" NL) ;
param_list        ::= parameter ("," parameter)* ;
parameter         ::= field_path ["?"] ["as" FIELD_IDENT]
                    | FIELD_IDENT ":" type_ref attributes ;
typed_param_list  ::= typed_parameter ("," typed_parameter)* ;
typed_parameter   ::= FIELD_IDENT ":" type_ref attributes ;

operation_member ::= route_stmt
                    | bind_stmt
                    | set_stmt
                    | "emit" TYPE_IDENT NL
                    | "conflict" "on" field_list NL
                    | join_stmt
                    | resolve_stmt
                    | "order" "by" order_list NL
                    | "limit" INT NL
                    | "select" field_list NL
                    | "update" field_list NL
                    | "if-match" precondition NL
                    | "partition" "by" FIELD_IDENT NL ;

join_stmt         ::= "join" TYPE_IDENT ["as" FIELD_IDENT] "on"
                      join_mapping ("," join_mapping)* NL ;
join_mapping      ::= field_path "->" field_path ;
resolve_stmt      ::= "resolve" field_path "from" field_path "where"
                      field_path "=" FIELD_IDENT NL ;
set_stmt          ::= "set" field_path "=" literal NL ;

component_decl    ::= component_head
                      (NL | empty_block | "{" NL component_member* "}" NL) ;
component_head    ::= "component" KIND TYPE_IDENT
                      ["(" [typed_param_list] ")"] attributes ;
component_member  ::= "on" symbol_ref NL
                    | "yields" symbol_ref NL
                    | route_stmt
                    | bind_stmt
                    | variant_decl
                    | "source" STRING NL ;
variant_decl      ::= "variant" TYPE_IDENT
                      ["(" [typed_param_list] ")"] attributes NL ;

route_stmt        ::= "route" http_method STRING ["consumes" wire_format] NL ;
bind_stmt         ::= "bind" FIELD_IDENT "from" binding_source [STRING] NL ;

attributes        ::= attribute* ;
attribute         ::= "@" IDENT ["(" [attribute_args] ")"] ;
attribute_args    ::= positional_args | named_args ;
positional_args   ::= value ("," value)* ;
named_args        ::= IDENT ":" value ("," IDENT ":" value)* ;

value             ::= literal | IDENT | function_call | array | range ;
function_call     ::= IDENT "(" [attribute_args] ")" ;
array             ::= "[" [value ("," value)*] "]" ;
range             ::= [INT] ".." [INT] ;
literal           ::= STRING | INT | DECIMAL | ENUM_IDENT | "true" | "false" ;

field_list        ::= "[" field_path ("," field_path)* "]" ;
order_list        ::= "[" order_item ("," order_item)* "]" ;
order_item        ::= field_path ["asc" | "desc"] ;
field_path        ::= [TYPE_IDENT "."] FIELD_IDENT ("." FIELD_IDENT)* ;
symbol_ref        ::= TYPE_IDENT ("." (TYPE_IDENT | FIELD_IDENT))* ;
boundary_ref      ::= TYPE_IDENT "." IDENT ("." IDENT)*
                    | "id" "(" IDENT ")" ;
empty_block       ::= "{" "}" NL ;

http_method       ::= "GET" | "POST" | "PUT" | "PATCH" | "DELETE" ;
wire_format       ::= "json" | "form" | "none" ;
binding_source    ::= "path" | "query" | "header" | "form" | "claim" ;
precondition      ::= "required" | "optional" | "none" ;
referential_action ::= "restrict" | "cascade" | "set-null" ;
```

`scalar_type` is one of the canonical names in section 9. `KIND` is not an
open identifier in contexts such as cap, projection, and component;
the relevant registry MUST recognize it.

An empty enum, entity, operation, event, or component block may be written `{}`
on the declaration line. A relation cannot be empty, and an app still requires
all five properties. The formatter uses the multiline spelling for
non-empty blocks.

A `range` MUST have at least one bound. It is accepted only where an attribute
schema expects it, such as `@length(1..200)`, `@length(..200)`, or
`@length(1..)`. Comments and blank lines are lossless CST trivia omitted from
the grammar.

## 7. Source model, linked model, and ordering

The implementation MUST keep two representations.

### 7.1 Source tree

The source tree is a concrete syntax tree (CST). It retains:

- every token and byte span;
- comments, blank lines, and original indentation;
- whether an optional `@id` was written or derived; and
- the source order of declarations, fields, parameters, mappings, and index
  columns.

CLI edits operate on CST spans. They MUST NOT render the complete file from the
semantic model. This is what preserves comments and unrelated formatting.

### 7.2 Linked model

The linker produces a closed, typed model. Maps are keyed by stable ID, not by
Java name or generated path. At minimum it contains:

```rust
struct AppModel {
    language_version: u16,
    convention_version: u16,
    app: App,
    capabilities: BTreeMap<CapabilityId, Capability>,
    dependencies: BTreeMap<DependencyId, Dependency>,
    properties: BTreeMap<PropertyId, Property>,
    entities: BTreeMap<EntityId, Entity>,
    enums: BTreeMap<EnumId, Enum>,
    projections: BTreeMap<ProjectionId, Projection>,
    relations: BTreeMap<RelationId, Relation>,
    operations: BTreeMap<OperationId, Operation>,
    components: BTreeMap<ComponentId, Component>,
    ejections: BTreeMap<EjectionId, Ejection>,
    derived: BTreeMap<DerivedRoleKey, DerivedValue>,
}
```

For JDL v1, `convention_version` is exactly `1`; it is stored separately so a
plan cannot accidentally compare models produced by different convention
registries. Contract pins live on their typed owner nodes. `derived` contains
the inspectable records from section 18.4 and is part of the accepted-model and
plan digest.

`Operation` and `Component` MUST be tagged enums with kind-specific payloads.
They MUST NOT be maps of strings or structs where most fields are `Option`.
For example, `Query` owns joins/order/limit, while `Event` owns partitioning;
putting both in a generic property bag would move errors from parsing to code
generation.

### 7.3 Significant order

The following source order is semantic and MUST be retained:

- entity fields, because Java record component order is ABI;
- operation parameters;
- relation mappings;
- composite key, unique, and index fields; and
- order-by expressions.

Top-level declaration order, cap order, attributes on one declaration,
and separate `use` rule order are not semantic. Compilation output MUST be
deterministic for semantically equivalent orderings.

An ordered collection is represented as an ID vector plus an ID-keyed map (or
an equivalent checked structure). Iterating a `BTreeMap` is not a substitute
for field, parameter, mapping, key, or order-by source order.

## 8. Stable identity

### 8.1 Meaning

`@id(value)` is compiler identity for the declaration on its line. It never
means a primary key, HTTP identifier, Java name, wire name, table name, or
column name. Database primary keys use field `@pk` or entity member `pk`.

Stable IDs are allowed on:

- the app;
- entities, enums, enum values, and fields;
- relations and entity constraints;
- operations and components, including component variants;
- caps, deps, properties, and ejections.

An explicit ID MUST match `[a-z][a-z0-9_]{0,127}`. IDs are globally unique in
one linked model. Prefixes are conventional, not separate namespaces.

### 8.2 Derived IDs

`@id` is optional. If absent, the linker derives an ID deterministically:

| Node | Derived form |
|---|---|
| app | `app_<app>` |
| entity | `ent_<entity>` |
| enum | `enum_<enum>` |
| enum value | `ev_<enum-id>_<value>` |
| field | `fld_<entity-id>_<field>` |
| relation | `rel_<entity-id>_<relation>` |
| entity constraint | `<pk\|uq\|idx>_<entity-id>_<fields>` |
| concrete entity projection | `prj_<entity-id>_<projection>` |
| command/query/transition | `op_<entity-id>_<operation>` |
| owned/nested event | `evt_<entity-id>_<event>` |
| unowned top-level event | `evt_<event>` |
| component | `cmp_<kind>_<component>` |
| component variant | `var_<component-id>_<variant>` |
| cap | `cap_<kind>[_<instance>]` |
| dep | `dep_<group>_<artifact>` |
| prop | `prop_<target>_<property-key>` |
| ejection | `eject_<resolved-boundary-id>` |

Each substituted fragment is lower snake case. Repeated underscores collapse
and leading/trailing underscores are removed. A result longer than 128 bytes
is truncated and suffixed with the first 12 lowercase hex characters of
SHA-256 over the untruncated form.

The effective parent ID, not the parent's displayed name, is used when deriving
child IDs.

`scaffold` is a source profile, not a linked projection node. Its concrete
`repo`, `service`, and `http` members receive the projection IDs above,
regardless of whether they came from `scaffold` or separate `use` rules.

### 8.3 Rename rule

Changing a name while preserving the effective ID is a rename. Changing both
is a removal plus an addition.

When a CLI rename targets a declaration whose ID was derived, the edit MUST
materialize the old effective ID as an explicit `@id(...)` on the renamed
declaration. This keeps a new file clean without making later renames unsafe.

Stable identity does not freeze derived projection names. If a rename is meant
to change Java only, the CLI also materializes the old derived table name as a
`table "..."` member, or the old column/constraint name as a local `@map`
modifier. A renamed entity materializes its old scaffold `path` and any
affected operation `route` values so the table, columns, and public routes stay
fixed. A renamed operation similarly materializes its old derived route when
that public contract must remain fixed.

A table-only or column-only rename changes/adds the corresponding pin while
leaving the Java name unchanged. An enum-constant Java-only rename similarly
materializes its old wire value after `=`. A hand edit that preserves an ID but
changes several still-derived names is one multi-projection rename, and the
plan must show every affected Java type, wire value, path, table, column, and
route.

A hand rename without an explicit preserved ID is not guessed. If storage,
routes, or references make the apparent remove/add destructive, planning MUST
refuse and name the appropriate `jails rename` command.

### 8.4 Declaration-header attributes

The complete header-attribute matrix is closed:

| Declaration | Valid attributes |
|---|---|
| app | `@id` |
| entity | `@id`, `@retired`, `@retired(drop)` |
| enum | `@id` |
| enum value | `@id` |
| relation | `@id`, `@map` |
| command/query/transition | `@id`, `@internal` |
| event | `@id` |
| component | `@id` |
| component variant | `@id` |
| cap | section 15.1 |
| dep | section 15.2 |
| prop | section 15.3 |
| ejection | `@id` |

`@internal` suppresses the otherwise conventional HTTP adapter for one command,
query, or transition. It forbids `route` and `bind` members. This is a behavior
choice, not a naming override. Field attributes and entity storage members are
defined in sections 9.4 and 9.6. An attribute valid in one row is not
automatically valid on a child declaration.

## 9. App, types, and fields

### 9.1 App

Exactly one `app` block is required.

| Property | Values | Meaning |
|---|---|---|
| `pkg` | Java package | base package for generated Java |
| `java` | integer | Java release; at least 21 and supported by this compiler |
| `platform` | `spring`, `plain` | Spring Boot application or framework-free Java application |
| `build` | `maven`, `gradle` | managed build-document format and launcher |
| `storage` | `postgres`, `h2`, `sqlite`, `none` | primary persistence profile for entities |

The linked representation is closed:

```rust
struct App {
    id: AppId,
    name: TypeName,
    pkg: JavaPackage,
    java_release: u16,
    platform: Platform,     // Spring | Plain
    build: BuildSystem,     // Maven | GradleGroovy
    storage: Storage,       // Postgres | H2 | Sqlite | None
}
```

There is no `Unknown`, free-form string, or inferred fallback in a successfully
linked model.

All five properties are required and appear exactly once. Source order is not
semantic; the formatter writes the table order. The app name and effective ID
are model identity. The workspace directory, module path, group/artifact
coordinates, and exact tool versions remain workspace facts. Managed layer
layout does not: it follows section 9.7.

There are no defaults for `platform`, `build`, or `storage`: omitting one would
make the same JDL mean something different after a compiler default changes or
when it is opened in another workspace.

`platform` and `build` are explicit because they change emitted source,
dependencies, build-file patches, and commands. They MUST NOT be inferred on
every compile. A new-project command writes the selected values. An importer
for an existing project detects them once, writes them into JDL, and then
checks the workspace against JDL on later runs.

`build maven` selects `pom.xml` and Maven launch semantics. `build gradle`
selects a Groovy `build.gradle` and Gradle wrapper/launcher semantics. JDL v1
does not mean `build.gradle.kts`; an importer that finds only Kotlin Gradle MUST
report an unsupported build language rather than record `gradle`. Exact Maven
or Gradle wrapper versions, plugin versions, and the Spring Boot version stay
in the checked-in build files and accepted plan inputs. JDL chooses the build
family, not a second copy of every build-file version.

Whenever a resolved output role is marked `integration`, build support is
derived with no JDL switch:

| Build | Canonical integration-test projection |
|---|---|
| Maven | `*IT.java` stays in `src/test/java`; Surefire excludes it and Failsafe runs it during `integration-test`/`verify` |
| Gradle | `*IT.java` lives in `src/integrationTest/java`; an `integrationTest` source set/task extends test dependencies, includes main/test outputs, and `check` depends on it |

Thus the semantic fact "this is an integration test" is shared, while Maven
versus Gradle controls the native build mechanics. Authors do not choose both a
build and an independent test layout.

An existing-project importer uses the selected module only:

| Evidence | Materialized value |
|---|---|
| one supported `pom.xml` and no supported Gradle build | `build maven` |
| one supported Groovy `build.gradle` and no Maven build | `build gradle` |
| both supported build files | error: ambiguous build ownership |
| only `build.gradle.kts` or another foreign build | error: unsupported build language |
| neither supported build file | error: no build system to adopt |

After choosing the build document, the importer records `platform spring` only
when that document declares the supported Spring Boot plugin/parent profile;
otherwise it records `platform plain`. Wrapper files alone do not select a
build, and Java imports do not select a platform. This detection runs for
adoption/upgrade, not as a fallback during normal linking.

On a fresh workspace, `app` selects the bootstrap emitters. In an existing
workspace, a disagreement between JDL and the observed platform/build is a
compatibility error before any write. A build or platform conversion is valid
desired state only through an explicit workspace-migration plan that lists all
created, replaced, and retired build files; a normal model sync MUST NOT
silently convert or overwrite the project.

`storage` is deliberately not called `dialect`. A SQL dialect is an emitter
detail: keyword quoting, DDL spelling, physical types, and vendor limits.
`storage` states the user's intent: which primary store backs entity
projections. `db` is not used as the property name because current Jails uses
`db` for one specific PostgreSQL support bundle; reusing it for H2, SQLite, and
`none` would be ambiguous. The compiler derives the SQL dialect and support
bundle:

| `storage` | Derived SQL dialect | Effective built-in support |
|---|---|---|
| `postgres` | PostgreSQL | current Jails `db` capability |
| `h2` | H2 | current Jails `h2` capability |
| `sqlite` | SQLite | bare/default current Jails `sqlite` capability |
| `none` | none | no primary database support |

These effective storage capabilities are implicit and MUST NOT be repeated as
`cap db`, `cap h2`, or, for `storage sqlite`, bare `cap sqlite`. Named SQLite
caps remain valid auxiliary stores, for example `cap sqlite AuditCache`.
`storage none` forbids stored projections, relations, SQL indexes, and
database-backed components. Compatibility between a platform, storage, and a
specific emitter is checked by the closed registries; parsing a combination
does not permit the compiler to omit unsupported output.

A plain Gradle application with no primary database is therefore:

```jdl
app Toolbox {
  pkg com.example.toolbox
  java 26
  platform plain
  build gradle
  storage none
}
```

### 9.2 Canonical scalar types

JDL accepts one canonical spelling for each logical scalar:

| JDL | Required Java | Nullable Java |
|---|---|---|
| `string` | `String` | `Optional<String>` |
| `int` | `int` | `Optional<Integer>` |
| `long` | `long` | `Optional<Long>` |
| `double` | `double` | `Optional<Double>` |
| `decimal` | `BigDecimal` | `Optional<BigDecimal>` |
| `boolean` | `boolean` | `Optional<Boolean>` |
| `uuid` | `UUID` | `Optional<UUID>` |
| `date` | `LocalDate` | `Optional<LocalDate>` |
| `datetime` | `LocalDateTime` | `Optional<LocalDateTime>` |
| `instant` | `Instant` | `Optional<Instant>` |
| `duration` | `Duration` | `Optional<Duration>` |
| `zone-id` | `ZoneId` | `Optional<ZoneId>` |
| `uri` | `URI` | `Optional<URI>` |
| `path` | `Path` | `Optional<Path>` |
| `currency` | `java.util.Currency` | `Optional<java.util.Currency>` |
| `bytes` | `byte[]` | `Optional<byte[]>` |

Lowercase names not in this table are errors. An UpperCamel type resolves to a
declared JDL enum/entity/component type or an observed reader-owned project
type. An unresolved type is an error.

`list<T>` and `map<K,V>` are valid in non-stored records and component payloads.
`T`, `K`, and `V` cannot themselves be optional in v1; optionality applies to
the collection as a whole. A map key must be `string` or an enum. Generated
constructors defensively copy required collections and reject null; an omitted
optional collection is `Optional.empty()`. A stored entity field may use only a
scalar or enum in v1; persisting collections or structured values requires a
future explicit codec and MUST NOT silently become JSON.

The storage backend owns physical SQL mappings. Every supported SQL dialect
MUST either map a logical type and all its constraints or reject it during model
checking; emitters may not fall back to text.

### 9.3 Optionality

A bare type is required and stored `NOT NULL` when persisted. A `?` type is
nullable and projects to `Optional<T>` in generated record/DTO components.

`?` on a query parameter reference is different and local to the request: it
means the filter may be absent. It does not change the entity field's storage
nullability.

`!` is a syntax error with the fix `use @notBlank`.

### 9.4 Field attributes

The field attribute vocabulary is closed.

| Attribute | Valid on | Meaning and validation |
|---|---|---|
| `@id(stable_id)` | every field | stable compiler identity |
| `@map("column")` | stored field | explicit physical column name |
| `@pk` | required scalar | single-column primary key; one per stored entity |
| `@unique` | stored scalar | single-column unique constraint |
| `@index` | stored scalar | single-column ascending index |
| `@notBlank` | required `string` | trim/check non-blank in Java and SQL |
| `@length(min..max)` | `string` | inclusive character length; either bound may be omitted |
| `@positive` | numeric scalar | value greater than zero |
| `@nonnegative` | numeric scalar | value greater than or equal to zero |
| `@scope` | stored scalar | same-named authenticated claim scopes every generated operation |
| `@scope(claim: "name")` | stored scalar | as above with an explicit claim name |
| `@version` | required `long` | optimistic-lock version, initial value zero |
| `@default(value)` | type-compatible field/input | default described in section 9.5 |
| `@updated` | required `instant` or `datetime` | set on every successful generated transition |

Field `@pk` and entity member `pk` are mutually exclusive. A `@pk` field cannot
be nullable. `@notBlank` implies required but does not replace the required
type rule. An unknown or repeated non-repeatable attribute is an error.

`@scope` is not a SQL constraint. Routed operations obtain it from the named
claim; non-HTTP ports expose it in their execution context. Database adapters
include every scope field in reads, writes, updates, and uniqueness checks.
Scope fields are required immutable `string`, `uuid`, `int`, or `long` scalars;
claim names are unique within an entity. They cannot be request-controlled,
`set`, updated, defaulted, or version fields. A routed scoped operation requires
`cap security`.

Bare `@scope` derives the claim name from the field name. Supplying
`claim: "..."` is a contract pin and follows section 3.1's evidence rule.

`@version` and `@updated` fields are compiler-managed. They cannot be command
inputs or `set`/`update` targets; the sole request-visible version value is the
transition precondition defined in section 12.4.

### 9.5 Defaults

Defaults are typed expressions from a closed registry:

| Expression | Types | Execution owner |
|---|---|---|
| literal | matching scalar/enum | database where persisted; generated constructor otherwise |
| `uuid7()` | `uuid` | app, using RFC 9562 time-ordered UUIDs |
| `identity()` | `int`, `long` primary key | database identity column |
| `now()` | `instant`, `datetime` | database, returned with the written row |
| `today()` | `date` | database, returned with the written row |

Default function names and named arguments are closed. Arbitrary
`dbgenerated(...)`, Java expressions, or SQL strings are forbidden.

If a primary key has no explicit default, Jails derives `uuid7()` for `uuid`
and `identity()` for `int`/`long`. No other default is inferred.

A generated create command can omit:

- a defaulted field;
- a database-assigned primary key;
- a scope field supplied by execution context; and
- a nullable field, which becomes null.

It MUST refuse to generate an insert when any other required field has no
source. `@default` is ongoing app semantics; a one-time backfill is
change evidence and is not written as `@default`.

### 9.6 Entity storage members

| Form | Meaning |
|---|---|
| `table "legacy_tasks"` | explicit physical table-name pin |
| `pk [a, b]` | composite primary key in declared order |
| `unique [a, b]` | composite unique constraint |
| `index [a, b desc]` | composite or ordered index |

`table` belongs to the entity itself, may occur at most once, and accepts no
attributes. It is a guarded compatibility pin under section 3.1, not a normal
naming choice. It is invalid unless the entity is database-backed or is retired
with retained storage.

Each `pk`, `unique`, or `index` declaration is a first-class constraint node
and may carry `@id(...)` and `@map("physical_constraint_name")`. No other
trailing attributes are valid. Field `@pk`, `@unique`, and `@index` are
the required canonical form for an unpinned, ascending, single-field
constraint. Use an entity constraint only for multiple fields, descending
index order, an explicit constraint ID, or a physical constraint-name pin. An
entity constraint that has one ascending field and no trailing attribute is
non-canonical and rejected with a fix to the field attribute. Declaring both
forms for the same semantic constraint is an error.

Every referenced field must belong to the entity. A field may appear only once
in one constraint. `asc` is the default direction. Every `pk` field must be
required. A stored entity must have exactly one single or composite primary key
before `scaffold` or database adapters can compile.

There is no `check` member and no free-form SQL constraint. Jails only emits
constraints it can type-check.

### 9.7 Canonical projection conventions

One closed `ConventionRegistry` derives every managed name and location. Each
rule has a stable ID (`java.layer.v1`, `java.type.v1`, `sql.name.v1`,
`http.route.v1`, `test.name.v1`, `migration.name.v1`,
`cap.prerequisite.v1`, or `build.entry.v1`) recorded in the linked model. There
is no project-level override map.

#### Packages and source roots

All packages are relative to `app.pkg`:

| Layer | Package | Source root | Owns |
|---|---|---|---|
| domain | `domain` | `src/main/java` | entities, enums, value types, events |
| app | `app` | `src/main/java` | inbound/outbound ports, repository interfaces |
| service | `service` | `src/main/java` | application orchestration |
| web | `web` | `src/main/java` | Spring HTTP controllers and wire records |
| api | `api` | `src/main/java` | shared inbound API contracts and handlers |
| messaging | `messaging` | `src/main/java` | broker publishers/listeners |
| cli | `cli` | `src/main/java` | command-line adapters |
| clients | `clients` | `src/main/java` | outbound HTTP ports/adapters |
| jobs | `jobs` | `src/main/java` | scheduled and durable work |
| adapters | `adapters` | `src/main/java` | database and other outbound adapters |
| testkit | `testkit` | `src/test/java` | factories and reusable test support |

A production type's tests use the same relative package as that type.
Unit tests and `testkit` use `src/test/java`. Integration tests use
`src/test/java` under Maven, where Failsafe selects `*IT`; under Gradle they use
`src/integrationTest/java`, owned by the conventional `integrationTest` source
set/task. Resources use `src/main/resources`, `src/test/resources`, and, for
Gradle integration tests, `src/integrationTest/resources`. Maven and Gradle
therefore project identical main packages and type names while expressing
their native integration-test lifecycle differently.

A legacy `jails.toml` layer rename is importer evidence, not an additional JDL
setting. JDL-managed output cannot begin while such a rename still targets a
managed boundary; import must plan the canonical move or eject that boundary.

#### Java types and files

A generated Java type is `prefix + semantic stem + suffix` from a closed role
entry. Authors choose only the stem. The common entity entries are:

| Selected role for entity `E` | Main type/file | Package | Generated test |
|---|---|---|---|
| implicit record | `E.java` | `domain` | `ETest.java` |
| repository port | `ERepository.java` | `app` | none |
| primary SQL adapter | `JdbcERepository.java` | `adapters` | `JdbcERepositoryIT.java` |
| fake adapter | `InMemoryERepository.java` | `adapters` | registry-owned contract tests |
| service | `EService.java` | `service` | `EServiceTest.java` |
| HTTP | `ERequest.java`, `EResponse.java`, `EController.java` | `web` | `EControllerTest.java` |
| DTO only | `ERequest.java`, `EResponse.java` | `web` | `EDtoTest.java` |
| factory | `EFactory.java` | `testkit` | none |

`http` and `dto` select the same request/response facet. If both select it,
the linker coalesces that facet by stable projection ID; two files are not
generated and neither declaration may configure a second spelling.

Operation stem `N` has these fixed families:

| Operation | Main types | Tests |
|---|---|---|
| command | `NCommand`, `NUseCase`, one of `StoringNUseCase`, `ResolvingNUseCase`, or `EnsuringNUseCase`, and `NController` | `NUseCaseTest`, `NControllerTest`, plus an `IT` when its adapter requires one |
| query | `NCriteria`, `NQuery`, `JdbcNQuery`, `NQueryController` | `JdbcNQueryIT`, `NQueryControllerTest` |
| transition | `NCommand`, `NUseCase`, `JdbcNTransition`, `NController` | `JdbcNTransitionIT`, `NControllerTest` |
| event | `NEvent` payload plus cap-selected delivery types | role-specific `Test`/`IT` |

Operation stems may not end in a terminal generated by their row: command
forbids `Command`, `UseCase`, and `Controller`; query forbids `Criteria`,
`Query`, and `QueryController`; transition forbids `Command`, `UseCase`,
`Transition`, and `Controller`; event forbids `Event`. The diagnostic removes
the longest matching terminal and updates references as one safe CST fix only
before first acceptance. An accepted declaration uses the normal rename flow,
preserves its ID, and plans every affected contract.

The command implementation prefix is semantic: `Storing` is ordinary insert,
`Resolving` is selected by `resolve`, and `Ensuring` is selected by
`conflict on`. Exactly one applies. Component suffixes are fixed in section
14.2. Every public top-level type lives in a same-named `.java` file; Jails does
not offer multi-type files or filename overrides.

Unit tests end in `Test`; tests that require the real database, broker, server,
or full application end in `IT`. Test method names are derived from the
behavioral contract, not from a user-supplied test-name template.

#### SQL names

Snake case inserts a boundary before an uppercase letter that starts a run or
ends an acronym run, then lowercases with ASCII rules. Thus `WorkItem` becomes
`work_item` and `HTTPClient` becomes `http_client`. A field column is its field
name in snake case. An entity table is the plural of its snake-case entity
name.

Pluralization applies to the final snake-case word. The irregular map is
`person→people`, `child→children`, `man→men`, `woman→women`, `foot→feet`,
`tooth→teeth`, `goose→geese`, and `mouse→mice`. The invariant words are
`equipment`, `information`, `money`, `news`, `series`, `species`, `staff`,
`audio`, `metadata`, and `data`. Otherwise `fe→ves`, a non-`ff` final `f→ves`,
`ss|x|z|ch|sh→...es`, existing final `s` is unchanged, consonant+`y→ies`, and
all other words append `s`.

Constraint candidates are `pk_<table>_<columns>`,
`uq_<table>_<columns>`, `idx_<table>_<columns>`, and
`fk_<child_table>_<relation>`. Columns keep declared order and are joined with
`_`. When a candidate exceeds the selected store's identifier limit, the
compiler keeps the longest whole-character prefix that fits, then `_` and the
first 12 lowercase hex characters of SHA-256 over the untruncated candidate.
Entity `table` and declaration-local `@map` are contract pins that replace
these SQL rules.

#### Routes, migrations, and collisions

HTTP routes use the kebab form obtained by replacing `_` in snake case with
`-`; the exact operation rules are in section 12.6. A scaffold collection is
`/<plural-entity>`, for example `/work-items`.

Flyway files use `V<next:03>__<description>.sql`, where `next` is one greater
than the highest observed numeric version. Descriptions are lower snake case:
`create_<table>`, `drop_<table>`, `add_<table>_<column>`,
`rename_<table>_<old>_to_<new>`, or
`change_<first-table>_<12-hex-change-hash>` for a compound change. The change
hash is SHA-256 over the sorted stable patch IDs and their typed before/after
values, excluding the migration version/path and plan digest, so allocation is
not circular. Authors never name managed migrations in JDL.

Every effective Java FQN, file, SQL identifier, route, and migration path is
computed before emission and shown by `model plan`. Two logical declarations
that compute the same name are an error; no registry entry may add a counter
or silently choose another package.

## 10. Enums

An enum value's Java name is its identifier. Its wire/storage value is the
quoted value after `=` or the Java name when `=` is absent. The quoted spelling
is a contract pin and follows section 3.1's evidence rule.

```jdl
enum Priority @id(enum_priority) {
  NONE = "-" @id(ev_priority_none)
  HIGH = "!" @id(ev_priority_high)
  URGENT = "!!" @id(ev_priority_urgent)
}
```

That fragment intentionally models an existing external wire protocol, so its
first accepted plan carries `external` pin evidence for the three values.

Java names and effective wire values must each be unique. Enum fields store
wire values and emit a closed SQL check where the storage backend supports it. Adding a
value appends a forward widening migration. Removing or changing a wire value
is a guarded storage/contract change and cannot be inferred as safe.

## 11. Projections and set selectors

An `entity` is a semantic structured type and implicitly has its immutable
record projection. `use` adds generated projections or applies a named profile.

### 11.1 Projection registry

| Projection | Normal arguments | Contract pin | Effect | Requires |
|---|---|---|---|---|
| `value` | none | none | value-object validation profile for the record | none |
| `repo` | none | none | repository port; adapters come from storage/caps | primary key |
| `service` | none | none | application service over the repository port | `repo` |
| `http` | none | optional `path` | CRUD HTTP contract and adapter | `repo`, `service`, `platform spring` |
| `dto` | none | none | request/response facet, mappings, contract test | `platform spring` |
| `factory` | none | none | fluent typed test-data factory | none |
| `search` | required `fields` | none | PostgreSQL full-text search projection | `storage postgres`, string fields |
| `seed` | none | none | seed file contract and seed-profile runner | `repo`, `storage postgres`, derived default `json`, `platform spring` |
| `scaffold` | none | optional `path` | fixed profile: `repo`, `service`, and `http` | primary key, `platform spring` |

The entity record is implicit, so `scaffold` does not need to name it. Its HTTP
projection selects the shared request/response facet and CRUD tests. An
additional `dto` selection coalesces that facet and adds only the DTO contract
test; it does not create differently named wire types. The primary `storage`
adds its managed adapter, `cap fake` adds in-memory/scripted adapters, and
`cap api` adds shared validation and problem responses to Spring HTTP adapters.

Projection prerequisites are checked, except for entries explicitly marked
`derived` in the registry. The fixed `scaffold` expansion supplies `repo`,
`service`, and `http`; `seed` supplies its mechanical default JSON support.
Thus `use http` without `repo` and `service` is an error, while `use scaffold`
is complete by itself. An entity is physically stored only when one of its
selected projections or components uses the primary storage adapter; `dto`,
`factory`, and `value` alone remain non-stored.

Projection arguments are named and closed. `fields` is an array of entity field
names. `path` is a quoted absolute collection route and is accepted only on
`http`/`scaffold`; it pins a real public contract and replaces the conventional
route. There is no projection package argument. An unknown argument is an
error.

For example, a ported service whose existing frontend calls
`/admin_api/invoices` may pin that contract:

```jdl
entity Invoice {
  use scaffold(path: "/admin_api/invoices")

  id: uuid @pk
}
```

A greenfield `Invoice` uses `use scaffold` and derives `/invoices`.

### 11.2 Local and set-scoped use

Inside an entity, `use` applies to that entity and MUST NOT contain `for` or
`except`:

```jdl
entity Invoice {
  use scaffold, factory
  // ...
}
```

At top level, `for` is required:

```jdl
use dto for * except AuditEntry
use factory for Invoice, Payment
```

Selectors are resolved after every entity is collected, so declaration order
does not matter and `*` includes entities declared later, including retired
entities. Membership is retained for revival, but retired entities emit no
projection. Every named entity must exist.

Projection membership is the union of all matching rules. An exclusion applies
only to its rule, so a later or separate positive rule can select the entity.
If two matching rules configure the same projection with different arguments,
linking fails and cites both spans. Identical duplicates are accepted by the
linker but removed by the formatter.

`scaffold` is a fixed versioned profile. To omit one member, list the desired
projections instead of subtracting from the profile.

## 12. Operations and HTTP bindings

`command`, `query`, and `transition` MUST be nested in their target entity.
Nesting supplies the target that the CLI currently spells `--on`.

The v1 managed emitters for those three operation kinds require an active
entity with `repo`, `platform spring`, and `storage postgres`.
They generate the canonical Spring HTTP adapter and route unless the operation
has `@internal`. An explicit `route` replaces that convention as a public
contract pin; it is not required for ordinary exposure. Another storage backend
may become valid only when its registry entry implements the same atomicity and
result ABI; SQL text fallback is forbidden.

For example, this query remains an application port/JDBC adapter with no HTTP
controller or route:

```jdl
query ForReconciliation(status?) @internal
```

An `event` may be nested or top-level. A nested event is owned/partitionable by
the enclosing entity and may use field shorthands. A top-level event is unowned
and all of its parameters are typed declarations. There is no top-level
`for Entity` spelling; ownership is expressed once, by nesting.

### 12.1 Parameters

A shorthand parameter references a linked field and inherits its type and
validation:

```jdl
command Open(subject, category) {}
```

A qualified reference selects a joined or otherwise visible field. `as`
chooses the operation/wire name:

```jdl
query ByOwner(owner.email as ownerEmail) {
  join User as owner on ownerId -> owner.id
}
```

A typed parameter declares operation-owned data:

```jdl
event Imported(id: uuid, source: string @notBlank)
```

Parameter names after aliasing must be unique. A shorthand must resolve to
exactly one visible field. Typed parameter attributes are limited to
`@default`, `@notBlank`, `@length`, `@positive`, and `@nonnegative`. Parameter
identity is its parent operation/component ID plus its effective name; parameter
renames are explicit ABI changes rather than stable-ID renames.

`?` after a shorthand is valid only on a query in v1. It means the filter is
presence-sensitive: absent skips the predicate; present compares using the
field's type. It does not mean `column IS NULL`. A nullable stored field can
therefore be a required filter, and a required stored field can be an optional
filter.

### 12.2 Command

A command creates one row of its enclosing entity. It may contain:

| Statement | Cardinality | Rule |
|---|---:|---|
| `set field = value` | repeatable | supplies a typed constant not controlled by the caller |
| `resolve ...` | repeatable | resolves a target field through a unique row lookup |
| `conflict on [fields]` | at most one | atomic get-or-create on an exact PK/unique constraint |
| `emit Event` | repeatable | publishes through the transactional outbox |
| `route ...` | at most one | pins a non-conventional HTTP contract |
| `bind ...` | repeatable | overrides one default HTTP binding |

Every required entity field must come from a parameter, `set`, `resolve`,
scope context, field default, generated primary key, or compiler-managed
version/timestamp initialization. A field may have only one source.

`conflict on` fields must exactly match one declared single/composite primary or
unique constraint and all be supplied by the command. The DB emitter uses one
atomic insert-or-read operation; a read followed by insert is forbidden.

`resolve` is the explicit form of the create-side `--via` behavior. This
example also pins the form endpoint of an existing customer frontend:

```jdl
command PostNote(Author.email as email, body) {
  resolve authorId from Author.id where Author.email = email
  set senderType = CUSTOMER
  route POST "/customer_api/notes" consumes form
}
```

The `where` field or field tuple must be unique. The resolved remote field and
the local target field must have the same logical type. The implementation may
later extend the grammar to tuple lookups, but v1 resolves one field per
statement.

### 12.3 Query

A query reads rows of its enclosing entity. Its parameters are equality
filters. It may contain:

| Statement | Cardinality | Rule |
|---|---:|---|
| `join ...` | repeatable | typed inner join for qualified filters |
| `order by [...]` | at most one | semantic field order, each `asc`/`desc` |
| `limit N` | at most one | positive ceiling, default 100 |
| `route ...` | at most one | pins a non-conventional HTTP contract |
| `bind ...` | repeatable | binding override |

Example with the complete replacement for ambiguous `--via` inference:

```jdl
query ItemsByOwnerEmail(owner.email as email) {
  join Owner as owner on ownerId -> owner.id
  order by [createdAt desc, id]
  limit 20
}
```

Join mappings are local-target to joined-field pairs. Both sides must have the
same type. The joined right-hand fields must collectively be a primary or
unique key. Aliases are required when the same entity is joined twice and must
be unique within the query.

If the declared ordering is not unique, the compiler appends every missing
primary-key field in primary-key order as a stable tiebreak. This derived order
is visible in `model plan` and the emitted contract.

### 12.4 Transition

A transition atomically updates rows of its enclosing entity. It may contain:

| Statement | Cardinality | Rule |
|---|---:|---|
| `select [fields]` | at most one | row selector; defaults to the primary key |
| `update [fields]` | at most one | parameter-backed fields to change |
| `set field = value` | repeatable | constant-backed fields to change |
| `if-match policy` | at most one | `required` (default), `optional`, or `none` |
| `emit Event` | repeatable | event published in the same transaction |
| `route ...` | at most one | pins a non-conventional HTTP contract |
| `bind ...` | repeatable | binding override |

Parameters in `select` identify the row. Parameters in `update` provide new
values. Remaining entity-field parameters are equality guards in the update's
`WHERE` clause. A parameter cannot occupy more than one role. Constant `set`
fields must not also be parameters or appear in `update`.

When `update` is omitted, all non-selector, non-version entity parameters are
updated for compatibility with the current CLI. The formatter SHOULD insert an
explicit `update` when more than one interpretation is possible.

`if-match required` requires exactly one `@version long` field and one shorthand
parameter referencing it. `optional` has the same source parameter but makes
its request value presence-sensitive: a supplied value is compared and an
absent value permits an unconditional request. `none` forbids a version
parameter and is valid only when every changed field is pinned to a constant or
when the implementation boundary is ejected. Successful versioned transitions
increment the version once and update every `@updated` field in the same SQL
statement.

The result ABI is a sealed result with applied, stale-version, and not-found
cases. HTTP adapters map `If-Match`/`ETag` and `cap api`'s problem
format from that ABI; the controller must not reimplement the state machine.

### 12.5 Event

An event owns an immutable payload. It accepts only:

- `partition by field`, at most once.

The partition field must be a required payload field. With an owning entity and
no explicit partition, the entity primary key is used when it is present in the
payload; otherwise the event ID is used. Event IDs are time-ordered UUIDs and
are never inferred from business payload equality.

When a command or transition emits an event, every required event payload field
must resolve by name from the operation parameters, entity state after the
write, scope context, or an event parameter default. Ambiguity or a missing
source is a linking error.

`storage postgres` supplies the transactional outbox. `cap kafka` supplies the
Kafka publisher and listener boundary. `component http-sink` supplies an HTTP
delivery boundary. Delivery adapters track an event ID and per-sink delivery
state; they must not turn retries into duplicate logical events.

### 12.6 Route and binding semantics

A command, query, or transition owns one HTTP binding unless it has
`@internal`. The common binding is derived; source records only a departure
from it.

Let `collection(E)` be the entity's `http`/`scaffold` `path:` pin when present,
otherwise `/<plural-kebab(E)>`. Let `op(N)` be the lower kebab form of the
operation stem. The v1 routes are:

| Projection/operation | Method and path | Consumes |
|---|---|---|
| scaffold create | `POST collection(E)` | `json` |
| scaffold list | `GET collection(E)` | `none` |
| scaffold item read | `GET item(E)` | `none` |
| scaffold item replace | `PUT item(E)` | `json` |
| scaffold item delete | `DELETE item(E)` | `none` |
| command `N` | `POST collection(E)/actions/op(N)` | `json` |
| query `N` | `GET collection(E)/queries/op(N)` | `none` |
| transition `N` | `PATCH collection(E)/actions/op(N)` | `json` |

`item(E)` appends one `/{field}` segment for every primary-key field in key
order. Thus `Task` uses `/tasks/{id}`, while composite-key `TaskTag` uses
`/task-tags/{taskId}/{tagId}`. A collection pin changes the collection prefix
for scaffold and conventionally routed operations together.

For the complete example, the derived named-operation routes are therefore:

```text
POST  /tasks/actions/create
GET   /tasks/queries/open
PATCH /tasks/actions/complete
```

Single-endpoint components also use fixed rules. `input` means at least one
typed parameter or an `on` reference:

| Component | Method and path | Consumes |
|---|---|---|
| controller/handler without input | `GET /op(N)` | `none` |
| controller/handler with input | `POST /op(N)` | `json` |
| webhook | `POST /webhooks/op(N)` | `json` |
| socket handshake | `GET /ws/op(N)` | `none` |
| client | no default; exactly one explicit remote route | route default |

For example, `component controller Health` derives `GET /health`, and
`component webhook Stripe` derives `POST /webhooks/stripe`; neither repeats a
route in source.

`route METHOD "path" [consumes format]` replaces one operation or inbound
component's derived row. It is a contract pin for an existing frontend or
public API, not the normal way to expose one. A generator command omits a route
statement when its CLI inputs equal the convention; otherwise it writes the
complete pinned method, path, and non-default consumption mode. `@internal`
forbids both `route` and `bind`.

Valid method/kind pairs are:

| Kind | Methods |
|---|---|
| command | `POST` |
| query | `GET`, `POST` |
| transition | `PUT`, `PATCH`, `POST` |
| controller/client/handler component | `GET`, `POST`, `PUT`, `PATCH`, `DELETE` |
| webhook component | `POST` |
| socket component | `GET` (upgrade handshake) |

On an explicit route, omitted `consumes` means `none` for GET/DELETE and `json`
otherwise. `form` means `application/x-www-form-urlencoded`. A GET may use
`form` to mean query-string model binding, matching browser behavior; it does
not carry an HTTP body. GET and DELETE reject `consumes json` in v1.

A route path MUST start with `/`, contain no scheme, query, fragment, repeated
slash, percent-encoded slash, or `.`/`..` segment, and end with `/` only when it
is exactly `/`. A placeholder is exactly `{FIELD_IDENT}` and may appear once.
Static path characters are ASCII letters, digits, `_`, `-`, `.`, and `~`.
Paths are case-sensitive and are compared exactly after decoding the JDL string.

Canonical and pinned-route parameter sources are deterministic:

1. a `{name}` route variable binds the same-named parameter from `path`;
2. a scope field binds from its configured authenticated `claim`;
3. a transition version parameter under `if-match required|optional` binds from
   the `If-Match` header;
4. remaining GET/DELETE parameters bind from `query`;
5. remaining parameters bind from the aggregate JSON body or from `form`.

With `consumes none`, every parameter must resolve to path, query, header, or
claim through the defaults above or an explicit binding; a body is never
inferred. A parameter can have exactly one effective source. An explicit claim
binding is valid only for a scoped field, and explicit form binding requires
`consumes form`.

`bind parameter from source ["wire-name"]` overrides one default. The quoted
wire name is itself a contract pin. For example, this fixed legacy form route
does not alter the naming rules for any generated Java type:

```jdl
transition MarkSeen(id, version) {
  select [id]
  set seen = true
  if-match optional
  route POST "/customer_api/seen" consumes form
  bind id from form "note_id"
}
```

Without the quoted name, the wire name is the parameter name. Every path placeholder
must bind exactly once; no undeclared placeholder or unused explicit path
binding is allowed. A JSON body is one derived aggregate binding and cannot be
spelled by a per-field `bind` in v1; its field names are the declared lowerCamel
parameter names. Explicit per-field names are for form/query/header/path/claim
sources.

Routes, including derived routes, are part of the linked portable contract.
Inbound routes are unique by
`(listener binding, method, path)`; this includes operation, scaffold, handler,
webhook, and WebSocket handshake routes. An outbound client route is relative
to that client's configured base URL and conflicts only with another call of
the same method/path in the same client. Inbound and outbound namespaces do not
collide. Every conflict is reported before emission.

## 13. Relations

Relations represent database referential integrity, not ORM navigation.

```jdl
entity Item {
  ownerId:  uuid
  tenantId: uuid

  relation owner to Owner @id(rel_item_owner) {
    map ownerId  -> id
    map tenantId -> tenantId
    on delete restrict
    on update restrict
  }
}
```

Rules:

- a relation is nested in the child/foreign-key entity;
- it has one or more ordered `map local -> remote` entries;
- each of `on delete` and `on update` appears at most once;
- local fields must be distinct and remote fields must be distinct;
- corresponding logical types must agree;
- remote fields must be required and exactly match a primary or unique
  constraint;
- a composite local tuple must be either entirely required or entirely
  nullable, so a partial foreign-key identity cannot be represented;
- when the parent is scoped, the mapping must include every parent scope field
  and a child scope field with the same claim name; an unscoped parent is a
  deliberate global reference;
- the default for both referential actions is `restrict`;
- `set-null` requires every local field to be nullable;
- the generated SQL constraint name may be supplied with `@map("name")` on
  the relation header; and
- relation names are lowerCamel and unique within the child entity.

One-to-one is derived when the local field tuple is also unique; otherwise the
relation is many-to-one. One-to-many is the inverse read shape and is expressed
by a query, not a stored collection. Many-to-many uses an explicit join entity:

```jdl
entity TaskTag {
  use repo

  taskId: uuid
  tagId:  uuid

  pk [taskId, tagId]

  relation task to Task {
    map taskId -> id
  }

  relation tag to Tag {
    map tagId -> id
  }
}
```

This makes the table name, composite key, tenant columns, indexes, lifecycle,
and future fields explicit instead of hiding them in an implicit ORM join.

## 14. Generic components

`component <kind>` is the generic desired-state form for generators that are
not entity structure, projections, relations, or the four operation kinds.
It is not an untyped escape hatch.

```jdl
component strategy RewardRule @id(cmp_strategy_reward) {
  on Transaction
  yields Reward
  variant Coffee
  variant LargeTransaction
}

component durable-job ItemDispatcher(
  id: uuid,
  ownerId: uuid,
  name: string @notBlank
) {
  on AddItem
  yields Item
}
```

### 14.1 Common component shape

A component may have typed header parameters and may carry `@id`. Its name is
the semantic stem from which the kind registry derives every Java type. Its
body vocabulary is closed:

| Member | Value shape | Meaning |
|---|---|---|
| `on` | symbol reference | input, subject, dispatcher, or wrapped component according to kind |
| `yields` | symbol reference | output/event/resource according to kind |
| `route` | section 12.6 | one HTTP endpoint/call |
| `bind` | section 12.6 | server-side binding override |
| `variant` | named, optionally typed payload | closed/open implementation member |
| `source` | project-relative string path | reader-owned exact input |

The kind registry determines which members are required, optional, or
forbidden. Component and variant parameters use the typed-parameter attribute
schema in section 12.1. A parsed but irrelevant property is never ignored.

### 14.2 Closed v1 component-kind registry

`params` below means typed header parameters are allowed. In the `Layer/type`
column, `N` is the declared semantic stem. These placements and primary names
are fixed; helper types use additional closed role prefixes/suffixes from the
same registry.

| Kind | Layer / primary type | Parameters/body | Required references | Required caps or app axes |
|---|---|---|---|---|
| `class` | domain / `N` | params | none | Java |
| `interface` | app / `N` | params | none | Java |
| `service` | service / `NService` | params | none | `platform spring` |
| `controller` | web / `NController` | params, optional route pin; bind allowed | optional `on`, optional `yields` | `platform spring` |
| `sealed` | domain / `N` | one or more variants; payloads allowed | none | Java |
| `strategy` | service / `N` | one or more payload-free variants | required `on`, optional `yields` | `platform spring` |
| `handler` | api / `NHandler` | params, optional route pin | none | derived default `http`, `platform plain` |
| `command` | cli / `NCommand` | params, optional `on` dispatcher | none | `platform plain` CLI app |
| `cli` | cli / `NCli` | params | none | `platform plain` CLI app |
| `cases` | testkit / `NCases` | required `source`; no params | none | Java |
| `client` | clients / `NClient` | params, exactly one outbound route | optional `on`, optional `yields` | `platform spring` |
| `fetcher` | clients / `NFetcher` | params | none | `platform spring` |
| `job` | jobs / `NJob` | params | none | `platform spring` |
| `http-workflow` | jobs / `NWorkflow` | params; fixed status/pages/cancel route profile | required `on` fetcher | `storage postgres`, `platform spring` |
| `http-sink` | jobs / `NHttpOutboxSink` | params | required `on` command, required `yields` event | `storage postgres`, `cap json`, `platform spring` |
| `idempotency` | service / `NGuard` plus app/adapter support | params | none | `storage postgres`, `platform spring` |
| `auth` | api / `NTokenConfig`, `NTokens` | params | none | `cap security`, `platform spring` |
| `webhook` | api / `NVerifier`; web / `NWebhookController` | params, optional POST route pin; bind allowed | none | `platform spring` |
| `durable-job` | jobs / `NWork`, `NQueue`, `JdbcNStore`, `NWorker`; web / `NJobController` | params | required `on` command, required `yields` entity | `storage postgres`, `platform spring` |
| `socket` | web / `NSocketHandler` | params, optional GET handshake route pin | none | `platform spring` |
| `presence` | adapters / `NPresence` | params | none | `storage postgres`, `platform spring` |
| `test` | testkit / `NTest` | params | none | Java |
| `integration-test` | testkit / `NIT` | params | none | Java plus derived integration-test build feature |

The registry entry also owns the canonical suffix, argument semantics,
prerequisites, projection class, fixed route profile where listed,
implementation-boundary shape, and emitter.
The CLI command catalog and JDL validator MUST read this same registry. Adding a
kind in only one surface must fail an exhaustive test.

`class` and `interface` are intentionally narrow: a generic class is a domain
type and a generic interface is an application port. Code in another layer
uses the semantic kind for that layer (`service`, `client`, `job`, and so on)
or is reader-owned. This avoids reintroducing package selection through a
generic escape hatch.

Each kind entry supplies a closed `forbidden_source_suffixes` set covering its
managed family. If `N` already ends in one of those suffixes, validation fails
rather than producing two source spellings for the same type. For example,
`component client AuditClient` is fixed to `component client Audit`. Kinds whose
primary type is exactly `N` have no such restriction.

An inbound single-route component omits `route` when section 12.6's convention
fits; a different route is a guarded contract pin. An outbound client must
state its remote route because there is no honest local convention for another
system's API. A fixed multi-route profile derives its complete versioned route
set and displays it in `model plan`, as with scaffold CRUD.

`cases` converts headings/scenarios from its captured source file into managed
tests. The source file is reader-owned and is an exact compilation input;
changing it between plan and apply makes the plan stale.

`sealed` variants are a closed set and may carry record payloads. A generated
exhaustive switch test has no default. `strategy` variants are an open set of
Spring implementations and therefore cannot carry data declarations; `on` is
the examined type, while optional `yields` changes a predicate into an
`Optional<Yields>` strategy.

## 15. Caps and project declarations

### 15.1 Capabilities

Caps describe project-wide generated support that is not already selected by
the `app` block. JDL uses canonical names only. CLI aliases such as `image`,
`kubernetes`, `errors`, `events`, `smtp`, `metrics`, and `faults` MUST be
expanded before a CLI edit writes JDL. `db`/`postgres` and `h2` are storage
selections, not cap aliases in source. The same canonicalization rule applies
to artifact aliases.

The v1 capability registry is closed and has two parameter classes. Package,
class, file, and build-entry placement are always conventional:

| Class | Caps | Instance name |
|---|---|---:|
| named, repeatable | `csv`, `sqlite`, `json`, `http` | optional; required for a second instance |
| singleton | `api`, `actuator`, `cache`, `security`, `cors`, `sse`, `mail`, `redis`, `observability`, `kafka`, `testkit`, `fake`, `format`, `coverage`, `loadtest`, `ci`, `docker`, `k8s`, `toxiproxy`, `fast-test` | forbidden |

Those rows contain 24 canonical cap kinds. Together with current Jails `db`
and `h2`, which map to `app.storage`, they cover all 26 current capability
kinds. `sqlite` is one of the 24; only its primary/default instance may instead
be represented by `storage sqlite`.

Examples:

```jdl
cap json Orders
cap json Audit
cap security
cap sqlite AuditCache
cap fast-test
```

A bare repeatable cap is its conventional default instance. Therefore
`cap json` and `cap json Json` denote different instances and
MUST NOT be normalized into one another. Instance names are UpperCamel because
they become part of generated type names.

Only `@id` is a valid cap attribute. The registry owns every generated package,
type suffix, property prefix, dependency scope, test name, and file path.

The registry labels every prerequisite either `derived` or `chosen`. JDL v1 has
exactly four derived support rules: primary `storage` to its database support,
`use seed` to the default `json` instance, `component handler` to the default
`http` instance, and any output role marked `integration` to the selected
build's integration-test feature. They appear in the linked model and plan but
are not repeated as `cap` declarations.

Every other non-storage cap is a policy choice and MUST be declared. The linker
reports all missing chosen caps in one pass. `jails add` may insert a requested
chosen cap and every missing chosen prerequisite as one atomic CST edit;
mechanical derived support stays out of source. Removal is refused while
another declaration requires the cap. An unqualified cap prerequisite is
satisfied only by the bare/default instance, not by an arbitrary named one.
An explicit bare cap and a derived edge to that same cap coalesce by stable ID;
the explicit declaration is useful only when the application selects that cap
independently of the declaration that currently derives it.

Primary storage is handled once in `app`, so the exhaustive current-to-JDL
mapping is:

| Current Jails capability | JDL v1 representation |
|---|---|
| `db` or alias `postgres` | `storage postgres` (`App.storage = Postgres`) |
| `h2` | `storage h2` (`App.storage = H2`) |
| bare `sqlite` used as the primary datasource | `storage sqlite` (`App.storage = Sqlite`) |
| named or auxiliary `sqlite` | `cap sqlite [Name]` |
| every other canonical capability | `cap <kind> [Name]` |

An importer determines whether an observed bare SQLite instance is the primary
entity datasource. If the evidence is ambiguous, import MUST stop and request
an explicit storage choice; it MUST NOT guess. Named SQLite instances are
always auxiliary and do not make entity fields persistent in that store.

The current native/compatibility implementation status is not language syntax.
A compiler build that lacks an emitter for a valid registry entry reports an
unsupported-feature diagnostic; it may not parse the cap and then
silently omit it.

### 15.2 Dependencies

A `dep` declaration owns one build dependency coordinate:

```jdl
dep org.example:audit-runtime @version("2.4.1") @scope(runtime)
dep org.assertj:assertj-core @version("3.27.4") @scope(test)
```

Its attribute schema is:

| Attribute | Default | Rule |
|---|---|---|
| `@id(stable_id)` | derived | stable identity |
| `@version("version")` | platform-managed | exact version text; no ranges or `latest` |
| `@scope(compile\|runtime\|test)` | `compile` | target dependency configuration |

The same source declaration lowers according to `app.build`:

| JDL scope | Maven | Gradle (Groovy) |
|---|---|---|
| `compile` | default/`compile` dependency | `implementation` |
| `runtime` | `runtime` scope | `runtimeOnly` |
| `test` | `test` scope | `testImplementation` |

The derived Gradle `integrationTestImplementation` configuration extends
`testImplementation`; Maven Failsafe uses Maven's test classpath. JDL therefore
needs no second dependency scope merely to express the same test dependency for
the two build systems.

An absent `@version` means the selected platform/build projection manages the
version (for example, Spring dependency management). It never means `latest`.
An explicit version is emitted verbatim into the selected build format after
that format's syntax validation.

Coordinates are exactly `group:artifact`; classifiers, file dependencies,
repositories, exclusions, and annotation-processor wiring are not expressed by
this form. Those need a future typed declaration or an ejected build boundary.
The same coordinate may appear only once, because two versions or scopes for
one artifact are ambiguous across build tools.

Jails edits only dependency entries whose stable IDs it owns. An equivalent
reader-owned dependency may satisfy planning, but it is not silently adopted
or removed. Adoption is an explicit CLI operation that first proves the
coordinate, version, and scope match.

### 15.3 Properties

`prop` owns one application property in one of two targets:

```jdl
prop server.port = 8081
prop logging.level.com.example = "DEBUG" @target(test)
prop feature.preview = false @target(main)
```

Valid attributes are `@id` and `@target(main|test)`; the target defaults to
`main`. The pair `(target, key)` is unique. Values are scalar literals and are
rendered with the canonical Spring property spelling: strings as their decoded
contents, numbers in base ten, and booleans lowercase. Enum-like bare constants
are accepted only where the property registry declares a closed value set;
otherwise a textual value must be quoted.

Secrets MUST NOT be stored as prop values. Keys declared secret by the
property registry reject a concrete literal and require the app's
external secret mechanism; unregistered keys that look secret produce a
warning. JDL may configure an exact environment placeholder such as
`"${AUDIT_TOKEN}"`, but it does not own the environment value.

As with deps, Jails owns only the exact key/value entry, not the whole
properties file. A reader-owned duplicate is a conflict unless explicitly
adopted. Removing a declaration removes only the owned entry.

## 16. Lifecycle, change evidence, and ownership

### 16.1 Desired state versus a requested change

The compiler compares three inputs:

1. the new linked JDL model;
2. the last accepted linked model and ownership ledger; and
3. the observed reader state and immutable migration history.

It classifies each difference as `safe`, `guarded`, or `unsupported`. `safe`
changes can plan immediately. `guarded` changes require typed evidence captured
by the command or plan, never an attribute copied into permanent desired state.
`unsupported` changes require an explicit migration or an ejected boundary.

The exact evidence is part of the sealed plan input and its digest. If an
evidence file, JDL span, reader file, accepted model, or migration changes before
apply, the plan is stale.

Adding or changing a contract pin is `guarded` even when it has no storage
effect; section 3.1's exact `PinEvidence` is required. Removing a pin changes
the effective contract back to the current convention and is also guarded
unless both values are equal and no accepted rename depends on the pin.

Removing a non-storage projection, operation, component, cap, dep, or prop
removes only unchanged outputs/entries owned by that
node. A public route or ABI removal is a guarded contract change. A modified
owned output is a reader conflict and must be adopted/ejected or restored; it is
never overwritten merely because its declaration disappeared.

### 16.2 Entity retirement

An active stored entity MUST NOT disappear from JDL. It is retired first:

```jdl
entity LegacyOrder @id(ent_legacy_order) @retired {
  table "legacy_orders"

  id: uuid @id(fld_legacy_order_id) @pk
}
```

`@retired` means runtime projections, operations, routes, and generated Java are
inactive while the physical table is preserved. Its fields, constraints,
relations, physical mappings, projection membership, and stable IDs remain in
the linked model so revival is deterministic. The compiler still checks the
retained storage shape but ignores set-scoped `use` rules for code emission.

`@retired(drop)` means the entity is inactive and its physical table is absent.
The transition to that state is guarded by the exact table name and emits one
forward drop migration. The linked model retains the former shape for audit and
name-reservation purposes, but revival requires a new explicit create/restore
plan; it cannot assume the old data still exists.

The only valid entity-header attributes are `@id`, `@retired`, and
`@retired(drop)`. Retirement never changes canonical package placement.

Removing `@retired` revives the entity. A preserved table must be confirmed by
its exact physical name and observed compatible shape. A dropped table requires
an explicit restoration/creation plan. A retired entity may be forgotten only
after its table is dropped or transferred to a reader; forgetting removes its
JDL declaration but never deletes migration history.

### 16.3 Field and constraint evolution

The durable source always shows the final shape:

```jdl
// before
nickname: string?

// after
nickname: string @notBlank
```

The nullable-to-required transition additionally needs a backfill literal or a
project-relative SQL evidence file. That evidence is not rendered as
`@default`, because a one-time backfill does not grant a default to future
inserts.

Change policy is:

| Change | Classification | Required evidence or rule |
|---|---|---|
| add nullable/defaulted field | safe | no existing row is invalid |
| add required field | guarded | typed backfill literal or exact SQL file |
| nullable to required | guarded | typed backfill evidence |
| required to nullable | safe | widening |
| type widening supported by storage backend | guarded | exact old column and emitted conversion |
| type narrowing/semantic conversion | unsupported by inference | reader SQL migration plus explicit type change |
| rename Java field only | guarded rename | preserve stable ID |
| rename physical column | guarded rename | preserve field ID and confirm old column |
| drop field | guarded | exact physical column |
| add index/unique | guarded | generated migration; uniqueness preflight where needed |
| drop index/unique | guarded | exact physical constraint/index name |
| reorder record fields | ABI change | plan displays constructor and serialization impact |

Changing `@map` while retaining an ID is a physical rename, not a second column.
Changing `@id` is remove/add even if the displayed name is unchanged.

### 16.4 Ejection

Ejection transfers one generated implementation boundary to the reader while
keeping its port/contract managed:

```jdl
eject Task.repo.fake @id(eject_task_fake_repo)
eject id(boundary_cmp_client_audit_implementation)
```

The preferred reference is a readable, linked boundary path. `id(...)` is the
unambiguous escape when a boundary has been renamed. The boundary registry, not
string concatenation in the parser, defines valid paths. Examples include
`Entity.record`, `Entity.service`, `Entity.repo.fake`,
`Entity.repo.postgres`, `Entity.repo.h2`,
`Entity.repo.sqlite`, `Entity.http.api`, and `Component.implementation`.

An ejection resolves to and stores the stable boundary ID at link time. It is
one-way: removing the `eject` line is an error, because Jails cannot prove a
reader implementation is safe to overwrite. Re-adoption requires a separate
command that compares the reader implementation to the canonical generated
form and obtains explicit confirmation.

After ejection, the compiler:

- continues to validate the boundary's public ABI;
- stops writing and deleting its implementation files;
- treats those files as exact reader inputs when a downstream generated unit
  depends on them; and
- reports incompatible contract changes before emission.

Ejecting a broad entity or package is forbidden; each implementation boundary
is explicit so unrelated generated units remain refreshable.

## 17. Exhaustive Jails coverage

This section is the conformance inventory. A registry change that makes either
table incomplete MUST fail tests and block release.

### 17.1 Every artifact kind

| Current `ArtifactKind` / CLI word | JDL representation |
|---|---|
| `scaffold` | `entity` plus local `use scaffold` |
| `controller` | `component controller` |
| `service` | standalone `component service`; entity service is `use service` |
| `class` | `component class` |
| `interface` | `component interface` |
| `record` | `entity` and its implicit record projection |
| `field` | a field declaration inside its entity; not a top-level kind |
| `factory` | `use factory` |
| `value` | `entity` plus local `use value` |
| `enum` | `enum` |
| `sealed` | `component sealed` with `variant` members |
| `strategy` | `component strategy` with `variant` members |
| `repo` | `use repo` |
| `migration` | no declaration; append-only migration history |
| `handler` | `component handler` |
| `command` | standalone CLI generator is `component command` |
| `cli` | `component cli` |
| `cases` | `component cases { source "..." }` |
| `client` | `component client` |
| `fetcher` | `component fetcher` |
| `job` | `component job` |
| `http-workflow` | `component http-workflow` |
| `association` | `relation` inside the child entity |
| `http-sink` | `component http-sink` |
| `idempotency` | `component idempotency` |
| `auth` | `component auth` |
| `webhook` | `component webhook` |
| `search` | `use search(fields: [...])` |
| `durable-job` | `component durable-job` |
| `dto` | `use dto` |
| `usecase` | nested entity `command` |
| `query` | nested entity `query` |
| `transition` | nested entity `transition` |
| `event` | nested or top-level `event` |
| `socket` | `component socket` |
| `presence` | `component presence` |
| `seed` | `use seed` |
| `test` | `component test` |
| `integration-test` | `component integration-test` |

The two different meanings of today's CLI word `command` are deliberately
separate: an app create operation is the semantic `command` nested in
an entity; a plain Java CLI subcommand is `component command`.

### 17.2 Every generation argument

The positional `kind` and `name` choose the declaration. Remaining positional
values become entity fields (`record`, `scaffold`, `value`, `field`), enum
values, sealed/strategy variants, operation or component parameters, search
`fields: [...]`, relation mappings, or the `cases` source according to the
kind's registry schema. One raw vector with kind-dependent parsing MUST NOT
survive into the linked model. The 18 option arguments map as follows:

| CLI argument | Durable JDL or transient evidence |
|---|---|
| `--timestamps` | explicit `createdAt` and `updatedAt` fields with `now()`/`@updated`; no shorthand remains |
| `--package` | refused for a managed JDL projection; move to the canonical layer or eject/adopt the boundary |
| `--default-literal` | typed one-change backfill evidence; not an ongoing `@default` |
| `--backfill-file` | exact reader-owned one-change SQL evidence |
| `--index` | field `@index` for one ascending field; otherwise one `index [...]` declaration per occurrence |
| `--on` | operation nesting, relation child, or component `on` member, according to kind |
| `--yields` / `--returns` | component `yields`, relation target, or emitted/result type according to kind |
| `--via` | explicit query `join` or command `resolve`; inferred join facts are written out |
| `--order-by` | `order by [...]` |
| `--limit` | `limit N` |
| `--on-conflict` | `conflict on [...]` |
| `--path` | omitted when conventional; otherwise operation/component `route` or scaffold `path:` contract pin |
| `--select` | transition `select [field]` |
| `--set` | one typed `set field = value` per occurrence |
| `--if-match` | transition `if-match` |
| `--bind` | `bind field from form "wire-name"`; the CLI option remains valid only with form consumption |
| `--method` | omitted for a conventional entity operation; explicit in a generic component route or non-conventional operation route |
| `--consumes` | omitted when its operation/component default applies; otherwise explicit route consumption contract |

CLI conveniences derive conventions while building a patch. The CST edit MUST
materialize every semantic fact and every departure from convention shown in
this table, but MUST NOT repeat a derived name merely because a CLI displayed
it. Re-reading the edited JDL must produce the same plan without replaying
command arguments.

The `--package` row is an intentional compatibility break, not an uncovered
behavior: package placement changes presentation but adds no application
capability. Existing custom-package code remains usable through the documented
move/eject/adopt path; new managed code has exactly one placement.

### 17.3 Resource lifecycle commands

| CLI intent | Source edit | Plan evidence |
|---|---|---|
| add field | insert field declaration | backfill when required |
| rename field | change name, materialize old ID | old Java/column projection as applicable |
| change field type | replace type, preserve ID | storage conversion or exact SQL file |
| change nullability | add/remove `?` | backfill for tightening |
| drop field | remove declaration | exact old column |
| add index | add field `@index` or entity `index [...]` | generated DDL and uniqueness preflight if relevant |
| drop index | remove constraint | exact old physical name |
| rename resource Java projection | change entity name, preserve ID | old Java type |
| rename table | change `table`, preserve entity ID | exact old table |
| retire, preserve | add `@retired` | exact entity and table identity |
| retire, drop | add `@retired(drop)` | exact table and forward drop migration |
| revive | remove `@retired...` | exact compatible table or explicit restore/create plan |
| eject | add `eject` declaration | exact implementation boundary |

`new` and `new-cli` are bootstrap actions, not declarations; they materialize
the selected `app` axes in JDL. Other operational commands (`doctor`, `test`,
`run`, `sql`, broker/console commands, and plan review/apply) do not need a JDL
representation because their inputs do not alter desired app structure.

## 18. Static semantics and diagnostics

### 18.1 Namespaces and resolution

App, entity, enum, component, and top-level event names share one
global type namespace. Entity fields/relations/operations, enum values,
component variants, and operation aliases each have a separate parent-local
namespace. Operation names are unique across all four operation kinds in one
entity.

An unqualified field path resolves in the enclosing entity/operation. A
`Type.field` path resolves the global type first; an `alias.field` path resolves
an operation join alias. `emit Event` resolves a same-entity nested event first,
then a unique top-level event. Selectors resolve entities only, and a boundary
reference resolves its first segment in the global type namespace. There is no
filesystem or Java-import-based fallback.

### 18.2 Validation passes

One `model check` runs these passes in order and performs no writes:

1. **lex/parse** — recover at declaration boundaries and collect syntax errors;
2. **identity** — derive IDs, detect duplicates, and reserve retired names;
3. **name link** — resolve types, fields, symbols, selectors, events, and boundaries;
4. **type check** — validate literals, mappings, defaults, parameters, and results;
5. **kind schemas** — validate projection/component/cap attributes and members;
6. **convention projection** — derive packages, types, files, SQL names, routes, tests, migrations, and prerequisite support;
7. **cross-model invariants** — keys, relations, routes, scopes, operations, and prerequisites;
8. **workspace compatibility** — platform, build, Java, storage, build features, and reader ABIs; and
9. **change policy** — compare accepted state and report required evidence.

No emitter, migration allocator, syntax edit, or reader write may run until all
passes succeed. Independent errors SHOULD be accumulated; an error whose input
could not be linked may be suppressed to avoid cascades.

Important global invariants include:

- Java fully qualified names, physical table/column names, routes, stable IDs,
  cap instance IDs, and owned output paths are unique;
- every stored entity has exactly one primary key and a supported storage backend;
- relations target a key and cannot create a required cascade cycle;
- every operation is total over its required inputs and emitted payloads;
- every reader-owned input path is normalized, project-relative, contains no
  `..`, and is hashed into the plan;
- every generated output has exactly one owner or is explicitly ejected;
- no declaration is accepted with an irrelevant member or attribute;
- no semantic stem repeats its role's generated prefix/suffix; and
- no managed unit carries a package, filename, test-name, or suffix override.

### 18.3 Diagnostic contract

Diagnostics are structured and stable:

```text
code, severity, file, primary span, message, related spans[], notes[], fix?
```

Codes are grouped by phase:

| Range | Class | Example |
|---|---|---|
| `JDL0001–0099` | lexical/version | invalid escape, unsupported `jdl` version |
| `JDL0100–0199` | grammar | missing `}`, illegal semicolon |
| `JDL0200–0299` | identity/name/convention | duplicate ID, unknown field, redundant generated suffix |
| `JDL0300–0399` | type/value | invalid default, mismatched relation types |
| `JDL0400–0499` | declaration schema | member forbidden for component kind |
| `JDL0500–0599` | cap/platform/build/storage | missing cap, incompatible project axis |
| `JDL0600–0699` | contract | duplicate route, incomplete event payload |
| `JDL0700–0799` | evolution | destructive edit lacks typed evidence |
| `JDL0800–0899` | ownership/reader | reader conflict, ejection cannot be reversed |
| `JDL0900–0999` | internal registry | non-exhaustive kind mapping, emitter unavailable |

Messages MUST state the failed rule, name the declaration, point to its primary
span, and show related spans where another declaration caused the conflict.
Unknown closed names SHOULD include at most three edit-distance suggestions.
A fix is either a safe source edit or a concrete command; it is never a vague
instruction to "check the model".

Examples:

```text
JDL0214 .jails/model.jdl:42:19: component service BillingService repeats the
  generated suffix `Service`
derived: component service Billing -> com.example.tasks.service.BillingService
fix: replace `BillingService` with `Billing`

JDL0602 .jails/model.jdl:61:3: pinned route POST /legacy/tasks is already owned
  by operation ImportTask (line 25)
fix: change the external contract pin or make one operation @internal
```

Warnings cannot stand in for refused safety checks. A fact that would change
generated behavior is either modeled, derived and displayed, or rejected.

### 18.4 Derived-value inspection

Convention must not mean hidden behavior. `model plan` includes a `derived`
array, and `model explain <stable-id-or-boundary>` filters the same records.
Each record has this versioned shape:

```text
owner_id, role, value, rule_id, input_ids[], source_spans[], pinned, replaces?
```

`role` is closed (`java-package`, `java-type`, `file`, `sql-table`,
`sql-column`, `sql-constraint`, `http-route`, `test`, `migration`,
`cap-prerequisite`, or `build-entry`). `pinned` is true only for a contract pin;
`replaces` then contains the conventionally derived value it displaced.

For example, `model explain Task.repo.postgres` must be able to report:

```text
com.example.tasks.app.TaskRepository
  rule: java.type.v1(repository-port)
  from: entity Task + use scaffold

com.example.tasks.adapters.JdbcTaskRepository
  rule: java.type.v1(postgres-repository)
  from: Task.repo + app.storage=postgres
```

Human and JSON output are projections of the same typed records. Tests compare
the records, not terminal prose.

## 19. Formatting and CLI source edits

### 19.1 Canonical formatter

`jails model fmt` formats `.jails/model.jdl`; `--check` exits nonzero without
writing when formatting differs. The canonical style is:

- two-space indentation and LF line endings;
- one field/member per logical line;
- a maximum target width of 100 columns, wrapping only inside matching
  delimiters;
- inside an entity: `use` lines first, then `table` when present, fields,
  constraints, relations, and operations; one blank line separates those
  groups;
- one blank line between top-level declaration groups;
- double-quoted strings with the shortest valid JSON escapes;
- lowercase canonical kind/type/cap names and uppercase HTTP methods;
- explicit `asc` omitted and `desc` retained; and
- attributes ordered `@id`, identity/default, validation, behavior,
  physical, lifecycle, with the registry supplying the exact rank.

The formatter removes identical duplicate `use` selections but never combines
rules when doing so would change comments or selector meaning. It is
idempotent, and `link(parse(format(parse(source))))` MUST be semantically equal
to `link(parse(source))`.

Formatting never writes derived IDs, packages, type suffixes, SQL names,
routes, test names, prerequisites, or migration names. It retains an explicit
contract pin even when that pin currently equals the convention, because the
pin may intentionally preserve the value across a later semantic rename.

### 19.2 Minimal CLI edits

Model-mutating CLI commands build a candidate CST in memory, then parse,
validate, and plan it before atomically replacing the file as part of the
reviewed apply transaction. They MUST:

- preserve the original newline style until an explicit format command;
- preserve comments and byte-for-byte text outside touched declaration spans;
- insert a child beside members of the same class;
- materialize a derived ID when a rename needs it;
- omit a package/name/route argument when the convention already determines it;
- refuse `--package` for managed output with the exact canonical destination
  and ejection alternative in the diagnostic;
- refuse an edit when syntax errors make the target span ambiguous; and
- write through a same-directory temporary file with compare-and-swap against
  the source digest.

No failed or merely reviewed plan changes the source. `--plan` or `--pretend`
prints the prospective JDL diff and plan without replacing it.

Manual edits and CLI edits converge through the same pipeline:

```text
source -> parse -> link/check -> diff accepted model -> exact plan -> review -> apply
```

There is no second semantics path that converts CLI flags directly into files.

## 20. Compiler architecture

The v1 implementation should replace the current line parser and
JDL-to-intermediate-TOML rendering with a direct typed compiler.

### 20.1 Required layers

1. **Lexer** — emits tokens, significant newlines, trivia, and byte spans.
2. **Error-tolerant parser** — produces the lossless CST and diagnostics.
3. **AST lowering** — converts grammar forms to small source structs without
   resolving names.
4. **Linker** — derives IDs, expands selectors/profiles, resolves symbols, and
   produces the tagged linked model.
5. **Validator** — runs section 18 against the workspace and registries.
6. **Differ** — compares stable-ID maps and emits typed `ModelPatch` values.
7. **Planner** — combines patches with typed transient evidence and ownership.
8. **Emitters** — consume only a validated linked model and exact plan.
9. **Syntax editor/formatter** — edits CST spans; it does not depend on an
   emitter's rendered Java or SQL.

The parser MUST NOT call the TOML parser, and emitters MUST NOT inspect JDL
tokens. CST, linked model, accepted model, and execution plan have distinct
serialized versions and digests.

### 20.2 Shared registries

One registry crate is authoritative for:

- app platform/build/storage values and their compatibility matrix;
- scalar types and storage/dialect mappings;
- canonical layer packages, Java role prefixes/suffixes, source roots, SQL
  names, HTTP routes, tests, and migration filenames;
- field/declaration attribute and entity-constraint schemas;
- projections and the versioned `scaffold` expansion;
- component kinds and member schemas;
- caps, instance class, derived/chosen prerequisites, and emitters;
- operation statement schemas and result ABI;
- implementation boundaries; and
- CLI artifact-kind/JDL mappings.

At minimum, every Java-emitting role has a data entry equivalent to:

```rust
struct OutputConvention {
    rule_id: ConventionRuleId,
    role: OutputRole,
    source_set: SourceSet,       // Main | Test | Integration
    layer: Layer,                // the closed section 9.7 enum
    stem: StemSource,            // Owner | CapInstance | Fixed("...")
    prefix: &'static str,
    suffix: &'static str,
    extension: &'static str,     // ".java", ".sql", ...
    integration: bool,
}
```

The linker resolves this to an `OutputName` and `OutputPath`; emitters receive
those values and MUST NOT concatenate a package, prefix, suffix, filename, or
test marker themselves. A component/cap with several files has several role
entries. Exhaustiveness tests fail when an emitter asks for an unregistered
role or when a registered role has no emitter.

The CLI help, `jails commands --json`, JDL completion, diagnostics, and
exhaustive tests are generated from or validate against these registries. A
component kind is represented by a tagged payload such as
`Component::HttpSink(HttpSinkSpec)`, not `HashMap<String, Value>`.

### 20.3 Normalization versus source

Expansion occurs only in the linked model:

- `scaffold` expands to its member projections;
- set-scoped `use` expands to entity memberships;
- packages, generated Java names/files, SQL names, conventional HTTP routes,
  tests, and migration descriptions become explicit derived values;
- omitted primary-key defaults, route binding sources, query tiebreak order,
  and referential actions become explicit derived values;
- cap and storage requirements become chosen or derived edges; and
- readable boundary references resolve to stable IDs.

The compiler records each derived value with its rule and source span so
`model explain <id>` can show why it exists. It MUST NOT rewrite the concise
source merely to expose derived defaults, except when a CLI edit needs a fact
to remain stable across a rename or ambiguity.

### 20.4 Language versioning

`jdl 1` selects this exact grammar and registry contract. A compiler may read
older versions for upgrade, but it MUST refuse a newer major version before
planning. New optional attributes or kinds require a new language version if
an old compiler could otherwise accept and ignore their semantics. No unknown
node is retained as an untyped extension bag.

## 21. Conformance test suite

Implementation is complete only when all of these test families exist:

1. **Lexer/parser goldens** — every grammar production, recovery boundary,
   comment position, CRLF input, escape, invalid token, and multiline form.
2. **Formatter properties** — idempotence, comment preservation, semantic
   round-trip, and untouched-span preservation for every CLI edit.
3. **Identity tests** — every derived form, truncation/hash, global collision,
   rename materialization, and remove/add distinction.
4. **Convention snapshots** — every layer, Java role prefix/suffix, acronym,
   plural, SQL constraint, scaffold/operation route, unit/IT name, migration
   description, collision, contract pin, redundant-suffix diagnostic, and
   managed `--package` refusal.
5. **Registry exhaustiveness** — every one of the 39 `ArtifactKind` values,
   26 capabilities including `fast-test`, 18 generation options, projection,
   component, attribute, default, and boundary has exactly one mapping.
6. **Link/type tests** — forward references, selectors, enum wire values,
   defaults, relations, joins, resolves, scope, events, and route bindings.
7. **Project matrix** — Spring/plain, Maven/Gradle, primary storage choices,
   Java releases, native/compatibility emitters, conflicting importer evidence,
   fixed managed layout, derived/chosen prerequisites, and missing prerequisites.
8. **Evolution matrix** — add/rename/type/null/drop/index, retire
   preserve/drop, revive, source-evidence digest changes, and refusal paths.
9. **Ownership tests** — reader collision, adoption, each ejection boundary,
   reverse-ejection refusal, and downstream ABI checks.
10. **CLI equivalence** — for every generator scenario, a CLI-authored model and
   a hand-authored JDL model link to byte-identical canonical linked models and
   plans.
11. **End-to-end corpus** — fresh compile, idempotent second compile, generated
    Java build/tests, sealed plan replay, stale-plan refusal, and crash recovery.
12. **Fuzz/property tests** — parser never panics, formatter terminates, stable
    ID derivation is deterministic, and arbitrary failed edits leave source
    unchanged.

The complete example in section 4 is an executable conformance fixture. Every
smaller `jdl` fragment is parsed in a minimal synthetic document appropriate to
its context. Documentation examples MUST be extracted in CI rather than copied
into disconnected test strings.

## 22. Upgrade from the pre-v1 draft

A file without `jdl 1` is parsed only by the legacy importer. It is never
silently interpreted as v1. `jails model upgrade --to 1` produces a diff and
requires normal review/apply before replacing the source.

The mechanical translations are:

| Legacy draft | JDL v1 |
|---|---|
| unbraced `application` plus following properties | braced `app { ... }` |
| root `package com.example` | app property `pkg com.example` |
| declaration `@package(...)` | no syntax; plan a canonical layer move or eject the implementation boundary |
| `dialect postgresql` | `storage postgres` |
| `dialect h2` | `storage h2` |
| `capability db` or alias `postgres` | removed after materializing `storage postgres` |
| `capability h2` | removed after materializing `storage h2` |
| `capability <kind>` | `cap <kind>` |
| `dependency group:artifact ...` | `dep group:artifact ...` |
| `setting key = value` | `prop key = value` |
| `use repository` | `use repo` |
| entity `@scaffold` | `use scaffold` inside the entity |
| `string!` | required `string @notBlank` |
| `index (a, b desc)` | `index [a, b desc]` |
| `capability json @name(Orders) @package(io.orders)` | `cap json Orders`; plan a canonical placement move or eject an incompatible reader unit |
| dependency `group:artifact = "version"` | `dep group:artifact @version("version")` |
| route property with an unquoted path | `route METHOD "path"` |
| generator-specific top-level unit | matching projection, operation, relation, or typed `component` |
| inferred `--via` relation | explicit `join` or `resolve` mapping |

The legacy file does not contain the new `platform` and `build` axes. During
upgrade, the importer MUST inspect the selected module once and materialize
`platform spring|plain` and `build maven|gradle` in the `app` block. A module
with conflicting build evidence, an unsupported build language, or ambiguous
platform evidence aborts upgrade with a diagnostic; it is never guessed.

The upgrader preserves comments, source order, explicit IDs, logical names,
physical names, and operation routes. It materializes effective legacy IDs
before changing syntax. Non-conventional generated packages are not copied as
new JDL choices: the plan lists a canonical move for unchanged managed code or
requires ejection/adoption when reader changes prevent that move. If legacy
inference has multiple possible targets or an old free-form value has no typed
v1 equivalent, upgrade aborts with all candidate spans; it never chooses one.

Legacy TOML model state is imported into the same v1 AST through a separate
one-shot command. It does not become an alternate source format. After a
successful accepted upgrade, the compiler writes the language version to the
accepted-model metadata and no longer invokes the legacy parser.

## 23. Deliberate non-goals and future additions

JDL v1 intentionally has no:

- includes, imports, macros, templates, or conditional declarations;
- environment overlays or secret values;
- arbitrary Java annotations, Java expressions, SQL expressions, or build XML;
- per-declaration packages, configurable suffixes/plurals, route styles, test
  templates, migration names, or other naming profiles;
- implicit many-to-many relations or ORM navigation collections;
- migration history embedded in the desired-state file;
- plugin-defined unnamespaced keywords or attributes;
- automatic reverse adoption of ejected code; or
- multi-file partial declaration merging.

These omissions keep name resolution, ownership, diffs, and safety review
deterministic. Repetition that proves common should first become a typed
projection/profile or component kind in the shared registry. A capability or
component that cannot be modeled safely can still expose a managed port and an
explicit ejected implementation boundary.

Future versions can add a construct only with its grammar, typed linked-model
payload, validation schema, stable-ID rule, ownership boundary, formatter
behavior, CLI mapping, upgrade rule, and conformance tests. Syntax alone is not
a language feature.

## 24. Implementation acceptance checklist

JDL v1 is ready to ship when:

- every example parses and formats idempotently;
- every current Jails generator/capability/option passes the exhaustive mapping
  tests in section 21;
- a hand edit and its equivalent CLI command yield the same linked model and
  exact plan;
- every managed package, Java type/file, SQL name, route, test, and migration
  name matches the versioned convention snapshots with no naming configuration;
- `model explain` names the rule and source declaration behind every derived
  projection and prerequisite;
- no invalid model writes generated files or migrations;
- every destructive storage transition requires exact evidence;
- comments and untouched source spans survive CLI edits;
- every generated or reader file has one recorded ownership boundary; and
- a second compile of unchanged source produces an empty plan.

This is the intended balance: authors state the domain and behavior, Jails owns
the repetitive source-code decisions, rare features remain available through
typed components, and real storage/public-contract/ownership exceptions stay
explicit enough to plan safely.
