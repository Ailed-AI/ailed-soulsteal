use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};

/// Vocabulary mapping: action string → token ID.
pub struct Vocab {
    action_to_id: HashMap<String, u16>,
    id_to_action: HashMap<u16, String>,
    pub vocab_size: u32,
    pub special_offset: u16,
}

impl Vocab {
    /// Load vocabulary from a JSON file.
    /// Expected format: { "e2e4": 6, "d2d4": 7, ... }
    /// Token IDs should already include the special token offset.
    pub fn from_json(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read vocab file: {}", path.display()))?;
        let map: HashMap<String, u16> = serde_json::from_str(&contents)
            .with_context(|| "Failed to parse vocab JSON")?;

        let max_id = map.values().copied().max().unwrap_or(0);
        let id_to_action: HashMap<u16, String> = map.iter().map(|(k, &v)| (v, k.clone())).collect();

        Ok(Self {
            vocab_size: (max_id + 1) as u32,
            special_offset: 6, // PAD, BOS, EOS, WIN, DRAW, LOSS
            action_to_id: map,
            id_to_action,
        })
    }

    /// Look up token ID for an action string.
    pub fn get(&self, action: &str) -> Option<u16> {
        self.action_to_id.get(action).copied()
    }

    /// Look up action string for a token ID.
    pub fn get_action(&self, id: u16) -> Option<&str> {
        self.id_to_action.get(&id).map(|s| s.as_str())
    }

    /// Generate vocabulary from a PGN file by collecting all unique UCI moves.
    pub fn generate_from_pgn<R: std::io::BufRead>(_reader: R) -> Result<Self> {
        let mut uci_moves: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

        // Generate all possible UCI strings (source-dest pairs)
        for from_sq in 0..64u32 {
            for to_sq in 0..64u32 {
                if from_sq == to_sq {
                    continue;
                }
                let from = shakmaty::Square::new(from_sq);
                let to = shakmaty::Square::new(to_sq);
                let uci = format!("{}{}", from, to);
                uci_moves.insert(uci.clone());

                // Promotion variants for pawns reaching back rank
                let from_rank = from_sq / 8;
                let to_rank = to_sq / 8;
                let from_file = from_sq % 8;
                let to_file = to_sq % 8;
                if ((from_rank == 6 && to_rank == 7) || (from_rank == 1 && to_rank == 0))
                    && ((from_file as i32) - (to_file as i32)).unsigned_abs() <= 1
                {
                    for promo in &['q', 'r', 'b', 'n'] {
                        uci_moves.insert(format!("{}{}", uci, promo));
                    }
                }
            }
        }

        let offset = 6u16; // special tokens
        let mut action_to_id = HashMap::new();
        let mut id_to_action = HashMap::new();

        for (i, uci) in uci_moves.iter().enumerate() {
            let id = (i as u16) + offset;
            action_to_id.insert(uci.clone(), id);
            id_to_action.insert(id, uci.clone());
        }

        let max_id = action_to_id.values().copied().max().unwrap_or(0);

        Ok(Self {
            vocab_size: (max_id + 1) as u32,
            special_offset: offset,
            action_to_id,
            id_to_action,
        })
    }

    /// Save vocabulary to JSON file.
    pub fn to_json(&self, path: &Path) -> Result<()> {
        let contents = serde_json::to_string_pretty(&self.action_to_id)?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}
