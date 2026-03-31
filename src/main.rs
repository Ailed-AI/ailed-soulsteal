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

    /// Dump games from a .somabin file.
    Dump {
        /// Path to .somabin file.
        file: PathBuf,

        /// Vocabulary JSON file (for decoding token IDs to UCI moves).
        #[arg(long)]
        vocab: Option<PathBuf>,

        /// Starting game index.
        #[arg(long, default_value = "0")]
        game: usize,

        /// Number of games to dump.
        #[arg(long, default_value = "5")]
        count: usize,

        /// Output as JSON instead of human-readable.
        #[arg(long)]
        json: bool,
    },

    /// Export statistics from a .somabin file.
    Stats {
        /// Path to .somabin file.
        file: PathBuf,
    },

    /// Split a .somabin into train/val JSONL files for training.
    Split {
        /// Path to .somabin file.
        file: PathBuf,

        /// Output directory for train.jsonl and val.jsonl.
        #[arg(short, long, default_value = ".")]
        output: PathBuf,

        /// Vocabulary JSON file (for decoding token IDs to UCI moves).
        #[arg(long)]
        vocab: Option<PathBuf>,

        /// Validation split ratio (0.0-1.0).
        #[arg(long, default_value = "0.1")]
        val_ratio: f64,

        /// Random seed for shuffling before split.
        #[arg(long, default_value = "42")]
        seed: u64,

        /// Output raw token IDs instead of UCI moves.
        #[arg(long)]
        raw: bool,
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
        Commands::Dump { file, vocab, game, count, json } => cmd_dump(file, vocab, game, count, json),
        Commands::Stats { file } => cmd_stats(file),
        Commands::Split { file, output, vocab, val_ratio, seed, raw } => {
            cmd_split(file, output, vocab, val_ratio, seed, raw)
        }
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

fn cmd_dump(file: PathBuf, vocab_path: Option<PathBuf>, start: usize, count: usize, json_mode: bool) -> Result<()> {
    let reader = somabin::SomabinReader::open(&file)?;

    // Load vocab for decoding if provided
    let vocab = vocab_path.as_ref().map(|p| Vocab::from_json(p)).transpose()?;

    let special_names: std::collections::HashMap<u16, &str> = [
        (0, "PAD"), (1, "BOS"), (2, "EOS"), (3, "WIN"), (4, "DRAW"), (5, "LOSS"),
    ].into();

    let category_names = ["pawn", "knight", "bishop", "rook", "queen", "king"];
    let outcome_names = ["win", "draw", "loss"];

    let end = std::cmp::min(start + count, reader.num_games());

    for i in start..end {
        let game = reader.read_game(i)?;
        let outcome_str = if (game.outcome as usize) < outcome_names.len() {
            outcome_names[game.outcome as usize]
        } else {
            "unknown"
        };

        if json_mode {
            let moves: Vec<String> = game.token_ids.iter().enumerate().map(|(j, &tid)| {
                let name = if let Some(n) = special_names.get(&tid) {
                    n.to_string()
                } else if let Some(ref v) = vocab {
                    v.get_action(tid).unwrap_or("?").to_string()
                } else {
                    format!("#{}", tid)
                };
                let turn = if game.turn_ids[j] == 0 { "W" } else { "B" };
                let cat = category_names.get(game.category_ids[j] as usize).unwrap_or(&"?");
                format!("{{\"token\":\"{}\",\"turn\":\"{}\",\"piece\":\"{}\"}}", name, turn, cat)
            }).collect();
            println!("{{\"game\":{},\"length\":{},\"outcome\":\"{}\",\"moves\":[{}]}}",
                i, game.token_ids.len(), outcome_str, moves.join(","));
        } else {
            println!("\n=== Game {} ({} tokens, outcome={}) ===", i, game.token_ids.len(), outcome_str);
            let mut parts = Vec::new();
            for (j, &tid) in game.token_ids.iter().enumerate() {
                let turn = if game.turn_ids[j] == 0 { "W" } else { "B" };
                let cat = category_names.get(game.category_ids[j] as usize).unwrap_or(&"?");

                if let Some(name) = special_names.get(&tid) {
                    parts.push(format!("[{}]", name));
                } else if let Some(ref v) = vocab {
                    let name = v.get_action(tid).unwrap_or("?");
                    parts.push(format!("{}({},{})", name, turn, cat));
                } else {
                    parts.push(format!("#{}({},{})", tid, turn, cat));
                }
            }
            println!("{}", parts.join(" "));
        }
    }

    Ok(())
}

fn cmd_stats(file: PathBuf) -> Result<()> {
    let reader = somabin::SomabinReader::open(&file)?;
    let n = reader.num_games();

    let mut total_tokens: u64 = 0;
    let mut min_len = u16::MAX as usize;
    let mut max_len = 0usize;
    let mut outcomes = [0u64; 4]; // win, draw, loss, unknown
    let mut category_counts = [0u64; 6];

    for i in 0..n {
        let game = reader.read_game(i)?;
        let len = game.token_ids.len();
        total_tokens += len as u64;
        if len < min_len { min_len = len; }
        if len > max_len { max_len = len; }

        match game.outcome {
            0 => outcomes[0] += 1,
            1 => outcomes[1] += 1,
            2 => outcomes[2] += 1,
            _ => outcomes[3] += 1,
        }

        for &cat in &game.category_ids {
            if (cat as usize) < 6 {
                category_counts[cat as usize] += 1;
            }
        }
    }

    let avg_len = total_tokens as f64 / n as f64;

    println!("Games:          {}", n);
    println!("Total tokens:   {}", total_tokens);
    println!("Avg seq len:    {:.1}", avg_len);
    println!("Min seq len:    {}", min_len);
    println!("Max seq len:    {}", max_len);
    println!();
    println!("Outcomes:");
    println!("  Win:          {} ({:.1}%)", outcomes[0], 100.0 * outcomes[0] as f64 / n as f64);
    println!("  Draw:         {} ({:.1}%)", outcomes[1], 100.0 * outcomes[1] as f64 / n as f64);
    println!("  Loss:         {} ({:.1}%)", outcomes[2], 100.0 * outcomes[2] as f64 / n as f64);
    if outcomes[3] > 0 {
        println!("  Unknown:      {} ({:.1}%)", outcomes[3], 100.0 * outcomes[3] as f64 / n as f64);
    }
    println!();
    println!("Piece moves:");
    let piece_names = ["Pawn", "Knight", "Bishop", "Rook", "Queen", "King"];
    let total_moves: u64 = category_counts.iter().sum();
    for (i, name) in piece_names.iter().enumerate() {
        println!("  {:8} {} ({:.1}%)", name, category_counts[i],
            100.0 * category_counts[i] as f64 / total_moves.max(1) as f64);
    }

    Ok(())
}

fn cmd_split(
    file: PathBuf,
    output_dir: PathBuf,
    vocab_path: Option<PathBuf>,
    val_ratio: f64,
    seed: u64,
    raw: bool,
) -> Result<()> {
    use std::io::BufWriter;

    let reader = somabin::SomabinReader::open(&file)?;
    let n = reader.num_games();
    let vocab = vocab_path.as_ref().map(|p| Vocab::from_json(p)).transpose()?;

    let special_names: std::collections::HashMap<u16, &str> = [
        (0, "PAD"), (1, "BOS"), (2, "EOS"), (3, "WIN"), (4, "DRAW"), (5, "LOSS"),
    ].into();
    let outcome_names = ["win", "draw", "loss"];

    // Shuffle indices with seed
    let mut indices: Vec<usize> = (0..n).collect();
    // Simple Fisher-Yates with seeded LCG
    let mut rng_state = seed;
    for i in (1..indices.len()).rev() {
        rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let j = (rng_state >> 33) as usize % (i + 1);
        indices.swap(i, j);
    }

    let val_count = (n as f64 * val_ratio) as usize;
    let train_count = n - val_count;
    let train_indices = &indices[..train_count];
    let val_indices = &indices[train_count..];

    std::fs::create_dir_all(&output_dir)?;
    let train_path = output_dir.join("train.jsonl");
    let val_path = output_dir.join("val.jsonl");

    let mut train_file = BufWriter::new(File::create(&train_path)?);
    let mut val_file = BufWriter::new(File::create(&val_path)?);

    let write_game = |f: &mut BufWriter<File>, idx: usize| -> Result<()> {
        let game = reader.read_game(idx)?;
        let outcome_str = if (game.outcome as usize) < outcome_names.len() {
            outcome_names[game.outcome as usize]
        } else {
            "unknown"
        };

        if raw {
            // Raw token IDs
            let tokens: Vec<String> = game.token_ids.iter().map(|t| t.to_string()).collect();
            let turns: Vec<String> = game.turn_ids.iter().map(|t| t.to_string()).collect();
            let cats: Vec<String> = game.category_ids.iter().map(|t| t.to_string()).collect();
            writeln!(f, "{{\"tokens\":[{}],\"turns\":[{}],\"categories\":[{}],\"outcome\":\"{}\"}}",
                tokens.join(","), turns.join(","), cats.join(","), outcome_str)?;
        } else {
            // UCI moves
            let mut moves = Vec::new();
            for &tid in &game.token_ids {
                if let Some(name) = special_names.get(&tid) {
                    // skip special tokens in moves list
                    if tid >= 3 && tid <= 5 {
                        continue; // outcome tokens handled separately
                    }
                    if tid <= 2 {
                        continue; // BOS/EOS/PAD
                    }
                } else if let Some(ref v) = vocab {
                    if let Some(uci) = v.get_action(tid) {
                        moves.push(uci.to_string());
                    }
                } else {
                    moves.push(format!("#{}", tid));
                }
            }
            let moves_json: Vec<String> = moves.iter().map(|m| format!("\"{}\"", m)).collect();
            writeln!(f, "{{\"moves\":[{}],\"outcome\":\"{}\",\"length\":{}}}",
                moves_json.join(","), outcome_str, moves.len())?;
        }
        Ok(())
    };

    eprintln!("Splitting {} games: {} train, {} val (ratio={:.0}%, seed={})",
        n, train_count, val_count, val_ratio * 100.0, seed);

    for &idx in train_indices {
        write_game(&mut train_file, idx)?;
    }
    for &idx in val_indices {
        write_game(&mut val_file, idx)?;
    }

    train_file.flush()?;
    val_file.flush()?;

    let train_size = std::fs::metadata(&train_path)?.len();
    let val_size = std::fs::metadata(&val_path)?.len();

    eprintln!("  Train: {} ({:.1} MB)", train_path.display(), train_size as f64 / 1_048_576.0);
    eprintln!("  Val:   {} ({:.1} MB)", val_path.display(), val_size as f64 / 1_048_576.0);

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
