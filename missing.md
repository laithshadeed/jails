# Missing Features & Dogfooding Gaps in Jails CLI

Identified during the end-to-end implementation of **Minicom 2.0** (`minicom/prompt.md`) in `minicom/minicom-15-01-2026-playground`.

---

## 1. Capabilities & Dependency Management

### Missing `websocket` in `jails add`
- `jails g socket <Name>` generates a `TextWebSocketHandler` and `WebSocketConfigurer`, but `jails add websocket` is rejected as an invalid capability.
- `jails add sse` is supported, but bidirectional WebSockets require manual dependency additions on Gradle/Maven projects when not using `g socket`.
- **Expected**: `jails add websocket` should configure `spring-boot-starter-websocket` in `build.gradle` / `pom.xml` and record `websocket` in `[project] capabilities`.

### First-Class `devtools` Capability & Auto-Remediation in `jails run --watch`
- `jails run --watch` requires `spring-boot-devtools` to restart the application context on class recompilation. If `spring-boot-devtools` is missing, `--watch` cannot reload running instances.
- Currently, developers must manually diagnose why `--watch` isn't reloading and run `jails add dependency org.springframework.boot:spring-boot-devtools`.
- **Expected**:
  - `jails add devtools` should be a recognized first-class capability.
  - `jails run --watch` should detect when `spring-boot-devtools` is absent and offer to add it automatically or warn with an actionable fix.

### Gradle Parity for `jails fmt`
- `jails fmt` only supports Maven sandboxes and refuses Gradle projects with:
  `fix: ./gradlew spotlessApply -- jails add format has already configured it`
- **Expected**: `jails fmt` should invoke `./gradlew spotlessApply` automatically on Gradle projects rather than refusing.

---

## 2. Code Generation & Architecture Helpers

### Global Exception Handler & Error Scaffold (`jails g advice` / `jails add errors`)
- Spring Boot default error handling routes uncaught exceptions to `BasicErrorController`, producing generic 500 JSON without clear controller exception details or readable terminal stack traces (e.g. `@PathVariable` mismatches or missing URI parameters).
- Developers frequently need a structured `@RestControllerAdvice` that outputs clear debug logs and returns structured error payloads (e.g. RFC 9457 `ProblemDetail` or custom JSON with status, error, and message).
- **Expected**: `jails g advice <Name>` or `jails add error-handler` to scaffold a `@RestControllerAdvice` class with `@ExceptionHandler` methods for common web exceptions, validation binding errors, and fallback uncaught exceptions.

### Extending Existing Controllers (`jails g action` / `jails g route --on <Controller>`)
- `jails g controller <Name>` always creates a new standalone controller file. In traditional Spring projects where related routes live together in one controller (e.g. `MessagesController.java`), there is no CLI command to append an `@GetMapping` or `@PostMapping` handler method into an existing controller class.
- **Expected**: `jails g action <Name> --on <Controller>` (or `jails g method <Name> --on <Controller>`) to safely splice a new handler method and its corresponding MockMvc test into an existing controller.

### Dual-Format `consumes = [json, form]` Request Support

- Current generators support `--consumes json` or `--consumes form`, but real-world web applications (like Minicom with jQuery `$.post` and API clients) frequently require endpoints that accept both form-urlencoded and JSON payloads without returning HTTP 415.
- **Expected**: Generator support for hybrid request binders or unified payload parsing.

### In-Memory / Room-Based Presence Generators
- `jails g presence` generates PostgreSQL cluster-backed presence, but lightweight in-memory group/room chat presence (e.g. admin online tracking per customer email channel) is a common pattern that lacks a generator recipe.
- **Expected**: A `socket-presence` recipe for room-based presence and lifecycle events.

---
