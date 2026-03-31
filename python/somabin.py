"""Python reader for .somabin files — load tokenized game data into PyTorch.

Usage:
    from somabin import SomabinDataset
    dataset = SomabinDataset("train.somabin")
    print(len(dataset))           # number of games
    game = dataset[42]            # random access to game 42
    # game keys: 'token_ids', 'turn_ids', 'category_ids', 'outcome', 'length'

    # Or inspect from command line:
    python somabin.py info train.somabin
    python somabin.py dump train.somabin --game 0 --count 5
"""

import mmap
import struct
import sys
from pathlib import Path

import numpy as np

MAGIC = b"SOMA"
HEADER_SIZE = 64


def read_header(mm: mmap.mmap) -> dict:
    """Read the somabin header."""
    raw = mm[:HEADER_SIZE]
    magic = raw[0:4]
    if magic != MAGIC:
        raise ValueError(f"Invalid magic: {magic!r}, expected {MAGIC!r}")

    version, game_type = struct.unpack_from("<HH", raw, 4)
    vocab_size, num_games, max_seq_len = struct.unpack_from("<III", raw, 8)
    special_offset, category_dims = struct.unpack_from("<HH", raw, 20)
    index_offset = struct.unpack_from("<Q", raw, 24)[0]

    return {
        "version": version,
        "game_type": game_type,
        "vocab_size": vocab_size,
        "num_games": num_games,
        "max_seq_len": max_seq_len,
        "special_offset": special_offset,
        "category_dims": category_dims,
        "index_offset": index_offset,
    }


def read_index(mm: mmap.mmap, header: dict) -> np.ndarray:
    """Read the game offset index table."""
    offset = header["index_offset"]
    n = header["num_games"]
    raw = mm[offset : offset + n * 8]
    return np.frombuffer(raw, dtype=np.uint64)


def read_game(mm: mmap.mmap, offset: int) -> dict:
    """Read a single game record at the given byte offset."""
    pos = offset

    # seq_len (u16)
    seq_len = struct.unpack_from("<H", mm, pos)[0]
    pos += 2

    # token_ids (u16 × seq_len)
    token_ids = np.frombuffer(mm[pos : pos + seq_len * 2], dtype=np.uint16).copy()
    pos += seq_len * 2

    # turn_ids (u8 × seq_len)
    turn_ids = np.frombuffer(mm[pos : pos + seq_len], dtype=np.uint8).copy()
    pos += seq_len

    # category_ids (u8 × seq_len)
    category_ids = np.frombuffer(mm[pos : pos + seq_len], dtype=np.uint8).copy()
    pos += seq_len

    # outcome (u8)
    outcome = mm[pos]

    return {
        "token_ids": token_ids,
        "turn_ids": turn_ids,
        "category_ids": category_ids,
        "outcome": outcome,
        "length": seq_len,
    }


class SomabinDataset:
    """Memory-mapped dataset for .somabin files.

    Returns raw numpy arrays. For PyTorch training, use SomaTrainingDataset.
    """

    def __init__(self, path: str | Path):
        self.path = Path(path)
        self._file = open(self.path, "rb")
        self._mm = mmap.mmap(self._file.fileno(), 0, access=mmap.ACCESS_READ)
        self.header = read_header(self._mm)
        self._index = read_index(self._mm, self.header)

    def __len__(self) -> int:
        return self.header["num_games"]

    def __getitem__(self, idx: int) -> dict:
        if idx < 0 or idx >= len(self):
            raise IndexError(f"Game index {idx} out of range [0, {len(self)})")
        offset = int(self._index[idx])
        return read_game(self._mm, offset)

    def __del__(self):
        if hasattr(self, "_mm"):
            self._mm.close()
        if hasattr(self, "_file"):
            self._file.close()

    def __repr__(self) -> str:
        gt = {0: "chess", 1: "go", 2: "shogi"}.get(self.header["game_type"], "unknown")
        return (
            f"SomabinDataset({self.path.name}, "
            f"games={self.header['num_games']}, "
            f"type={gt}, "
            f"vocab={self.header['vocab_size']})"
        )


class SomaTrainingDataset:
    """Drop-in replacement for ailed-soma's GameDataset.

    Reads from .somabin via mmap. Returns the exact same dict format
    that SelfSupervisedTrainer expects — no code changes needed in the trainer.

    Usage:
        from somabin import SomaTrainingDataset
        train_ds = SomaTrainingDataset("train.somabin", max_seq_len=300)
        val_ds = SomaTrainingDataset("val.somabin", max_seq_len=300)
        # Pass directly to SelfSupervisedTrainer.train()

    Or split a single file:
        train_ds, val_ds = SomaTrainingDataset.split("data.somabin", val_ratio=0.1)
    """

    PAD = 0
    WIN = 3
    DRAW = 4
    LOSS = 5

    def __init__(self, path: str | Path, max_seq_len: int = 300, indices: list[int] | None = None):
        self._ds = SomabinDataset(path)
        self.max_seq_len = max_seq_len
        self._indices = indices if indices is not None else list(range(len(self._ds)))

    def __len__(self) -> int:
        return len(self._indices)

    def __getitem__(self, idx: int) -> dict:
        import torch

        raw = self._ds[self._indices[idx]]
        tokens = raw["token_ids"].astype(np.int64).tolist()
        turns = raw["turn_ids"].astype(np.int64).tolist()
        categories = raw["category_ids"].astype(np.int64).tolist()

        # Truncate to max_seq_len + 1 (need one extra for target shift)
        max_len = self.max_seq_len + 1
        tokens = tokens[:max_len]
        turns = turns[:max_len]
        categories = categories[:max_len]

        length = len(tokens) - 1

        # Split into input and target (next-token prediction)
        input_tokens = tokens[:-1]
        target_tokens = tokens[1:]
        input_turns = turns[:-1]
        target_categories = categories[1:]

        # Outcome from the somabin record (already extracted)
        outcome = int(raw["outcome"])  # 0=win, 1=draw, 2=loss

        # Pad to max_seq_len
        pad_len = self.max_seq_len - length
        if pad_len > 0:
            input_tokens = input_tokens + [self.PAD] * pad_len
            target_tokens = target_tokens + [self.PAD] * pad_len
            input_turns = input_turns + [0] * pad_len
            target_categories = target_categories + [0] * pad_len

        return {
            "input_tokens": torch.tensor(input_tokens, dtype=torch.long),
            "target_tokens": torch.tensor(target_tokens, dtype=torch.long),
            "turns": torch.tensor(input_turns, dtype=torch.long),
            "categories": torch.tensor(target_categories, dtype=torch.long),
            "outcome": torch.tensor(outcome, dtype=torch.long),
            "length": torch.tensor(length, dtype=torch.long),
        }

    @classmethod
    def split(
        cls,
        path: str | Path,
        max_seq_len: int = 300,
        val_ratio: float = 0.1,
        seed: int = 42,
    ) -> tuple["SomaTrainingDataset", "SomaTrainingDataset"]:
        """Split a single .somabin into train and val datasets.

        Returns:
            (train_dataset, val_dataset)
        """
        import random

        ds = SomabinDataset(path)
        n = len(ds)
        indices = list(range(n))
        random.Random(seed).shuffle(indices)

        val_count = int(n * val_ratio)
        train_indices = indices[val_count:]
        val_indices = indices[:val_count]

        return (
            cls(path, max_seq_len=max_seq_len, indices=train_indices),
            cls(path, max_seq_len=max_seq_len, indices=val_indices),
        )

    def __repr__(self) -> str:
        return f"SomaTrainingDataset(games={len(self)}, max_seq_len={self.max_seq_len})"


# --- CLI ---

OUTCOME_NAMES = {0: "win", 1: "draw", 2: "loss", 255: "unknown"}
CATEGORY_NAMES = {0: "pawn", 1: "knight", 2: "bishop", 3: "rook", 4: "queen", 5: "king"}


def cmd_info(path: str):
    ds = SomabinDataset(path)
    h = ds.header
    gt = {0: "chess", 1: "go", 2: "shogi"}.get(h["game_type"], "unknown")
    size_mb = ds.path.stat().st_size / 1_048_576
    print(f"Format:         somabin v{h['version']}")
    print(f"Game type:      {gt} ({h['game_type']})")
    print(f"Vocab size:     {h['vocab_size']}")
    print(f"Games:          {h['num_games']}")
    print(f"Max seq len:    {h['max_seq_len']}")
    print(f"Special offset: {h['special_offset']}")
    print(f"Category dims:  {h['category_dims']}")
    print(f"File size:      {size_mb:.1f} MB")


def cmd_dump(path: str, start: int = 0, count: int = 5, vocab_path: str | None = None):
    import json

    ds = SomabinDataset(path)
    vocab_rev: dict[int, str] = {}
    if vocab_path:
        raw = json.loads(Path(vocab_path).read_text())
        vocab_rev = {v: k for k, v in raw.items()}

    special_names = {0: "PAD", 1: "BOS", 2: "EOS", 3: "WIN", 4: "DRAW", 5: "LOSS"}

    end = min(start + count, len(ds))
    for i in range(start, end):
        game = ds[i]
        print(f"\n=== Game {i} ({game['length']} tokens, outcome={OUTCOME_NAMES.get(game['outcome'], '?')}) ===")

        moves = []
        for j, tid in enumerate(game["token_ids"]):
            tid = int(tid)
            turn = "W" if game["turn_ids"][j] == 0 else "B"
            cat = CATEGORY_NAMES.get(int(game["category_ids"][j]), "?")

            if tid in special_names:
                name = special_names[tid]
            elif tid in vocab_rev:
                name = vocab_rev[tid]
            else:
                name = f"#{tid}"

            if tid >= 6:  # action token
                moves.append(f"{name}({turn},{cat})")
            else:
                moves.append(f"[{name}]")

        print(" ".join(moves))


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python somabin.py <info|dump> <file.somabin> [options]")
        sys.exit(1)

    cmd = sys.argv[1]
    if cmd == "info":
        cmd_info(sys.argv[2])
    elif cmd == "dump":
        path = sys.argv[2]
        start = 0
        count = 5
        vocab = None
        i = 3
        while i < len(sys.argv):
            if sys.argv[i] == "--game":
                start = int(sys.argv[i + 1])
                i += 2
            elif sys.argv[i] == "--count":
                count = int(sys.argv[i + 1])
                i += 2
            elif sys.argv[i] == "--vocab":
                vocab = sys.argv[i + 1]
                i += 2
            else:
                i += 1
        cmd_dump(path, start, count, vocab)
    else:
        print(f"Unknown command: {cmd}")
        sys.exit(1)
