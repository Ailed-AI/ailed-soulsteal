# ailed-soulsteal

Fast game data extractor for ML training. Drains PGN into tokenized binary data at 60,000+ games/second.

Named after Alucard's **Soul Steal** from *Castlevania: Symphony of the Night* — extracts the essence from game records and absorbs it into a format your model can consume. Part of the [AILED](https://github.com/Ailed-AI) cognitive architecture.

## Why

Training game-playing AI models requires converting millions of PGN games into tokenized sequences. Python-based pipelines (python-chess) take 20+ minutes for 1M games. Soulsteal does it in under 30 seconds.

| Step | Python (python-chess) | Soulsteal |
|------|----------------------|-----------|
| Parse 1M PGN games | ~15 min | ~10 sec |
| Tokenize 1M games | ~10 min | ~5 sec |
| Total pipeline | ~25 min | **~15 sec** |
| Random access (PyTorch) | Load all to RAM | mmap, instant |

## Install

### Pre-built binaries

Download from [Releases](https://github.com/Ailed-AI/ailed-soulsteal/releases) for:
- Linux x86_64 / aarch64
- macOS x86_64 / Apple Silicon
- Windows x86_64

### From source

```bash
cargo install --git https://github.com/Ailed-AI/ailed-soulsteal.git
```

Or clone and build:

```bash
git clone https://github.com/Ailed-AI/ailed-soulsteal.git
cd ailed-soulsteal
cargo build --release
# Binary at ./target/release/soulsteal
```

## Quick Start

```bash
# Generate a vocabulary file (all possible chess UCI moves)
soulsteal vocab --generate -o vocab.json

# Convert PGN to binary (supports .pgn.zst for compressed)
soulsteal tokenize lichess_2016-02.pgn.zst \
  -o train.somabin \
  --vocab vocab.json \
  --elo 1000:1800 \
  --max-games 1000000

# Inspect the binary
soulsteal info train.somabin
soulsteal stats train.somabin

# View decoded games
soulsteal dump train.somabin --vocab vocab.json --game 0 --count 5

# Split into train/val JSONL
soulsteal split train.somabin -o data/ --vocab vocab.json --val-ratio 0.1
```

## Commands

### `tokenize` — PGN to binary

```bash
soulsteal tokenize input.pgn -o output.somabin --vocab vocab.json [options]
```

| Flag | Description |
|------|-------------|
| `--elo 1000:1800` | Filter by ELO range (both players) |
| `--min-moves 4` | Minimum moves per game |
| `--max-games N` | Stop after N games |
| `--result decisive` | Only wins/losses (no draws) |
| `--stream` | Output to stdout instead of file |

Supports `.pgn.zst` (zstd-compressed) input, auto-detected by extension.

### `info` — File metadata

```bash
soulsteal info train.somabin
```

### `stats` — Dataset statistics

```bash
soulsteal stats train.somabin
```

Output: game count, sequence lengths, outcome distribution, piece move breakdown.

### `dump` — Decode games

```bash
soulsteal dump train.somabin --vocab vocab.json --game 0 --count 5
soulsteal dump train.somabin --json  # JSON output
```

### `split` — Train/val JSONL

```bash
soulsteal split train.somabin -o data/ --vocab vocab.json --val-ratio 0.1
soulsteal split train.somabin -o data/ --raw  # Raw token IDs
```

### `vocab` — Generate vocabulary

```bash
soulsteal vocab --generate -o vocab.json
```

## Binary Format (.somabin)

Self-describing, indexed, memory-mappable. PyTorch loads games by index without reading the full file.

```
Header (64 bytes): magic, version, game_type, vocab_size, num_games, max_seq_len, ...
Index Table:       byte offset for each game (random access)
Data Section:      per game: [seq_len, token_ids, turn_ids, category_ids, outcome]
```

See [design doc](docs/2026-03-30-soulsteal-design.md) for full specification.

## Python Reader

A minimal mmap-based reader ships in `python/somabin.py`. Works with PyTorch DataLoader:

```python
from somabin import SomabinDataset

dataset = SomabinDataset("train.somabin")
print(len(dataset))     # 1000000
game = dataset[42]      # instant random access
# game['token_ids']     — numpy uint16 array
# game['turn_ids']      — numpy uint8 array
# game['category_ids']  — numpy uint8 array
# game['outcome']       — 0=win, 1=draw, 2=loss
```

500K random reads per second via mmap.

## Architecture

Designed for extensibility to any turn-based game:

```
Input Layer (parsers)  →  Filter Layer (predicates)  →  Output Layer (serializers)
     PGN (chess)              ELO, result, moves           .somabin binary
     SGF (go) [future]                                     JSONL
     KIF (shogi) [future]                                  streaming stdout
```

The `GameParser` and `GameTokenizer` traits define the interface. Chess is the v1 implementation. Adding a new game means implementing two traits — the rest of the pipeline is game-agnostic.

## Part of the AILED Ecosystem

| Project | Role |
|---------|------|
| [ailed-soma](https://github.com/Ailed-AI/ailed-soma) | Self-supervised game predictor (the cartridge) |
| **ailed-soulsteal** | Training data pipeline (this tool) |
| [ailed-engine](https://github.com/Ailed-AI/ailed-engine) | Chess move predictor |
| [ailed-grimoire](https://github.com/Ailed-AI/ailed-grimoire) | Game definition compiler |
| [ailed-glyph](https://github.com/Ailed-AI/ailed-glyph) | Universal engine wrapper |

## License

[MIT](LICENSE)
