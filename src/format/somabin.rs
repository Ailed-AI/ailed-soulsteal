use std::io::{self, Seek, Write};

use anyhow::{Context, Result};

use crate::game::TokenizedGame;

const MAGIC: &[u8; 4] = b"SOMA";
const VERSION: u16 = 1;
const HEADER_SIZE: u64 = 64;

/// Writer for the .somabin binary format.
pub struct SomabinWriter<W: Write + Seek> {
    writer: W,
    game_type: u16,
    vocab_size: u32,
    special_offset: u16,
    category_dims: u16,
    offsets: Vec<u64>,
    max_seq_len: u32,
    data_start: u64,
}

impl<W: Write + Seek> SomabinWriter<W> {
    pub fn new(
        writer: W,
        game_type: u16,
        vocab_size: u32,
        special_offset: u16,
        category_dims: u16,
    ) -> Self {
        Self {
            writer,
            game_type,
            vocab_size,
            special_offset,
            category_dims,
            offsets: Vec::new(),
            max_seq_len: 0,
            data_start: 0,
        }
    }

    /// Write a placeholder header and index. Call finalize() when done.
    pub fn begin(&mut self, estimated_games: u32) -> Result<()> {
        // Write placeholder header (64 bytes)
        self.writer.write_all(&[0u8; 64])?;

        // Reserve space for index (will be overwritten in finalize)
        // We don't know exact count yet, so we'll collect offsets and write at the end
        self.data_start = HEADER_SIZE;

        Ok(())
    }

    /// Write a single tokenized game.
    pub fn write_game(&mut self, game: &TokenizedGame) -> Result<()> {
        let current_pos = self.writer.stream_position()?;
        self.offsets.push(current_pos);

        let seq_len = game.token_ids.len() as u16;
        if seq_len as u32 > self.max_seq_len {
            self.max_seq_len = seq_len as u32;
        }

        // seq_len
        self.writer.write_all(&seq_len.to_le_bytes())?;

        // token_ids (u16 each)
        for &tid in &game.token_ids {
            self.writer.write_all(&tid.to_le_bytes())?;
        }

        // turn_ids (u8 each)
        self.writer.write_all(&game.turn_ids)?;

        // category_ids (u8 each)
        self.writer.write_all(&game.category_ids)?;

        // outcome (u8)
        self.writer.write_all(&[game.outcome])?;

        Ok(())
    }

    /// Finalize: rewrite header, append index at end.
    pub fn finalize(mut self) -> Result<()> {
        let num_games = self.offsets.len() as u32;

        // Write index table at current position
        let index_offset = self.writer.stream_position()?;
        for &offset in &self.offsets {
            self.writer.write_all(&offset.to_le_bytes())?;
        }

        // Seek back and write real header
        self.writer.seek(io::SeekFrom::Start(0))?;

        // magic (4)
        self.writer.write_all(MAGIC)?;
        // version (2)
        self.writer.write_all(&VERSION.to_le_bytes())?;
        // game_type (2)
        self.writer.write_all(&self.game_type.to_le_bytes())?;
        // vocab_size (4)
        self.writer.write_all(&self.vocab_size.to_le_bytes())?;
        // num_games (4)
        self.writer.write_all(&num_games.to_le_bytes())?;
        // max_seq_len (4)
        self.writer.write_all(&self.max_seq_len.to_le_bytes())?;
        // special_offset (2)
        self.writer.write_all(&self.special_offset.to_le_bytes())?;
        // category_dims (2)
        self.writer.write_all(&self.category_dims.to_le_bytes())?;
        // index_offset (8) — stored in reserved area so reader can find the index
        self.writer.write_all(&index_offset.to_le_bytes())?;
        // remaining reserved (32 bytes)
        self.writer.write_all(&[0u8; 32])?;

        self.writer.flush()?;
        Ok(())
    }
}

/// Read and display info from a .somabin file header.
pub fn read_info(path: &std::path::Path) -> Result<SomabinInfo> {
    let data = std::fs::read(path).with_context(|| format!("Failed to read {}", path.display()))?;

    if data.len() < 64 {
        anyhow::bail!("File too small for somabin header");
    }

    if &data[0..4] != MAGIC {
        anyhow::bail!("Invalid magic: expected SOMA");
    }

    let version = u16::from_le_bytes([data[4], data[5]]);
    let game_type = u16::from_le_bytes([data[6], data[7]]);
    let vocab_size = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
    let num_games = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
    let max_seq_len = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
    let special_offset = u16::from_le_bytes([data[20], data[21]]);
    let category_dims = u16::from_le_bytes([data[22], data[23]]);
    let index_offset = u64::from_le_bytes([
        data[24], data[25], data[26], data[27],
        data[28], data[29], data[30], data[31],
    ]);

    Ok(SomabinInfo {
        version,
        game_type,
        vocab_size,
        num_games,
        max_seq_len,
        special_offset,
        category_dims,
        index_offset,
        file_size: data.len() as u64,
    })
}

pub struct SomabinInfo {
    pub version: u16,
    pub game_type: u16,
    pub vocab_size: u32,
    pub num_games: u32,
    pub max_seq_len: u32,
    pub special_offset: u16,
    pub category_dims: u16,
    pub index_offset: u64,
    pub file_size: u64,
}

impl std::fmt::Display for SomabinInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let game_type_name = match self.game_type {
            0 => "chess",
            1 => "go",
            2 => "shogi",
            _ => "unknown",
        };
        writeln!(f, "Format:         somabin v{}", self.version)?;
        writeln!(f, "Game type:      {} ({})", game_type_name, self.game_type)?;
        writeln!(f, "Vocab size:     {}", self.vocab_size)?;
        writeln!(f, "Games:          {}", self.num_games)?;
        writeln!(f, "Max seq len:    {}", self.max_seq_len)?;
        writeln!(f, "Special offset: {}", self.special_offset)?;
        writeln!(f, "Category dims:  {}", self.category_dims)?;
        writeln!(f, "File size:      {:.1} MB", self.file_size as f64 / 1_048_576.0)?;
        Ok(())
    }
}
