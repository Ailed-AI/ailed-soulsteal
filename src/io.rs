use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

use anyhow::{Context, Result};

/// Open an input file, auto-detecting zstd compression from extension.
pub fn open_input(path: &Path) -> Result<Box<dyn BufRead>> {
    let file = File::open(path)
        .with_context(|| format!("Failed to open {}", path.display()))?;

    if path.extension().map_or(false, |ext| ext == "zst" || ext == "zstd") {
        let decoder = zstd::Decoder::new(file)
            .with_context(|| "Failed to create zstd decoder")?;
        Ok(Box::new(BufReader::with_capacity(1024 * 1024, decoder)))
    } else {
        Ok(Box::new(BufReader::with_capacity(1024 * 1024, file)))
    }
}

/// Open stdin as a buffered reader.
pub fn open_stdin() -> Box<dyn BufRead> {
    Box::new(BufReader::with_capacity(1024 * 1024, io::stdin()))
}
