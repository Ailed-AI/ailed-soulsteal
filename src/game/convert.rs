use std::io::{BufRead, Write};

use anyhow::Result;
use shakmaty::{Chess, Position, uci::UciMove};

use crate::game::chess::PgnParser;
use crate::game::GameParser;

/// Convert PGN to UGN format.
///
/// Reads PGN games from `reader` and writes UGN to `writer`.
/// Uses shakmaty to convert SAN moves to UCI notation.
pub fn pgn_to_ugn(reader: Box<dyn BufRead + '_>, writer: &mut dyn Write) -> Result<u64> {
    let parser = PgnParser;
    let games = parser.parse(reader);
    let mut count = 0u64;

    for game in games {
        // Convert SAN moves to UCI via shakmaty
        let mut pos = Chess::default();
        let mut uci_moves: Vec<String> = Vec::new();
        let mut valid = true;

        for san_str in &game.moves {
            let san: shakmaty::san::San = match san_str.parse() {
                Ok(s) => s,
                Err(_) => {
                    valid = false;
                    break;
                }
            };
            let m = match san.to_move(&pos) {
                Ok(m) => m,
                Err(_) => {
                    valid = false;
                    break;
                }
            };
            let uci = UciMove::from_move(&m, shakmaty::CastlingMode::Standard);
            uci_moves.push(uci.to_string());
            pos = match pos.play(&m) {
                Ok(p) => p,
                Err(_) => {
                    valid = false;
                    break;
                }
            };
        }

        if !valid || uci_moves.len() < 4 {
            continue;
        }

        // Result token
        let result_token = match game.result.as_deref() {
            Some("1-0") => "1-0",
            Some("0-1") => "0-1",
            Some("1/2-1/2") => "1/2",
            _ => "*",
        };

        // Write @game header
        writeln!(writer, "@game chess")?;

        // Map PGN tags to UGN tags
        if let Some(event) = game.tags.get("Event") {
            writeln!(writer, "  @event \"{}\"", event)?;
        }
        if let Some(date) = game.tags.get("Date") {
            // PGN dates use dots (2026.03.30), UGN uses hyphens
            let ugn_date = date.replace('.', "-");
            writeln!(writer, "  @date {}", ugn_date)?;
        }

        // White player + elo
        if let Some(white) = game.tags.get("White") {
            let mut line = format!("  @white \"{}\"", white);
            if let Some(elo) = game.tags.get("WhiteElo") {
                if elo != "?" {
                    line.push_str(&format!(" @elo {}", elo));
                }
            }
            writeln!(writer, "{}", line)?;
        }

        // Black player + elo
        if let Some(black) = game.tags.get("Black") {
            let mut line = format!("  @black \"{}\"", black);
            if let Some(elo) = game.tags.get("BlackElo") {
                if elo != "?" {
                    line.push_str(&format!(" @elo {}", elo));
                }
            }
            writeln!(writer, "{}", line)?;
        }

        if let Some(tc) = game.tags.get("TimeControl") {
            if tc != "-" && tc != "?" {
                writeln!(writer, "  @time {}", tc)?;
            }
        }
        if let Some(eco) = game.tags.get("ECO") {
            if eco != "?" {
                writeln!(writer, "  @eco {}", eco)?;
            }
        }
        if let Some(opening) = game.tags.get("Opening") {
            writeln!(writer, "  @opening \"{}\"", opening)?;
        }
        if let Some(term) = game.tags.get("Termination") {
            writeln!(writer, "  @termination {}", term.to_lowercase())?;
        }

        writeln!(writer, "  @result {}", result_token)?;

        // Write moves — 10 per line, result token appended to the last line
        let num_chunks = (uci_moves.len() + 9) / 10;
        for (i, chunk) in uci_moves.chunks(10).enumerate() {
            let line: Vec<&str> = chunk.iter().map(|s| s.as_str()).collect();
            if i == num_chunks - 1 {
                // Last chunk — append result
                writeln!(writer, "  : {} {}", line.join(" "), result_token)?;
            } else {
                writeln!(writer, "  : {}", line.join(" "))?;
            }
        }

        writeln!(writer)?; // blank line between games

        count += 1;
    }

    Ok(count)
}
