use std::collections::HashMap;
use std::io::BufRead;

use crate::game::{GameParser, RawGame};

/// UGN parser — yields RawGame from a UGN stream.
pub struct UgnParser;

impl GameParser for UgnParser {
    fn parse<'a>(&'a self, reader: Box<dyn BufRead + 'a>) -> Box<dyn Iterator<Item = RawGame> + 'a> {
        Box::new(UgnIterator {
            reader,
            line_buf: String::new(),
            pending_line: None,
        })
    }
}

struct UgnIterator<R> {
    reader: R,
    line_buf: String,
    /// When we read a `@game` line that belongs to the *next* game, stash it here.
    pending_line: Option<String>,
}

/// Chess result tokens (including suffixed variants like `1-0R`, `0-1T`).
fn is_result_token(s: &str) -> bool {
    let base = s.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    matches!(base, "1-0" | "0-1" | "1/2" | "1/2-1/2" | "*")
}

/// Strip annotations from a move token.
/// The pure move is everything before the first `!`, `?`, or `{`.
fn strip_annotations(token: &str) -> &str {
    let end = token
        .find(|c: char| c == '!' || c == '?' || c == '{')
        .unwrap_or(token.len());
    &token[..end]
}

/// Parse inline tags from a string like `@white "Magnus Carlsen" @elo 2850`.
/// Returns parsed tags and any leftover text that isn't a tag.
fn parse_inline_tags(s: &str, tags: &mut HashMap<String, String>) {
    let mut rest = s.trim();

    while let Some(at_pos) = rest.find('@') {
        rest = &rest[at_pos + 1..];

        // Tag name: everything up to first whitespace
        let name_end = rest
            .find(|c: char| c.is_whitespace())
            .unwrap_or(rest.len());
        let name = rest[..name_end].to_string();
        rest = rest[name_end..].trim_start();

        if rest.is_empty() || rest.starts_with('@') {
            // Boolean tag (presence = true)
            tags.insert(name, "true".to_string());
            continue;
        }

        // Value: quoted or bare
        if rest.starts_with('"') {
            // Quoted value — find closing quote
            let after_quote = &rest[1..];
            if let Some(close) = after_quote.find('"') {
                let value = after_quote[..close].to_string();
                tags.insert(name, value);
                rest = after_quote[close + 1..].trim_start();
            } else {
                // Unterminated quote — take the rest
                tags.insert(name, after_quote.to_string());
                break;
            }
        } else {
            // Bare value — up to next @ or end
            let val_end = rest.find('@').unwrap_or(rest.len());
            let value = rest[..val_end].trim().to_string();
            if !value.is_empty() {
                tags.insert(name, value);
            } else {
                tags.insert(name, "true".to_string());
            }
            rest = &rest[val_end..];
        }
    }
}

impl<R: BufRead> Iterator for UgnIterator<R> {
    type Item = RawGame;

    fn next(&mut self) -> Option<RawGame> {
        let mut tags = HashMap::new();
        let mut moves_text = String::new();
        let mut found_game = false;

        // If we stashed a @game line from a previous iteration, process it first
        if let Some(pending) = self.pending_line.take() {
            found_game = true;
            // Parse @game line: "@game <type> [inline-tags]"
            let after_game = pending.trim_start_matches("@game").trim();
            // First token is the game type
            let type_end = after_game
                .find(|c: char| c.is_whitespace() || c == '@')
                .unwrap_or(after_game.len());
            let game_type = &after_game[..type_end];
            if !game_type.is_empty() {
                tags.insert("_type".to_string(), game_type.to_string());
            }
            // Parse any inline tags after the type
            let remainder = &after_game[type_end..];
            if !remainder.trim().is_empty() {
                parse_inline_tags(remainder, &mut tags);
            }
        }

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

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line.starts_with("@game") {
                if found_game {
                    // This @game belongs to the next game — stash it
                    self.pending_line = Some(line.to_string());
                    break;
                }
                found_game = true;
                // Parse @game line
                let after_game = line.trim_start_matches("@game").trim();
                let type_end = after_game
                    .find(|c: char| c.is_whitespace() || c == '@')
                    .unwrap_or(after_game.len());
                let game_type = &after_game[..type_end];
                if !game_type.is_empty() {
                    tags.insert("_type".to_string(), game_type.to_string());
                }
                let remainder = &after_game[type_end..];
                if !remainder.trim().is_empty() {
                    parse_inline_tags(remainder, &mut tags);
                }
            } else if line.starts_with('@') && found_game {
                // Tag line(s)
                parse_inline_tags(line, &mut tags);
            } else if line.starts_with(':') && found_game {
                // Move line — strip the leading `:`
                let move_part = line[1..].trim();
                if !moves_text.is_empty() {
                    moves_text.push(' ');
                }
                moves_text.push_str(move_part);
            }
        }

        if !found_game {
            return None;
        }

        // Parse moves from the accumulated moves text
        let mut moves = Vec::new();
        let mut result = tags.get("result").cloned();

        for token in moves_text.split_whitespace() {
            if is_result_token(token) {
                result = Some(token.to_string());
                continue;
            }
            let pure_move = strip_annotations(token);
            if !pure_move.is_empty() {
                moves.push(pure_move.to_string());
            }
        }

        // Map UGN result tokens to the standard result format used by the tokenizer
        let normalized_result = result.map(|r| {
            let base = r.trim_end_matches(|c: char| c.is_ascii_alphabetic() && c != '/' && c != '-');
            match base {
                "1-0" => "1-0".to_string(),
                "0-1" => "0-1".to_string(),
                "1/2" | "1/2-1/2" => "1/2-1/2".to_string(),
                _ => r,
            }
        });

        Some(RawGame {
            tags,
            moves,
            result: normalized_result,
        })
    }
}
