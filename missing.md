# missing.md — what eight real minicom projects needed and jails could not give

Written 2026-08-26. `CLAUDE.md` says an earlier `missing.md` was folded into
`pending.md`; `pending.md` is itself gone now (deleted in `2f8003b`), so the
live open-item files are `bugs.md` and `research.md`. This file is the same
kind of document as the original: **what one set of real migrations needed and
did not get.** Nothing here is a proposal for its own sake — every entry is a
line I could not write with the CLI.

## What was built

Every project in `minicom/` dated 2025 or 2026 was rebuilt from scratch as a
Spring Boot 4.1 / Java 26 service using **only** `jails` commands — no editor,
no hand-written Java, no hand-edited pom. Sources are at
`/home/laith/code/minicom-jails/`.

| project | original stack | jails project | result |
|---|---|---|---|
| `minicom-05-02-2026` ("Minicom 2.0") | Django 5 + Channels + OpenAI | `minicom-2026-02-05` | **green**, 68 tests |
| `mc-01-06-2026` | Node 24 + TypeScript + Sequelize | `minicom-2026-06-01` | **green**, 34 tests |
| `mc-public-09-01-2026` | Django 5 | `minicom-2026-01-09` | **green**, 34 tests |
| `mc-13-12-2025` | Django 5 / Node / Rails / Spring skeleton | `minicom-2025-12-13` | **green**, 9 tests |
| `mc-21-11-2025` | Django + Channels, read receipts | `minicom-2025-11-21` | **green**, 32 tests |
| `mc-16-11-2025` | Django + Channels + HF bot | `minicom-2025-11-16` | **RED** — M1, now closed |
| `minicom-05-02-2026`, second deeper pass | Django + Channels + OpenAI | `mc2-full` | **green**, 70 tests |
| `minicom-15-01-2026` (4 backends) | Django / Rails / Node / Spring | `mc-15-01` | **green**, 70 tests, **0 of 10 endpoints match** |

A seventh check ran `jails` inside the *unmodified* `mc-01-06-2026/spring`
tree — Gradle 8.5, Spring Boot 2.7.18, Java 21 — to test the foreign-project
path. That was M2, **now closed**: `add db` reads the project's Boot version
and picks the module set that version has. Boot 4 gets `spring-boot-flyway`;
Boot 3.1 upwards gets Flyway's auto-configuration where it always was, with
both Flyway artifacts pinned to one version; below 3.1 it refuses by name,
saying which module is missing and which capabilities do work. Each of the
three boundaries was checked in `deps/spring-boot`, not recalled.

**M1 is closed.** The port stays in `domain` and its beans moved to `service`,
so the ArchUnit rule `g scaffold` writes and the `@Component` `g strategy`
needs stop contradicting each other; `--on`/`--yields` reach both through
`import_of`, which is what makes `--package` compile; and a `destroy` that
cannot find a `--package`-placed resource now names the package it was
recorded under instead of reporting it as never generated. The reason the
suite could not see it is closed too — the shared Spring toolbox generates a
scaffold *and* a strategy into one project and runs `mvn test` over both, and
its cache is salted with the harness text so adding a step to that list
actually runs it.

The domain modelling held up well. `g scaffold`, `g enum`, `g association`,
`g query`, `g transition`, `g usecase`, `g strategy`, `add db/api/cors/json/sse`
and `set` covered most of every app's schema and CRUD surface, and five of six
reached `BUILD SUCCESS` with no file touched by hand. What follows is the
remainder.

---

## Case study A — Minicom 2.0, feature by feature

`minicom-05-02-2026/django/minicom/`: 13 Python files, 671 lines, doing
customer↔admin chat over WebSockets with admin presence, an OpenAI assistant
that takes over when no admin is online, and escalation into an issue queue.
The jails rebuild is `minicom-jails/mc2-full` — 106 Java files, 4,646 lines,
`BUILD SUCCESS`, 70 tests, nothing hand-written.

Taken feature by feature rather than stopping at the schema:

| what the Django app does | jails | how |
|---|---|---|
| `User`, `Message`, `Issue` + their enums | yes | `g scaffold`, `g enum` |
| FKs `message.to_user`, `issue.user` | yes | `g association` |
| `User.get_or_create_from_email` | **no** | **M6** — used by 4 of the 5 endpoints |
| `unread_messages()` / `unread_count()` | partial | list yes; `count()` no — **M5** |
| `Message.mark_read()` | partial | `g transition`, after adding `version` — **M11** |
| `POST /customer_api/ping {email}` | **no** | get-or-create **and** a join on email — M5, M6, M8 |
| `POST /customer_api/read {email, message_id}` | partial | keyed by id, not email |
| `POST /admin_api/messages {email, content}` | partial | keyed by `toUserId`, not email |
| `GET /admin_api/users` | yes | the scaffold's own `GET /users` |
| `GET /admin_api/issues` with `user_email` joined | **no** | `select_related('user')` — **M5** |
| 400/404 error bodies | yes | `add api` |
| Django admin at `/admin/` | **no** | **M18** — no back-office surface |
| CORS | yes | `add cors` |
| `ws/chat/<email>/?role=`, groups, `history`, broadcast | **no** | **M4** |
| admin presence + `admin_status` broadcast | **no** | **M4** |
| bot greeting / "no admin → let the AI answer" | **no** | domain logic |
| `POST /v1/chat/completions` to OpenAI | **no** | **M7** |
| `SYSTEM_PROMPT`, history mapping, `parse_escalation` | **no** | domain logic (fair) |
| create the `Issue` on escalation | yes | `g usecase … --on Issue` |
| `OPENAI_API_KEY` from the environment | partial | `jails set`; no secret-shaped setting |

**10 covered, 6 partial, 14 not.** By weight the split is cleaner than the
count: **jails built the entire persistence and CRUD half and none of the
transport or conversation half.** The WebSocket rows are one absence (M4) and
the two email-keyed endpoints are another (M5/M6), and between them they are
what makes this a chat product rather than a table with a REST face.

Prompt text and response parsing are honest non-goals. An outbound `POST` with
a typed body is not — see M7.

---

## Case study B — `minicom-15-01-2026`, and what "100% feature complete" costs

This checkout is the pristine old-generation skeleton with **four backends
implementing one contract** — Django, Rails, Node and Spring — plus two static
websites on `:8010` and `:8011` that call it on `:3001`. jails only emits Java,
so the question it can be asked is: *can one jails-built Spring service satisfy
the contract all four implement?*

The contract, as the two shipped websites call it:

```
POST   /customer_api/ping                          form {email}
POST   /customer_api/read                          form {message_id}
POST   /customer_api/messages                      form {email, content, category?, priority?}
GET    /admin_api/users?status=&category=&priority=
POST   /admin_api/messages                         form {email, content}
GET    /admin_api/messages/:user_id
GET    /admin_api/conversations?status=&category=
PATCH  /admin_api/conversations/:user_id/status    json {status}
PATCH  /admin_api/conversations/:user_id/category  json {category}
PATCH  /admin_api/conversations/:user_id/priority  json {priority}
```

`mc-15-01` models the whole domain — `User`, `Message`, `Conversation`, four
enums, two associations, four transitions, four queries, two use cases —
and builds green with 70 tests. Then:

```sh
jails routes
```

```
PUT   /actions/mark-read              POST  /queries/conversations-by-status
POST  /actions/post-message           POST  /queries/messages-for-user
PUT   /actions/set-status             GET   /users
PUT   /actions/set-category           GET   /messages
PUT   /actions/set-priority           GET   /conversations
…22 routes
```

**Zero of the ten match.** Not one path, and the ones that are close are the
wrong verb. The two websites cannot talk to it at all, and they are not files
you are allowed to rewrite — they are the fixed side of the contract.

Four separate causes, three of them new:

- **the paths are derived** — M8

This is the clearest answer available to "can jails do 100%": for a service
with an existing client, **the domain half is free and the contract half is
unreachable**, and that is a property of four missing knobs rather than of any
deep design commitment.

---

## M12 — a fully applied transaction exits 1 because an external effect failed

`add db` writes every file, records the ledger, and then returns a failure
status because it could not start the compose service.

```sh
jails new p --package com.x --offline --no-git && cd p
jails add db ; echo "EXIT=$?"
```

```
  create  compose.yaml
  create  jails.toml
  replace pom.xml
  …
  ledger  create
  effect  compose reconcile (1 up, 0 stopped) (failed)
EXIT=1
```

The project is correct — `compose.yaml`, `jails.toml` and the migration
directory are all there, and a second run says `nothing to do` and exits 0.
Only the side effect failed.

Two things follow. In a script — `for c in db api cors json sse; do jails add
$c || fail; done`, which is how I installed capabilities on six projects —
this reads as a failed install of the one capability that actually succeeded.
And the natural response is to re-run it, which is a no-op that reports
success, so the operator learns to ignore the status.

**`--no-start` exists and fixes it** (`jails add db --no-start` → `EXIT=0`),
but the failure line names neither the cause nor the flag. Everywhere else
jails puts a `fix:` on a refusal; this line has none.

The narrow fix is a distinct exit status for "applied, effect failed" — or, at
minimum, `fix: jails add db --no-start` on that line.

---

## M18 — no back-office surface

Every Django checkout registers its models with the Django admin — list
columns, filters, search fields, read-only keys — in about twenty lines:

```python
@admin.register(Issue)
class IssueAdmin(admin.ModelAdmin):
    list_display = ('id', 'user', 'issue_summary', 'status', 'created_at')
    list_filter = ('status',)
    search_fields = ('user__email', 'issue_summary')
```

```sh
jails commands | grep -icE 'admin|crud|backoffice'    # 0
```

jails generates the REST surface and no operator surface. This is a defensible
scope line — an admin UI is a product, not scaffolding — but it is worth
recording that it is the one thing every Django port gets for free and every
jails port does not, and that `jails.toml` plus the ledger already know the
field model such a view would need.

---

## What this exercise says about the tool

The scoreboard is 7 of 8 green with **zero** hand-written Java, on eight real
apps written by as many people in four languages. The layering, the
field spec, `association`, `query`, `transition` and the write-path rules
(import normalisation, `ensure_failsafe`, `ensure_assertj`,
`ensure_webmvc_test`) all did their job without being thought about, which is
the point of them.

The gaps cluster, and the cluster has a shape. **jails models a resource
extremely well and a conversation not at all.** Every one of M4, M5 and M6
is the same missing idea from a different angle: a participant identified by a
natural key, a stream of messages between participants, and presence. That is
not a request for a chat feature in core — `app.rs` is domain-blind on purpose
and should stay that way. It is the observation that *get-or-create by natural
key*, *read across an association*, and *bidirectional push* are three generic
primitives, and that all six of these projects needed all three.

The second cluster is smaller and cheaper, and it is **closed**: a route path
(M8), a form binding (M15), an enum's wire value (M14) and an optional filter
(M16) were four missing knobs, and all four have shipped. None of
them asks jails to understand anything new. Together they are the whole reason
`mc-15-01` matched zero of ten endpoints while modelling the domain perfectly,
and they are what separates "scaffolds a new service" from "can be pointed at
an existing client".

M1 and M2 were different again: defects rather than absences, and both
invisible to the suite for the same reason — the golden scenarios exercise one
kind on one flavour, and each bug needs a second thing present. Both are
closed, along with the blind spot behind them. **M7, M12 and M13 are the same
species and still open**, and all three share one symptom worth naming: a
command that reports success for something it did not do.

---

## Where the evidence lives

The eight builds are at `/home/laith/code/minicom-jails/`, each a plain Maven
project — `cd` into one and `jails build`. The command log per project is its
`.jails/ledger.toml`; `jails history` prints it.

Every transcript above was produced by running the commands as written, JDK
26.0.2, Maven via `./mvnw`. **M3–M11 were recorded against `9aac1b0`; M12–M18
were recorded and every earlier entry re-checked against `d1e2185`** — the tree
moved under this document while it was being written, which is why the versions
are stated. One thing found on the way is deliberately not filed: between
`3b58d17` and `d1e2185`, `g query` emitted a port whose declared type did not
match its filename, so every query broke the build. `d1e2185` closed it. The M2
Gradle run used `JAVA_HOME=…/openjdk-21.0.2` because Gradle 8.5 cannot run on
JDK 26 (`Unsupported class file major version 70`) — that part is the
checkout's age, not a jails problem.

The M2 module claims were checked against `deps/spring-boot`, not from memory:

```sh
cd deps/spring-boot
git ls-tree -r --name-only v2.7.18 | grep -c spring-boot-flyway          # 0
git ls-tree -r --name-only v2.7.18 | grep -c spring-boot-testcontainers  # 0
```
