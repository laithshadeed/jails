# Missing Features & Dogfooding Gaps in Jails CLI

Identified during the end-to-end implementation of **Minicom 2.0** (`minicom/prompt.md`) in `minicom/minicom-15-01-2026-playground`.

---

## 1. Capabilities & Dependency Management

### Missing `websocket` in `jails add`
- `jails g socket <Name>` generates a `TextWebSocketHandler` and `WebSocketConfigurer`, but `jails add websocket` is rejected as an invalid capability.
- `jails add sse` is supported, but bidirectional WebSockets require manual dependency additions on Gradle/Maven projects when not using `g socket`.
- **Expected**: `jails add websocket` should configure `spring-boot-starter-websocket` in `build.gradle` / `pom.xml` and record `websocket` in `[project] capabilities`.

### Gradle Parity for `jails fmt`
- `jails fmt` only supports Maven sandboxes and refuses Gradle projects with:
  `fix: ./gradlew spotlessApply -- jails add format has already configured it`
- **Expected**: `jails fmt` should invoke `./gradlew spotlessApply` automatically on Gradle projects rather than refusing.

---

## 4. Code Generation & Architecture Helpers

### Dual-Format `consumes = [json, form]` Request Support
- Current generators support `--consumes json` or `--consumes form`, but real-world web applications (like Minicom with jQuery `$.post` and API clients) frequently require endpoints that accept both form-urlencoded and JSON payloads without returning HTTP 415.
- **Expected**: Generator support for hybrid request binders or unified payload parsing.

### In-Memory / Room-Based Presence Generators
- `jails g presence` generates PostgreSQL cluster-backed presence, but lightweight in-memory group/room chat presence (e.g. admin online tracking per customer email channel) is a common pattern that lacks a generator recipe.
- **Expected**: A `socket-presence` recipe for room-based presence and lifecycle events.
