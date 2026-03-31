# Contributing to ailed-soulsteal

Thanks for your interest in contributing! Here's how to get started.

## Development Setup

```bash
git clone https://github.com/Ailed-AI/ailed-soulsteal.git
cd ailed-soulsteal
cargo build
cargo test
```

Requires Rust 1.75+ (for stable async traits). Install via [rustup](https://rustup.rs/).

## Making Changes

1. Fork the repo and create a branch from `main`
2. Write your code
3. Add tests if applicable
4. Run `cargo test` and `cargo clippy`
5. Open a pull request

## Code Style

- Follow standard Rust conventions (`rustfmt`)
- Keep functions small and focused
- Use `anyhow::Result` for error handling in CLI code
- Use `Option`/`Result` without `anyhow` in library code where appropriate

## Adding a New Game Format

Soulsteal is designed to support any turn-based game. To add a new game:

1. Create `src/game/your_game.rs`
2. Implement `GameParser` (parse notation into `RawGame`)
3. Implement `GameTokenizer` (convert moves to token IDs)
4. Add the game type constant to the somabin header
5. Wire it into the CLI

See `src/game/chess.rs` for the reference implementation.

## Adding New Filters

1. Create `src/filter/your_filter.rs`
2. Implement the `GameFilter` trait
3. Add CLI flags in `src/main.rs`

## Reporting Issues

Use [GitHub Issues](https://github.com/Ailed-AI/ailed-soulsteal/issues). Include:

- What you expected
- What happened
- Steps to reproduce
- Your OS and Rust version

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
