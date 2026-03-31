use std::io::BufRead;

use shakmaty::{Chess, Move, Position, Role};

use crate::game::{GameParser, GameTokenizer, RawGame, TokenizedGame, outcome, special};
use crate::vocab::Vocab;

/// PGN parser — yields RawGame from a PGN stream.
pub struct PgnParser;

impl GameParser for PgnParser {
    fn parse<'a>(&'a self, reader: Box<dyn BufRead + 'a>) -> Box<dyn Iterator<Item = RawGame> + 'a> {
        Box::new(PgnIterator { reader, line_buf: String::new() })
    }
}

struct PgnIterator<R> {
    reader: R,
    line_buf: String,
}

impl<R: BufRead> Iterator for PgnIterator<R> {
    type Item = RawGame;

    fn next(&mut self) -> Option<RawGame> {
        let mut tags = std::collections::HashMap::new();
        let mut movetext = String::new();
        let mut in_tags = false;
        let mut found_game = false;

        loop {
            self.line_buf.clear();
            let bytes = self.reader.read_line(&mut self.line_buf).ok()?;
            if bytes == 0 {
                // EOF — return final game if we have one
                if found_game {
                    break;
                }
                return None;
            }

            let line = self.line_buf.trim();

            if line.is_empty() {
                if in_tags {
                    // Blank line after tags = start of movetext
                    in_tags = false;
                }
                if found_game && !movetext.is_empty() {
                    // Blank line after movetext = end of game
                    break;
                }
                continue;
            }

            if line.starts_with('[') {
                if found_game && !movetext.is_empty() {
                    // New game started — we need to re-parse this line next time
                    // For simplicity, break and lose this tag (games are separated by blank lines in valid PGN)
                    break;
                }
                in_tags = true;
                found_game = true;
                // Parse tag: [Key "Value"]
                if let Some(tag) = parse_tag(line) {
                    tags.insert(tag.0, tag.1);
                }
            } else if found_game {
                // Movetext line
                if !movetext.is_empty() {
                    movetext.push(' ');
                }
                movetext.push_str(line);
            }
        }

        if !found_game {
            return None;
        }

        let result = tags.get("Result").cloned();
        let moves = parse_san_moves(&movetext);

        Some(RawGame { tags, moves, result })
    }
}

fn parse_tag(line: &str) -> Option<(String, String)> {
    // [Key "Value"]
    let line = line.trim_start_matches('[').trim_end_matches(']');
    let space = line.find(' ')?;
    let key = line[..space].to_string();
    let val = line[space + 1..].trim().trim_matches('"').to_string();
    Some((key, val))
}

fn parse_san_moves(movetext: &str) -> Vec<String> {
    // Extract SAN moves, skipping move numbers, results, comments, NAGs
    let mut moves = Vec::new();
    let mut in_comment = false;

    for token in movetext.split_whitespace() {
        if token.starts_with('{') {
            in_comment = true;
            continue;
        }
        if in_comment {
            if token.ends_with('}') {
                in_comment = false;
            }
            continue;
        }
        // Skip move numbers (1. 1... 23.)
        if token.ends_with('.') || token.contains("...") {
            continue;
        }
        // Skip results
        if token == "1-0" || token == "0-1" || token == "1/2-1/2" || token == "*" {
            continue;
        }
        // Skip NAGs ($1, $2, etc.)
        if token.starts_with('$') {
            continue;
        }
        // Skip variation markers
        if token == "(" || token == ")" {
            continue;
        }
        moves.push(token.to_string());
    }
    moves
}

/// Chess tokenizer — converts SAN moves to token IDs using shakmaty for validation.
pub struct ChessTokenizer;

impl GameTokenizer for ChessTokenizer {
    fn tokenize(&self, game: &RawGame, vocab: &Vocab) -> Option<TokenizedGame> {
        let sans = &game.moves;
        if sans.len() < 4 {
            return None;
        }

        let mut pos = Chess::default();
        let mut token_ids = vec![special::BOS];
        let mut turn_ids = vec![0u8];
        let mut category_ids = vec![0u8];

        for san_str in sans {
            // Parse SAN move in current position
            let san: shakmaty::san::San = san_str.parse().ok()?;
            let m = san.to_move(&pos).ok()?;

            // Get UCI string
            let uci = uci_string(&m);

            // Look up in vocabulary
            let token_id = vocab.get(&uci)?;

            // Piece category
            let category = role_to_category(m.role());

            // Turn: 0 = white, 1 = black
            let turn = if pos.turn().is_white() { 0u8 } else { 1u8 };

            token_ids.push(token_id);
            turn_ids.push(turn);
            category_ids.push(category);

            // Apply move
            pos = pos.play(&m).ok()?;
        }

        // Outcome token
        let outcome_val = match game.result.as_deref() {
            Some("1-0") => {
                token_ids.push(special::WIN);
                turn_ids.push(0);
                category_ids.push(0);
                outcome::WIN
            }
            Some("0-1") => {
                token_ids.push(special::LOSS);
                turn_ids.push(0);
                category_ids.push(0);
                outcome::LOSS
            }
            Some("1/2-1/2") => {
                token_ids.push(special::DRAW);
                turn_ids.push(0);
                category_ids.push(0);
                outcome::DRAW
            }
            _ => outcome::UNKNOWN,
        };

        // EOS
        token_ids.push(special::EOS);
        turn_ids.push(0);
        category_ids.push(0);

        Some(TokenizedGame {
            token_ids,
            turn_ids,
            category_ids,
            outcome: outcome_val,
        })
    }
}

/// Convert a shakmaty Move to UCI string.
fn uci_string(m: &Move) -> String {
    match m {
        Move::Normal { from, to, promotion, .. } => {
            let mut s = format!("{}{}", from, to);
            if let Some(role) = promotion {
                s.push(match role {
                    Role::Queen => 'q',
                    Role::Rook => 'r',
                    Role::Bishop => 'b',
                    Role::Knight => 'n',
                    _ => 'q',
                });
            }
            s
        }
        Move::EnPassant { from, to, .. } => format!("{}{}", from, to),
        Move::Castle { king, rook } => {
            // UCI castling: king start → king destination
            let king_to = if rook.file() > king.file() {
                // Kingside
                shakmaty::Square::from_coords(shakmaty::File::G, king.rank())
            } else {
                // Queenside
                shakmaty::Square::from_coords(shakmaty::File::C, king.rank())
            };
            format!("{}{}", king, king_to)
        }
        Move::Put { .. } => String::new(), // crazyhouse, not relevant
    }
}

/// Map piece role to category index (matches soma tokenizer).
fn role_to_category(role: Role) -> u8 {
    match role {
        Role::Pawn => 0,
        Role::Knight => 1,
        Role::Bishop => 2,
        Role::Rook => 3,
        Role::Queen => 4,
        Role::King => 5,
    }
}
