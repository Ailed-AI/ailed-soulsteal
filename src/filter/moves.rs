use crate::filter::GameFilter;
use crate::game::RawGame;

/// Filter games by move count.
pub struct MovesFilter {
    min: Option<usize>,
    max: Option<usize>,
}

impl MovesFilter {
    pub fn new(min: Option<usize>, max: Option<usize>) -> Self {
        Self { min, max }
    }
}

impl GameFilter for MovesFilter {
    fn accept(&self, game: &RawGame) -> bool {
        let count = game.moves.len();
        if let Some(min) = self.min {
            if count < min {
                return false;
            }
        }
        if let Some(max) = self.max {
            if count > max {
                return false;
            }
        }
        true
    }
}
