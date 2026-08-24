# Working on this project

This project targets Java 25. Its base package is `com.example.paymentsgateway`.

## Commands

- Run one test with `jails test <Name>` or place the cursor in it and pass `path:line`.
- Run `jails check` before handing work off. It performs a clean Maven verify so stale class files cannot hide a regression.
- Run `jails doctor` before debugging the machine, container runtime, ports, or dependency injection.
- Use `jails g <kind> --pretend` and `jails add <capability> --pretend` to inspect changes.

## Design

- Domain values are immutable records. Use no ORM and no Lombok.
- Persistence is a repository port plus an explicit JDBC adapter and forward-only SQL migrations.
- Field specs use `name:type`, `name:type!` for non-blank text, `name:type?` for nullable values, and `@pk`, `@unique`, `@index`, or numeric constraints where applicable.
- Keep domain, application, service, adapter, web/API, messaging, job, and testkit packages separate. `jails about --json` reports the configured names.
- Generated applications must remain operable without jails installed.

## Checked stale APIs

`jails lint` enforces this exact list:

- Never use `@MockBean`; use @MockitoBean because the former Spring Boot test annotation is deprecated.
- Never use `javax.validation`; use jakarta.validation because current Spring uses the Jakarta namespace.
- Never use `spring-boot-starter-web</artifactId>`; use spring-boot-starter-webmvc because Boot 4 splits the MVC starter explicitly.
- Never use `@Entity`; use a record plus a generated repository port and explicit JDBC adapter because jails projects use explicit SQL rather than an ORM.
- Never use `lombok.`; use records or explicit Java because generated methods hide the API from compiler and editor checks.
- Never use `--enable-preview`; use a non-preview Java API because generated applications must run on a standard release toolchain.
