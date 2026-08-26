# minicom.md — implementation guide

Q&A. Each question is the interviewer prompt; the answer is what to build.
Backend on `:3000`, customer site `foo-website` on `:8008`, agent site
`bar-website` on `:8009`.

---

## What am I given?

Two tables, already declared, and nothing wired to them:

```sql
users     (id, email, created_at, updated_at)
messages  (id, user_id, content, message_read, created_at, updated_at)
```

Plus `POST /foo` and `POST /bar`, both returning `{"success": true}`, and two
websites that do nothing but call them and `alert()`.

Per backend, what exists:

| backend | tables | models | note |
|---|---|---|---|
| node | migrations | **`User` + `Message` wired** | best start |
| spring / spring-4 | `schema.sql` applied on boot | none | Boot 2.7 Gradle / Boot 4.1 Maven |
| rails | `schema.rb` **empty** | none | write your own migration |
| django | **no `models.py`** | none | write everything |

Two hints are hiding in that schema: **`message_read` exists and nothing uses
it** (read receipts are on the list), and **there is no sender column** — so
`user_id` is ambiguous and you must decide what it means in Q2.

---

## What schema should I land on, before I write anything?

This one. It absorbs every question below without a second migration.

```sql
users     (id, email)
messages  (id, user_id, sender, content, message_read, created_at)
             │          │
             │          └── 'CUSTOMER' | 'ADMIN' | 'BOT'
             └── whose conversation this is (the thread key), NOT who typed
```

`user_id` = the customer the thread belongs to. `sender` = who typed. That
split is the whole trick: an agent reply has `user_id = <the customer>` and
`sender = 'ADMIN'`, so threading works from message one and Q3 needs no
migration.

Do **not** make `user_id` mean "who typed". That is the choice that forces a
migration when multiple customers arrive.

---

## Q1 — "Make a customer able to send a message, and the agent able to see it."

**Endpoints** — replace the two stubs:

```
POST /messages   {user_id, sender, content}   -> 201 {message: {...}}
GET  /messages                                -> 200 {messages: [...]}
```

Serialize each message as `{id, user_id, sender, content, created_at}`.

**Frontend.** Both sites currently have a constructor that fires one POST and
alerts. In each: an input, a send button, a rendered list, and
`setInterval(() => this.refresh(), 3000)`. Escape the content before injecting
it. Leave the CSS alone.

**Validate** — `content` non-empty after trim, `user_id` a positive number.
Return `400 {error: "..."}`, don't 500.

**Done when:** type in foo, see it in bar within 3s.
**Deadline: minute 15.** Everything else depends on this.

---

## Q2 — "Now let the agent reply, and the customer see it."

This is the stated first task and it is nearly free if you took the schema
above: the agent posts the same body with `sender: 'ADMIN'`.

**Render by sender, not by position** — customer messages right-aligned on foo
and left-aligned on bar, and vice versa. Getting this backwards on one side is
the visible bug.

**Done when:** both sides show both messages, each attributed to the right
party.

---

## Q3 — "We have many customers now. Let the agent pick a conversation."

No schema change. `user_id` is already the thread key.

**Inbox list.** Derive it from the messages you already fetched rather than
adding an endpoint:

```js
const threads = [...new Set(messages.map(m => m.user_id))].sort();
```

Add `GET /users` only if you need emails for the labels.

**Two panes:**

```
┌──────────────────┬────────────────────────┐
│ > alice@ex.com   │ Chat with alice@ex.com │
│   bob@ex.com     │ alice: Hello           │
│   (last msg,     │ me:    Hi Alice        │
│    40 chars)     │ [ Reply… ] [Send]      │
└──────────────────┴────────────────────────┘
```

Four rules worth copying:
- auto-select the first thread on load, so the pane is never empty
- disable the input until a thread is selected
- preview = last message truncated to ~40 chars
- the agent's POST must carry the **selected** `user_id`, not the sender's

**Server-side, reject the ambiguity:** if `sender == 'ADMIN'` and `user_id` is
missing, `400`. An agent message with no thread is unroutable.

**Say this:** "I'm keying the thread off `user_id` because it's free. In a real
build I'd add a `Conversation` entity — assignment, status and SLA need
somewhere to live." Choosing the cheap version deliberately reads better than
shipping it silently.

**Done when:** A and B both send; two threads appear; replying in A's does not
show in B's.

---

## Q4 — "Agents want to know whether the customer saw their message."

Use the `message_read` column that was already there.

```
POST /messages/read   {user_id, reader}   -> marks the OTHER party's messages read
```

```sql
update messages set message_read = true
 where user_id = ? and sender <> ? and message_read = false;
```

**The `sender <> ?` is the whole thing.** You mark the messages you *received*,
not the ones you sent. Marking your own is the easy bug and it makes every tick
double instantly.

**Trigger it on conversation opened** — the customer when the chat window is
opened (and on an inbound message only if it is already open), the agent when a
thread is selected. Render `✓` for sent, `✓✓` for `message_read`.

**Have the answer ready** for what "read" means:

```
fetched < rendered < conversation opened < window focused < acknowledged
```

Say which you picked. "Conversation opened" is the defensible one.

**Bonus, cheap:** unread count per thread in the inbox list — one `filter` on
data you already have.

---

## Q5 — "People have to refresh. Improve that."

**Do not lead with WebSockets.** Polling at 3s is a legitimate answer and
finishing it beats a half-built socket.

If you go real-time, one room per customer, role in the query string:

```
ws://host/ws/chat/<email>/?role=customer|admin

server on connect:  {"type":"history","messages":[…]}
client sends:       {"message":"…","sender":"CUSTOMER"|"ADMIN"}
server broadcasts:  {"type":"message","id":…,"message":"…","sender":…,"timestamp":…}
```

Sanitise the email into the room name: `re.sub(r'[^a-zA-Z0-9._-]', '_', email)`.
The agent opens a socket per selected thread and closes the previous one.

**Say this:** "I started with polling to get the flow working end to end. The
message model doesn't change when I swap the transport — only the fan-out."

---

## Q6 — "Customers can't tell if anyone is there. Show agent availability."

Track connected admins per room; broadcast on connect and disconnect.

```
server -> {"type":"admin_status","online":true|false}
```

Green dot = an agent is connected, grey = nobody. Send the current status to
each new connection too, or the first paint is wrong.

**Know the limitation before they ask.** An in-memory set is correct on one
process and silently wrong on two — no error either way. The scalable version
is a heartbeat: `last_seen_at > now() - 30s` in the database or Redis.

---

## Q7 — "If no agent is available, have something reply until a human joins."

One boolean decides who answers. Add `agent_joined` (on the conversation, or
derive it: "has any `ADMIN` message in this thread").

```python
save(customer_message)
if not agent_joined(user_id):
    save(bot_reply(content), sender='BOT')
```

The agent's first reply flips it, and the bot goes quiet permanently.

```
UNASSIGNED ──no agent──> BOT_HANDLING ──agent replies──> HUMAN
```

**Make the bot a keyword table**, not a model call: greeting / order / refund /
damaged / pricing / thanks / fallback. If you do call an LLM, wrap it in
try/except and fall back to the table — the deterministic path must be the one
that always works.

---

## Q8 — "Escalate what the bot can't solve, with enough context to take over."

```sql
issues (id, user_id, issue_summary, conversation_summary, status, created_at)
        status ∈ OPEN | IN_PROGRESS | RESOLVED
```

`GET /issues` feeds an "Escalated Issues" panel on the agent dashboard.

If an LLM drives it: instruct it to emit, once, on the last line:

```json
{"action":"escalate","issue_summary":"…","conversation_summary":"…"}
```

Then `rfind('{"action"')`, parse from there, create the issue, and **strip the
JSON out before broadcasting the text to the customer.** Rebuild the model's
context from the database each turn and exclude `ADMIN` messages from it.

Call that protocol what it is — scrappy — and name the real answer (tool call /
structured output) rather than defending it.

---

## Q9 — "What would you build next?"

Answer in dependency order, not as a wishlist:

```
1. Conversation isolation      nothing matters if threads leak
2. Real-time delivery          polling is visibly a prototype
3. Read / unread state         the agent needs a work queue
4. Assignment + status         needs a Conversation entity
5. Auth                        there is none at all today
```

Not: refactor, rewrite the CSS, add animations, introduce Kafka.

Keep a `todo.txt` as you go — one line each time you knowingly cut a corner.
It costs nothing and it is the whole back half of the conversation.

---

## Timebox

45-minute sprints, requirements fed one at a time.

```
0–03   which backend, does it have models, where is the schema
03–15  Q1  message persists, both sites render      ← hard deadline
15–25  Q2  sender discriminator, agent replies
25–38  Q3  thread key + two-pane inbox
38–45  Q4 read receipts, or Q5 sockets — pick one, finish it
```

Two rules: **don't start with WebSockets**, and **don't start with a
`Conversation` entity.** Get `POST → persist → GET → render` working, then
extend.

## Prepare in advance

Four small functions, not a framework, so minute three is
*route → validate → persist → respond* rather than remembering your web
framework:

```
requireFields(payload, [...])   -> {valid, missing}
jsonResponse(body, status)
asyncHandler(fn)                -> catches, returns 500 instead of hanging
errorHandler                    -> {error: "..."} not an HTML stack trace
```

Also know cold: your framework's migration command, and how to nuke and
recreate the database when a migration goes wrong.
