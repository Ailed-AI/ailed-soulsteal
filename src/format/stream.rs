use std::io::Write;

use anyhow::Result;

use crate::game::TokenizedGame;

/// Write a tokenized game as a length-prefixed record to stdout.
/// Format: [total_bytes: u32][seq_len: u16][token_ids...][turn_ids...][category_ids...][outcome: u8]
pub fn write_stream_record<W: Write>(writer: &mut W, game: &TokenizedGame) -> Result<()> {
    let seq_len = game.token_ids.len() as u16;
    // Total bytes: 2 (seq_len) + 2*seq_len (tokens) + seq_len (turns) + seq_len (cats) + 1 (outcome)
    let record_bytes = 2 + (2 * seq_len as u32) + (seq_len as u32) + (seq_len as u32) + 1;

    // Length prefix
    writer.write_all(&record_bytes.to_le_bytes())?;

    // seq_len
    writer.write_all(&seq_len.to_le_bytes())?;

    // token_ids
    for &tid in &game.token_ids {
        writer.write_all(&tid.to_le_bytes())?;
    }

    // turn_ids
    writer.write_all(&game.turn_ids)?;

    // category_ids
    writer.write_all(&game.category_ids)?;

    // outcome
    writer.write_all(&[game.outcome])?;

    Ok(())
}
