# Repository Guidelines

## Project Structure & Module Organization

This is a Rust 2024 terminal UI application. `src/main.rs` starts the binary and `src/lib.rs` exposes shared modules. Keep responsibilities separated:

- `src/api/`: Bilibili request/response models, client methods, WBI signing, and protocol handling.
- `src/app/` and `src/application/`: application state, actions, runtime loop, network commands, and events.
- `src/ui/` and `src/presentation/`: Ratatui pages, reusable widgets, themes, and input handling.
- `src/infrastructure/`, `src/player/`, and `src/storage/`: media playback, persistence, credentials, and external integrations.
- `screenshots/`: UI examples used by the README.

Unit tests generally live beside their implementation in `#[cfg(test)]` modules. Do not commit generated `target/` artifacts or user configuration.

## Build, Test, and Development Commands

- `cargo run`: build and launch a debug instance.
- `RUST_BACKTRACE=1 cargo run`: run with Rust panic backtraces.
- `cargo build`: verify the debug build.
- `cargo build --release`: produce `target/release/bilibili-tui`.
- `cargo test`: run all unit tests.
- `cargo fmt --check`: verify formatting without modifying files.
- `cargo clippy --all-targets --all-features -- -D warnings`: enforce lint-clean code.

The application expects `mpv` for playback. Network-facing behavior may require a valid Bilibili login stored through the application.

## Coding Style & Naming Conventions

Use standard `rustfmt` formatting (four-space indentation). Name modules, functions, and variables in `snake_case`; structs, enums, and traits in `PascalCase`; constants in `SCREAMING_SNAKE_CASE`. Keep API wire models in `src/api/` and UI state transitions out of rendering code. Prefer explicit action and event variants such as `OpenVideoDetail` and `VideoDetailLoaded`.

## Testing Guidelines

Add focused tests for deserialization changes, navigation state, key handling, and stale network responses. Name tests by observable behavior, for example `feed_accepts_string_update_num`. When fixing API type drift, include a minimized JSON fixture covering both numeric and string representations. Run formatting, tests, Clippy, and a build before submitting.

## Commit & Pull Request Guidelines

History mostly follows Conventional Commits: `feat:`, `fix:`, `refactor:`, `chore(deps):`, and `chore(release):`. Keep each commit scoped and use an imperative summary.

Pull requests should explain the user-visible change, implementation approach, and verification commands. Link relevant issues. Include screenshots for layout or theme changes, and call out configuration migrations, API assumptions, or new external dependencies.

## Security & Configuration

Never commit cookies, credentials, debug response bodies, or files from the platform configuration directory. Redact Bilibili identifiers, query strings, signed media URLs, and session data from bug reports and logs. Diagnostic code must not persist response bodies by default and must create any potentially private log with owner-only permissions.
