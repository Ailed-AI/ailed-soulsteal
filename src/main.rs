use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use ailed_soulsteal::filter::elo::EloFilter;
use ailed_soulsteal::filter::moves::MovesFilter;
use ailed_soulsteal::filter::result::ResultFilter;
use ailed_soulsteal::filter::FilterChain;
use ailed_soulsteal::format::somabin::{self, SomabinWriter};
use ailed_soulsteal::format::stream;
use ailed_soulsteal::game::chess::{ChessTokenizer, PgnParser};
use ailed_soulsteal::game::{GameParser, GameTokenizer};
use ailed_soulsteal::vocab::Vocab;

#[derive(Parser)]
#[command(name = "soulsteal", version, about = "Fast game data extractor for AILED-Soma")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Convert PGN to tokenized binary format.
    Tokenize {
        /// Input PGN file (or .pgn.zst for compressed). Use - for stdin.
        input: PathBuf,

        /// Output .somabin file.
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Vocabulary JSON file (UCI move -> token ID).
        #[arg(long)]
        vocab: PathBuf,

        /// ELO range filter, e.g. "1000:1800".
        #[arg(long)]
        elo: Option<String>,

        /// Minimum number of moves.
        #[arg(long, default_value = "4")]
        min_moves: usize,

        /// Maximum number of games to process.
        #[arg(long)]
        max_games: Option<usize>,

        /// Result filter: "decisive", "draws", or "all".
        #[arg(long, default_value = "all")]
        result: String,

        /// Stream tokenized games to stdout instead of writing binary.
        #[arg(long)]
        stream: bool,
    },

    /// Display info about a .somabin file.
    Info {
        /// Path to .somabin file.
        file: PathBuf,
    },

    /// Generate or export vocabulary.
    Vocab {
        /// Generate vocabulary (all possible chess UCI moves).
        #[arg(long)]
        generate: bool,

        /// Output JSON file.
        #[arg(short, long)]
        output: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Tokenize {
            input,
            output,
            vocab,
            elo,
            min_moves,
            max_games,
            result,
            stream: stream_mode,
        } => {
            cmd_tokenize(input, output, vocab, elo, min_moves, max_games, result, stream_mode)
        }
        Commands::Info { file } => cmd_info(file),
        Commands::Vocab { generate, output } => cmd_vocab(generate, output),
    }
}

fn cmd_tokenize(
    input: PathBuf,
    output: Option<PathBuf>,
    vocab_path: PathBuf,
    elo: Option<String>,
    min_moves: usize,
    max_games: Option<usize>,
    result: String,
    stream_mode: bool,
) -> Result<()> {
    let start = Instant::now();

    // Load vocabulary
    let vocab = Vocab::from_json(&vocab_path)?;
    eprintln!("Vocabulary: {} tokens", vocab.vocab_size);

    // Build filter chain
    let mut filters = FilterChain::new();
    if let Some(elo_str) = &elo {
        let f = EloFilter::from_str(elo_str)
            .ok_or_else(|| anyhow::anyhow!("Invalid ELO range: {}. Use format 1000:1800", elo_str))?;
        filters.add(Box::new(f));
    }
    if min_moves > 0 {
        filters.add(Box::new(MovesFilter::new(Some(min_moves), None)));
    }
    if result != "all" {
        let f = ResultFilter::from_str(&result)
            .ok_or_else(|| anyhow::anyhow!("Invalid result filter: {}. Use decisive, draws, or all", result))?;
        filters.add(Box::new(f));
    }

    // Open input
    let reader = if input.to_str() == Some("-") {
        ailed_soulsteal::io::open_stdin()
    } else {
        ailed_soulsteal::io::open_input(&input)?
    };

    // Parse
    let parser = PgnParser;
    let tokenizer = ChessTokenizer;
    let games_iter = parser.parse(reader);

    // Output setup
    let mut writer: Option<SomabinWriter<BufWriter<File>>> = if !stream_mode {
        let out_path = output.as_ref()
            .ok_or_else(|| anyhow::anyhow!("--output is required in batch mode (or use --stream)"))?;
        let file = File::create(out_path)
            .with_context(|| format!("Failed to create {}", out_path.display()))?;
        let mut w = SomabinWriter::new(
            BufWriter::new(file),
            0, // chess
            vocab.vocab_size,
            vocab.special_offset,
            6, // chess category dims
        );
        w.begin(0)?;
        Some(w)
    } else {
        None
    };

    let mut stdout = if stream_mode {
        Some(BufWriter::new(io::stdout()))
    } else {
        None
    };

    let mut total_parsed = 0u64;
    let mut total_filtered = 0u64;
    let mut total_tokenized = 0u64;
    let mut total_failed = 0u64;

    for game in games_iter {
        total_parsed += 1;

        // Apply filters
        if !filters.accept(&game) {
            total_filtered += 1;
            continue;
        }

        // Check max games
        if let Some(max) = max_games {
            if total_tokenized >= max as u64 {
                break;
            }
        }

        // Tokenize
        match tokenizer.tokenize(&game, &vocab) {
            Some(tgame) => {
                if let Some(ref mut w) = writer {
                    w.write_game(&tgame)?;
                }
                if let Some(ref mut s) = stdout {
                    stream::write_stream_record(s, &tgame)?;
                }
                total_tokenized += 1;

                // Progress
                if total_tokenized % 100_000 == 0 {
                    let elapsed = start.elapsed().as_secs_f64();
                    let rate = total_tokenized as f64 / elapsed;
                    eprintln!(
                        "  {} games tokenized ({} parsed, {} filtered, {} failed) [{:.0} games/sec]",
                        total_tokenized, total_parsed, total_filtered, total_failed, rate
                    );
                }
            }
            None => {
                total_failed += 1;
            }
        }
    }

    // Finalize
    if let Some(w) = writer {
        w.finalize()?;
    }
    if let Some(ref mut s) = stdout {
        s.flush()?;
    }

    let elapsed = start.elapsed().as_secs_f64();
    let rate = if elapsed > 0.0 { total_tokenized as f64 / elapsed } else { 0.0 };

    eprintln!("\nDone in {:.1}s", elapsed);
    eprintln!("  Parsed:    {}", total_parsed);
    eprintln!("  Filtered:  {}", total_filtered);
    eprintln!("  Failed:    {}", total_failed);
    eprintln!("  Tokenized: {}", total_tokenized);
    eprintln!("  Rate:      {:.0} games/sec", rate);

    if let Some(out_path) = &output {
        if !stream_mode {
            let size = std::fs::metadata(out_path)?.len();
            eprintln!("  Output:    {} ({:.1} MB)", out_path.display(), size as f64 / 1_048_576.0);
        }
    }

    Ok(())
}

fn cmd_info(file: PathBuf) -> Result<()> {
    let info = somabin::read_info(&file)?;
    print!("{}", info);
    Ok(())
}

fn cmd_vocab(generate: bool, output: PathBuf) -> Result<()> {
    if generate {
        eprintln!("Generating chess vocabulary (all possible UCI moves)...");
        let vocab = Vocab::generate_from_pgn(io::empty())?;
        vocab.to_json(&output)?;
        eprintln!("Saved {} tokens to {}", vocab.vocab_size, output.display());
    } else {
        anyhow::bail!("Use --generate to create a vocabulary file");
    }
    Ok(())
}
