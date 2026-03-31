# ailed-soulsteal — Design Spec

## Purpose

Fast Rust CLI that extracts game data from PGN files, filters by metadata, tokenizes moves, and writes a self-describing binary format for ML training. Primary consumer: ailed-soma. Designed with a universal game trait so future game notations (SGF, etc.) plug in without changing the core pipeline.

Named after Alucard's Soul Steal from Castlevania: Symphony of the Night.

## Architecture

Three layers, each a clean boundary:

```
Input Layer → Filter Layer → Output Layer
(parsers)     (predicates)    (serializers)
```

### Input Layer

Reads game notation and yields games one at a time (streaming, never all in memory).

- PGN parser for chess (v1)
- Auto-detects `.zst` extension for zstd decompression
- Reads from file or stdin

### Filter Layer

Chain of predicates on game metadata (tags). Fast because filters operate on headers, not moves.

- ELO range (both players must be in range)
- Result: decisive only, draws only, or all
- Minimum/maximum move count
- Max games (stop after N matches)

### Output Layer

Two modes:

- **Binary batch** (`-o file.somabin`): self-describing indexed binary file
- **Streaming** (`--stream`): length-prefixed records to stdout, pipeable to any consumer

## Binary Format (`.somabin`)

```
Header (64 bytes, fixed):
  magic:           [u8; 4]    b"SOMA"
  version:         u16        format version (1)
  game_type:       u16        0=chess, 1=go, 2=shogi, ...
  vocab_size:      u32        total vocabulary including special tokens
  num_games:       u32        total games in file
  max_seq_len:     u32        longest sequence (for pre-allocation)
  special_offset:  u16        where action tokens start (6 for chess)
  category_dims:   u16        number of categories (6 for chess)
  reserved:        [u8; 40]   zeroed, future use

Index Table (num_games * 8 bytes):
  offsets:         [u64]      byte offset of each game record from data section start

Data Section (variable):
  Per game record:
    seq_len:       u16        number of tokens (including BOS/EOS)
    token_ids:     [u16]      seq_len entries
    turn_ids:      [u8]       seq_len entries (0=white, 1=black)
    category_ids:  [u8]       seq_len entries (piece type)
    outcome:       u8         0=win, 1=draw, 2=loss, 255=unknown
```

Design decisions:
- u16 token_ids: vocab 4214 fits, max 65535 covers any game
- Index table: enables random access for PyTorch Dataset.__getitem__
- No padding: variable-length games, padding is DataLoader's job
- Self-describing: header carries all metadata needed to interpret the file

## CLI Interface

```bash
# Convert PGN to binary
soulsteal tokenize input.pgn -o train.somabin --vocab vocab.json

# With filters
soulsteal tokenize input.pgn -o train.somabin \
  --vocab vocab.json \
  --elo 1000:1800 \
  --min-moves 4 \
  --max-games 1000000 \
  --result decisive

# Zstd input (auto-detected)
soulsteal tokenize lichess_2016-02.pgn.zst -o train.somabin --vocab vocab.json

# Streaming mode
soulsteal tokenize input.pgn --vocab vocab.json --stream | consumer

# Inspect binary file
soulsteal info train.somabin

# Generate vocabulary from PGN
soulsteal vocab --generate input.pgn -o vocab.json
```

## Crate Structure

```
src/
  main.rs              CLI entry (clap)
  lib.rs               public API
  game/
    mod.rs             Game trait + types
    chess.rs           PGN parser, chess tokenizer, move validation
  filter/
    mod.rs             Filter trait + chain
    elo.rs
    result.rs
    moves.rs
  format/
    mod.rs             OutputFormat trait
    somabin.rs         Binary writer + reader
    stream.rs          Streaming stdout
  vocab.rs             Vocabulary (JSON load/save)
  io.rs                File/zstd/stdin handling
```

## Game Trait (extensibility)

```rust
pub struct RawGame {
    pub tags: HashMap<String, String>,
    pub moves: Vec<String>,       // notation-specific move strings
    pub result: Option<String>,
}

pub struct TokenizedGame {
    pub token_ids: Vec<u16>,
    pub turn_ids: Vec<u8>,
    pub category_ids: Vec<u8>,
    pub outcome: u8,
}

pub trait GameParser: Send {
    fn parse<R: BufRead>(&self, reader: R) -> Box<dyn Iterator<Item = RawGame> + '_>;
}

pub trait GameTokenizer {
    fn tokenize(&self, game: &RawGame, vocab: &Vocab) -> Option<TokenizedGame>;
}
```

Chess implements both. Future games implement both. Filters and output are game-agnostic.

## Dependencies

- `clap` — CLI argument parsing
- `serde`, `serde_json` — vocabulary files
- `zstd` — decompression
- `shakmaty` — fast chess move generation/validation (pure Rust)

## Performance Target

- Parse + tokenize 1M PGN games in under 60 seconds
- Binary file loading in PyTorch: instant (mmap, seek to game N)

## Python Reader

Ships as a standalone ~30 line Python snippet (or micro-package) that mmap-loads `.somabin` files into a PyTorch Dataset. No compilation needed on the Python side.

## v1 Scope

- Chess PGN only (game trait exists but only chess implements it)
- Batch binary output + streaming stdout
- ELO, result, move count filters
- Zstd decompression
- Info/inspect command
- Vocab generation from PGN

## Future (v2+)

- Go SGF parser
- Shogi KIF/CSA parser
- Universal turn-based notation format
- Incremental append to existing .somabin files
- Parallel parsing with rayon
