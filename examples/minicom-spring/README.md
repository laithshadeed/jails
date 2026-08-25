# minicom-spring

`minicom-public/spring` — the Gradle interview scaffold — stood up from
nothing.

**Not `examples/minicom/`.** That one ports the Rails and Django minicom
*domain* (users, messages, read flags, a direction enum) onto Spring, and is
about jails' generic intents covering a real application. This one is about the
skeleton those interviews start from, and about a much narrower question: can
jails create the project at all? Until `--gradle` it could not — `jails new`
hardcoded `type=maven-project` in its start.spring.io request, and nothing in
jails wrote a `build.gradle`.

## One command

```
jails new spring --gradle --boot 2.7.18 --java 21 \
  --package com.intercom.spring \
  --jar-name gs-rest-service --jar-version 0.1.0 \
  --deps web,data-jdbc,h2 \
  --app examples/minicom-spring/.jails/app.toml
```

Verified end to end: `./gradlew build` on the result is `BUILD SUCCESSFUL`
against a real Gradle 8.5 and a real JDK 21, with 8 tests collected, 5 executed
and 0 failures. The other 3 are the `@Disabled` stubs `g controller` emits for a
handler nobody has written yet.

That verification is the point of this example rather than a footnote. The
first run of it was green over **zero** tests: the Boot 2 Gradle plugin does not
configure `useJUnitPlatform()`, so Gradle ran the JUnit 4 provider, found no
JUnit 5 test, and reported success. Same failure as the Failsafe one in the
root `README.md` — a green build claiming it passed something it never ran. The
legacy template writes the `test { useJUnitPlatform() }` block for that reason
and says so in a comment.

## What matches, and what does not

Matching the target: the `buildscript {}` block and its Boot 2.7.18 plugin
classpath, the five `apply plugin:` lines, `bootJar`'s
`archiveBaseName`/`archiveVersion`, `sourceCompatibility`/`targetCompatibility`,
`settings.gradle`, the wrapper, `Application.java`, `ApplicationTests.java`, and
two `@PostMapping` controllers in `com.intercom.spring.controllers`.

Deliberately different:

- **Whitespace.** The target's `dependencies {}` block mixes tabs and spaces
  and its `bootJar` has a double space, both artefacts of hand-editing, and its
  files have no trailing newline. A generator that reproduced those would put
  one project's typos into every Gradle project jails ever writes.
- **`gradle-wrapper.jar`.** 55,616 bytes in the target, written by
  `gradle wrapper`; jails fetches Gradle's own 43,462-byte bootstrap jar from
  its repository at `v8.5.0`, because no standalone coordinate for the
  generated one is published. Both launch
  `org.gradle.wrapper.GradleWrapperMain`, which reads the properties beside it
  and fetches the distribution they name; the jar is not tied to that version.
- **The distribution.** `-bin.zip` rather than the target's `-all.zip`. `-all`
  ships sources and docs for IDE completion and costs a much larger download.
- **CORS.** The target's `WebConfig` is `@EnableWebMvc` plus
  `addMapping("/**")` — no origin list. `add cors` writes the credentialed form
  with explicit origins, which is the shape that keeps working the first time
  somebody sends a cookie.
- **JSpecify.** jails adds `org.jspecify:jspecify` because every generator
  writes a null-marked `package-info.java`.

Not expressible at all, and left to the reader:

- **`schema.sql` and `spring.sql.init.mode=always`.** jails' database story is
  forward-only migrations, not a schema script re-applied at every start. The
  target's `users` and `messages` tables have no Java behind them in this
  project — they are the shared interview schema — so there is nothing to
  scaffold from either.
- **`server.port=3000`** and **`jdbc:h2:file:~/minicom`**. `add h2` writes
  `jdbc:h2:file:./data/app` on the default port, deliberately: a database under
  `~` is one two checkouts of the same project silently share.
