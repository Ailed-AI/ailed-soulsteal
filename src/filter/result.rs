use crate::filter::GameFilter;
use crate::game::RawGame;

pub enum ResultFilter {
    /// Only games with a decisive result (1-0 or 0-1).
    Decisive,
    /// Only draws (1/2-1/2).
    DrawsOnly,
    /// All results (no filtering).
    All,
}

impl ResultFilter {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "decisive" => Some(Self::Decisive),
            "draws" => Some(Self::DrawsOnly),
            "all" => Some(Self::All),
            _ => None,
        }
    }
}

impl GameFilter for ResultFilter {
    fn accept(&self, game: &RawGame) -> bool {
        match self {
            Self::All => true,
            Self::Decisive => matches!(
                game.result.as_deref(),
                Some("1-0") | Some("0-1")
            ),
            Self::DrawsOnly => matches!(
                game.result.as_deref(),
                Some("1/2-1/2")
            ),
        }
    }
}
