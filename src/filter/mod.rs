pub mod elo;
pub mod result;
pub mod moves;

use crate::game::RawGame;

/// A predicate that decides whether a game should be included.
pub trait GameFilter {
    fn accept(&self, game: &RawGame) -> bool;
}

/// Chain of filters — all must accept for a game to pass.
pub struct FilterChain {
    filters: Vec<Box<dyn GameFilter>>,
}

impl FilterChain {
    pub fn new() -> Self {
        Self { filters: Vec::new() }
    }

    pub fn add(&mut self, filter: Box<dyn GameFilter>) {
        self.filters.push(filter);
    }

    pub fn accept(&self, game: &RawGame) -> bool {
        self.filters.iter().all(|f| f.accept(game))
    }

    pub fn is_empty(&self) -> bool {
        self.filters.is_empty()
    }
}
