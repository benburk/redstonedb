# Rationale

## Why server-rendered HTML instead of React or Vue

SPAs split every feature across two codebases communicating over JSON APIs. Adding a feature requires updating the backend serializer, API schema, TypeScript interfaces, and frontend deserializer. Frontend and backend deploy independently, requiring API versioning and backwards compatibility layers.

Here every page is HTML rendered by the server, and every change is a form the server answers with the next page. Features live in one place and deploy as one artifact. No JSON serialization, no API versioning, no coordinating changes across deployments. The server owns all state, so there is no client-side cache to keep in step with it. Where a change touches only part of a page, fixi sends the form and swaps in the fragment the server renders for it, so the server still owns the HTML; a little script only enhances a form, such as turning typed names into pills, and the page works without either.

## Why SQLite instead of Postgres

Postgres means deploying two services, managing separate servers, configuring authentication and firewall rules, implementing connection pooling, and monitoring multiple processes. Every outage could be application or database.

SQLite is a file. No separate server, no authentication to configure, no firewall rules, no connection pooling, no second process to monitor. For single-server applications with read-heavy workloads, the entire operational burden disappears.

## Why in-process SQLite instead of a database client

Postgres queries carry 0.5-1ms of network latency per query even within a single AWS region. A dozen database calls per request costs 6-12ms before business logic runs.

SQLite queries execute as function calls within your process. Simple lookups complete in microseconds. You spend your latency budget on business logic and external API calls rather than fighting database round-trips.

## Why Rust instead of Python or TypeScript

Python and TypeScript excel at writing the first version but struggle as understanding evolves. Change a dictionary key and your editor won't catch every reference. Rename a class attribute and you'll find bugs at runtime.

Rust makes refactoring mechanical. Change a struct field and every reference breaks at compile time. Add an enum variant and pattern matches become non-exhaustive until you handle the new case. The compiler guides you through every consequence of a change.

## Why MAUD instead of Askama or Tera

String-based templating (Askama, Tera, Jinja) separates HTML from Rust code. Template syntax errors surface at runtime or require separate compile steps. Variable bindings between Rust and templates are stringly-typed.

MAUD embeds HTML directly in Rust using macros. Template errors are compile errors. Variables are just Rust variables with full type checking. Refactoring a struct field updates templates automatically through compiler errors.

## Why single-binary deployment instead of containers

Containers add layers: base images, Dockerfiles, registries, orchestration. Each layer has configuration, versioning, and failure modes. Local development often differs from production container behavior.

A single Rust binary contains everything: HTTP server, business logic, SQLite, templates, static files, and migrations. Deployment builds it on the server and swaps it in, keeping the old one beside it. Rollback is moving the old one back. Local development runs the exact same architecture as production.

## References

- [I'm All-In on Server-Side SQLite](https://fly.io/blog/all-in-on-sqlite-litestream/)
- [The HARM Stack Considered Unharmful](https://nguyenhuythanh.com/posts/the-harm-stack-considered-unharmful/)
- [Server-Informed UI](https://max.engineer/server-informed-ui)
