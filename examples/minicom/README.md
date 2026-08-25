# Minicom, ported from Rails and Django

Three implementations of one application — `minicom-rails`, `minicom-django`
and `minicom-django-master` — expressed as a single jails manifest and applied
to Spring Boot with one command:

```
jails new minicom-spring --offline --app examples/minicom/.jails/app.toml
```

## They are one app, not three

That is the finding, and it is what makes the port a single manifest. The
schemas agree field for field:

| | Rails `db/schema.rb` | Django `models.py` | manifest |
|---|---|---|---|
| user | `email`, unique index | `EmailField(max_length=254)` | `email:string!@unique` |
| message → user | `user_id`, indexed | `ForeignKey(User)` | `userId:long@index` + `association` |
| body | `t.text "message"` | `TextField()` | `body:string!` |
| read flag | `t.boolean "is_read"` | `BooleanField(default=False)` | `isRead:boolean` |
| timestamp | `t.datetime`, indexed | `DateTimeField(db_index=True)` | `timeStamp:instant@index` |
| direction | `t.string` | `TextField(default='TO_USER')`, `CharField(max_length=1)` | `enum MessageDirection` |

`minicom-django-master` differs from `minicom-django` only in spelling —
`CharField(max_length=1)` for direction, `auto_now_add` rather than `auto_now`.
Neither difference reaches the domain.

The routes agree too: `POST /api/ping`, `/api/read`, `/api/send`, and a
conversation read. Rails adds `send_admin` and `last`; Django adds an admin
page. Those are the same three intents with a fourth caller.

## What the port is worth

Nothing in the manifest names Rails, Django, or minicom. `scaffold`, `enum`,
`association`, `usecase`, `transition` and `query` are the same generic intents
the crawler, the support inbox and the payments gateway are built from — which
is the claim `app.rs` makes about being domain-blind, tested against a domain
nobody designed for it.

**Verified**: `jails check` is BUILD SUCCESS — 44 unit tests and 6 integration
tests against a real PostgreSQL through Testcontainers, none skipped.

## What jails asked for that neither framework had

One thing, and it is worth stating because it is a real improvement rather than
a tax: `transition MarkAsRead` **refuses without a `version` column**. Django's
`save()` and Rails' `update` are last-write-wins, so two readers marking the
same message read at once silently lose one update. The port cannot express
that bug.
