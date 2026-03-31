pub mod chess;

use std::collections::HashMap;
use std::io::BufRead;

/// A raw game as parsed from notation, before tokenization.
pub struct RawGame {
    pub tags: HashMap<String, String>,
    pub moves: Vec<String>,
    pub result: Option<String>,
}

/// A tokenized game ready for binary serialization.
pub struct TokenizedGame {
    pub token_ids: Vec<u16>,
    pub turn_ids: Vec<u8>,
    pub category_ids: Vec<u8>,
    pub outcome: u8,
}

/// Special token IDs — shared across all games.
pub mod special {
    pub const PAD: u16 = 0;
    pub const BOS: u16 = 1;
    pub const EOS: u16 = 2;
    pub const WIN: u16 = 3;
    pub const DRAW: u16 = 4;
    pub const LOSS: u16 = 5;
    pub const OFFSET: u16 = 6;
}

/// Outcome encoding for binary format.
pub mod outcome {
    pub const WIN: u8 = 0;
    pub const DRAW: u8 = 1;
    pub const LOSS: u8 = 2;
    pub const UNKNOWN: u8 = 255;
}

/// Parser for a specific game notation.
pub trait GameParser {
    fn parse<'a>(&'a self, reader: Box<dyn BufRead + 'a>) -> Box<dyn Iterator<Item = RawGame> + 'a>;
}

/// Tokenizer for a specific game type.
pub trait GameTokenizer {
    fn tokenize(&self, game: &RawGame, vocab: &crate::vocab::Vocab) -> Option<TokenizedGame>;
}
