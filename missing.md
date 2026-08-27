# missing.md — what real minicom projects needed and jails cannot give

Every entry here is a line that could not be written with the CLI, found by
rebuilding a real application with **only** `jails` commands — no editor, no
hand-written Java, no hand-edited pom. Nothing here is a proposal for its own
sake.

**A closed entry is *deleted* from this file, not marked done.**
`git log -p -- missing.md` is where a closed one and the run that closed it
live. Numbers are stable and never reused, so an `missing.md M6` citation in
the source still resolves to a subject.

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
scope line — an admin UI is a product, not scaffolding — but it is the one
thing every Django port gets for free and every jails port does not, and
`jails.toml` plus the ledger already know the field model such a view would
need. Recorded so the decision is a decision rather than an omission.

---

## What the closed entries taught, kept because it outlives them

**jails models a resource extremely well and a conversation not at all.** M4,
M5 and M6 were one missing idea from three angles — a participant identified
by a natural key, a stream of messages between participants, and presence —
and every ported project needed all three. They are closed as three generic
primitives (`--on-conflict`, `--via`, `socket`/`presence`), not as a chat
feature: `app.rs` is domain-blind on purpose and stays that way.

**The contract half was four knobs, not a design commitment.** A route path
(M8), a form binding (M15), an enum's wire value (M14) and an optional filter
(M16) were the whole reason a project could model a domain perfectly and match
**zero** of the ten endpoints its shipped frontend calls. None of them asked
jails to understand anything new, and they are what separates "scaffolds a new
service" from "can be pointed at an existing client".

**The defects here were invisible to the suite for one reason.** M1 and M2 each
needed a *second* thing present — a scaffold and a strategy in one project, a
Boot version the fixture does not use — and the golden scenarios exercise one
kind on one flavour. Goldens compare bytes and never run the code, so the
project's own build is the oracle that finds this class. It still is: every
defect in `bugs.md` came from running something rather than from reading it.

---

## Where the evidence lives

The rebuilt projects are at `/home/laith/code/minicom-jails/`, each a plain
Maven project — `cd` into one and `jails build`. The command log per project is
its `.jails/ledger.toml`; `jails history` prints it. The untouched originals
are under `minicom/`, and `minicom/minicom-15-01-2026` is the checkout with
four backends and two hand-written frontends that `plan.md` P10 is driven by.
