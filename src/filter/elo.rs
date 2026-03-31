use crate::filter::GameFilter;
use crate::game::RawGame;

/// Filter games by ELO range. Both WhiteElo and BlackElo must be in range.
pub struct EloFilter {
    min: u16,
    max: u16,
}

impl EloFilter {
    pub fn new(min: u16, max: u16) -> Self {
        Self { min, max }
    }

    /// Parse "1000:1800" format.
    pub fn from_str(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 2 {
            return None;
        }
        let min = parts[0].parse().ok()?;
        let max = parts[1].parse().ok()?;
        Some(Self::new(min, max))
    }
}

impl GameFilter for EloFilter {
    fn accept(&self, game: &RawGame) -> bool {
        let white_elo = game.tags.get("WhiteElo")
            .and_then(|s| s.parse::<u16>().ok());
        let black_elo = game.tags.get("BlackElo")
            .and_then(|s| s.parse::<u16>().ok());

        match (white_elo, black_elo) {
            (Some(w), Some(b)) => {
                w >= self.min && w <= self.max && b >= self.min && b <= self.max
            }
            _ => false, // No ELO tags = reject
        }
    }
}
