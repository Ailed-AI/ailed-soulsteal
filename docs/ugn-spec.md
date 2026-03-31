# UGN — Universal Game Notation

**Version 0.1 (Draft)**

## Purpose

A human-readable, machine-parseable notation for any turn-based game. UGN replaces game-specific formats (PGN for chess, SGF for Go, KIF for Shogi) with one notation that works for all.

UGN is the source code. Somabin is the compiled binary.

```
UGN (human-readable)  →  soulsteal (compiler)  →  somabin (machine binary)
```

Soulsteal can also decompile: somabin → UGN.

## Design Principles

1. **One game, readable at a glance** — a person can look at a UGN block and understand what happened
2. **Parseable without context** — no state machine needed, no "a semicolon breaks everything"
3. **Universal** — same syntax for chess, Go, poker, tic-tac-toe. Only the move vocabulary changes
4. **Extendable** — metadata fields can be added without breaking parsers
5. **No external compression needed** — dense enough that zst is optional
6. **Grep-friendly** — common queries work with standard text tools

## Format

### Game Block

A game is a block that starts with `@game` and ends at the next `@game` or EOF.

```
@game <type> [inline-tags]
  [@tag value]...
  : <moves> <result>
```

### Tags

Tags start with `@` and can appear inline with `@game` or on their own lines.

```
@game chess
  @white "Magnus Carlsen" @elo 2850
  @black "Hikaru Nakamura" @elo 2780
  @date 2026-03-30
  @event "Speed Chess Championship"
  @time 300+0
  @result 1-0
  : e2e4 e7e5 g1f3 b8c6 f1b5 a7a6 b5a4 g8f6 1-0
```

Tag values:
- Quoted strings: `@white "Magnus Carlsen"`
- Bare values (no spaces): `@elo 2850`, `@date 2026-03-30`
- Boolean (presence = true): `@rated`

### Moves Line

Moves are on lines starting with `:`. Moves are space-separated. The result token terminates the sequence.

```
: e2e4 e7e5 g1f3 b8c6 f1b5 a7a6 1-0
```

Multiple `:` lines are concatenated (for readability of long games):

```
: e2e4 e7e5 g1f3 b8c6 f1b5 a7a6
: b5a4 g8f6 e1g1 f8e7 f1e1 b7b5
: a4b3 d7d6 c2c3 e8g8 1-0
```

### Move Annotations

Moves can carry inline annotations using suffixes. Annotations are optional and stripped by the tokenizer — they don't affect the move token.

**NAGs (Numeric Annotation Glyphs):** Standard chess symbols appended directly to the move:

| Symbol | Meaning |
|--------|---------|
| `!` | Good move |
| `!!` | Brilliant move |
| `?` | Mistake |
| `??` | Blunder |
| `!?` | Interesting move |
| `?!` | Dubious move |

**Clock time:** `{t:<seconds>}` — seconds remaining on the player's clock after making the move.

**Combined:** Annotations stack: `e5!{t:42}` means "good move, 42 seconds remaining."

```
: e2e4{t:180} e7e5{t:180} g1f3{t:179} b8c6{t:177}
: f1b5!{t:170} a7a6?!{t:160} b5a4{t:155} g8f6{t:148}
```

**Parsing rule:** The move token is everything before the first `!`, `?`, or `{`. Parsers extract the pure move for tokenization and optionally preserve annotations as metadata.

```
"e5!{t:42}" → move: "e5", nag: "!", clock: 42
"b5a4{t:155}" → move: "b5a4", nag: none, clock: 155
"a6?!" → move: "a6", nag: "?!", clock: none
```

### Results

Standard result tokens per game type:

| Game | Win (first player) | Win (second player) | Draw |
|------|-------------------|--------------------|----|
| Chess | `1-0` | `0-1` | `1/2` |
| Go | `B+<score>` | `W+<score>` | `0` |
| Generic | `1-0` | `0-1` | `1/2` |

Result suffixes for termination reason:

| Suffix | Meaning |
|--------|---------|
| `R` | Resignation (`1-0R`) |
| `T` | Timeout (`0-1T`) |
| `A` | Abandoned (`1-0A`) |

Unknown result: `*`.

### Chess Tags Reference

| Tag | Value | Example |
|-----|-------|---------|
| `@white` | Player name (quoted) | `@white "Magnus Carlsen"` |
| `@black` | Player name (quoted) | `@black "Hikaru Nakamura"` |
| `@elo` | Rating | `@elo 2850` |
| `@date` | ISO date | `@date 2026-03-30` |
| `@event` | Event name (quoted) | `@event "Tata Steel"` |
| `@time` | Time control | `@time 600+5` |
| `@eco` | ECO code | `@eco C50` |
| `@opening` | Opening name (quoted) | `@opening "Italian Game"` |
| `@rated` | Boolean (presence) | `@rated` |
| `@termination` | How the game ended | `@termination normal` |
| `@result` | Game outcome | `@result 1-0` |

### Comments

Comments start with `#` and run to end of line.

```
@game chess
  @white "Player1" @elo 1400
  # This was a great game
  : e2e4 e7e5 g1f3 b8c6 1-0
```

Inline comments on move lines are NOT supported — moves are pure data.

## Game Types

### Chess

- Type: `chess`
- Move notation: UCI (e.g., `e2e4`, `e7e8q` for promotion)
- Players: `@white`, `@black`
- Tags: `@elo`, `@date`, `@event`, `@time`, `@eco`, `@rated`

```
@game chess
  @white "Player1" @elo 1400
  @black "Player2" @elo 1300
  @date 2026-03-30
  @result 1-0
  : e2e4 e7e5 g1f3 b8c6 f1b5 a7a6 b5a4 g8f6 1-0
```

### Go

- Type: `go`
- Board size: `@size 19` (default 19, also 9 or 13)
- Move notation: column+row (e.g., `Q16`, `D4`). Pass: `pass`
- Players: `@black`, `@white`
- Tags: `@rank`, `@komi`, `@handicap`

```
@game go @size 19
  @black "Player1" @rank 5d
  @white "Player2" @rank 3d
  @komi 6.5
  @result B+2.5
  : Q16 D4 R14 Q3 C16 E16 D17 pass B+2.5
```

### Checkers

- Type: `checkers`
- Move notation: square numbers (e.g., `11-15`, `23-18`). Jumps: `11x18`
- Players: `@black`, `@white`

```
@game checkers
  @black "Player1"
  @white "Player2"
  @result 1-0
  : 11-15 23-18 8-11 27-23 11x18 22x15 1-0
```

### Tic-Tac-Toe

- Type: `tictactoe`
- Move notation: cell index 0-8 (top-left to bottom-right)
- Players: `@x`, `@o`

```
@game tictactoe
  @x "Player1"
  @o "Player2"
  @result 1-0
  : 4 0 1 3 7 1-0
```

### Poker (Texas Hold'em hand)

- Type: `holdem`
- Move notation: action/amount (e.g., `f` fold, `c` call, `r500` raise 500, `k` check)
- Players: `@seats <n>`
- Tags: `@blinds`, `@cards` (revealed)

```
@game holdem @seats 6
  @blinds 1/2
  @result 0-1
  # preflop
  : f f r6 c f c
  # flop: Ah Kd 7s
  @board AhKd7s
  : k r15 c
  # turn: 3c
  @board AhKd7s3c
  : k k
  # river: Jh
  @board AhKd7s3cJh
  : r50 c 0-1
```

## Compact One-Line Form

For pipelines and streaming, UGN supports a compact single-line form:

```
chess|w:Player1|b:Player2|elo:1400:1300|e2e4 e7e5 g1f3 b8c6|1-0
```

Format: `<type>|<tags>|<moves>|<result>`

Soulsteal accepts both forms. The compact form is useful for:
- Streaming (`soulsteal tokenize --stream`)
- Log files
- Database exports
- Piping between tools

## File Extension

- `.ugn` — UGN text files
- `.somabin` — compiled binary (soulsteal output)

## Comparison with PGN

| Feature | PGN | UGN |
|---------|-----|-----|
| Games | Chess only | Any turn-based game |
| Move notation | SAN (requires board state to parse) | UCI/direct (parseable without state) |
| Structure | Multi-line with brackets, semicolons | `@tag` prefix, `:` moves, `#` comments |
| Parsing | Stateful, error-prone | Line-by-line, no state machine |
| Comments | `{inline}` and `;to-eol` | `#` only, never inline with moves |
| Variations | `(nested (parens))` | Not in v0.1 (future: separate game blocks) |
| Machine output | Text only | Text (UGN) + binary (somabin) |
| One game, one line | No | Yes (compact form) |
| Grep for ELO range | Requires multi-line parsing | `grep "@elo 1[4-6]"` |

## Soulsteal Integration

```bash
# UGN → somabin (compile)
soulsteal tokenize games.ugn -o train.somabin --format ugn

# PGN → UGN (convert legacy)
soulsteal convert games.pgn -o games.ugn

# somabin → UGN (decompile)
soulsteal dump train.somabin --vocab vocab.json --format ugn > games.ugn

# Compact one-liner pipe
cat games.ugn | soulsteal tokenize --format ugn --stream | consumer
```

## Versioning

The UGN spec is versioned. Files should declare version in a header comment:

```
# UGN v0.1
@game chess
  ...
```

Parsers should accept files without version headers (default to latest).
