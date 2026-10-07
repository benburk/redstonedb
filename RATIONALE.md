# Rationale

## Why fixi.js instead of React or Vue

SPAs split every feature across two codebases communicating over JSON APIs. Adding a feature requires updating the backend serializer, API schema, TypeScript interfaces, and frontend deserializer. Frontend and backend deploy independently, requiring API versioning and backwards compatibility layers.

fixi.js swaps HTML returned by the server. Features live in one place and deploy as one artifact. No JSON serialization, no API versioning, no coordinating changes across deployments. Breaking changes are non-issues because there's no separate client lagging behind.

## Why server-owned state instead of client-side state management

SPAs require client-side state that mirrors server state, spawning libraries like React Query and Redux solely to manage cache invalidation, optimistic updates, and refetch logic. You solve stale data refresh timing and concurrent modification handling across tabs.

With fixi.js, the server owns all state. Every interaction fetches current state. No cache invalidation, no optimistic updates that need rollback, no stale data bugs from client-side state diverging from reality.

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

## Why PicoCSS instead of Tailwind

Tailwind requires a build pipeline: watching files, generating CSS, configuring purging. Development includes debugging why styles aren't being generated. Every element needs explicit utility classes.

PicoCSS styles semantic HTML directly. A `<button>` looks like a button without classes. A `<table>` looks like a table. No build step, no configuration. The tradeoff is less customization, but for CRUD applications the defaults are sufficient.

## Why mpsc channels instead of Redis for background jobs

Redis adds another service to deploy, monitor, and maintain. Connection failures between your app and Redis become a failure mode. Job serialization crosses process boundaries.

Tokio mpsc channels keep jobs in-process. No serialization, no network, no additional infrastructure. Jobs are Rust enums with full type safety. The tradeoff: jobs don't survive process restarts and can't distribute across machines. For single-server applications, this is acceptable.

## Why single-binary deployment instead of containers

Containers add layers: base images, Dockerfiles, registries, orchestration. Each layer has configuration, versioning, and failure modes. Local development often differs from production container behavior.

A single Rust binary contains everything: HTTP server, business logic, SQLite, and templates. Deployment is copying one file. Rollback is copying the old file back. Local development runs the exact same architecture as production.

## References

- [I'm All-In on Server-Side SQLite](https://fly.io/blog/all-in-on-sqlite-litestream/)
- [The HARM Stack Considered Unharmful](https://nguyenhuythanh.com/posts/the-harm-stack-considered-unharmful/)
- [Server-Informed UI](https://max.engineer/server-informed-ui)
