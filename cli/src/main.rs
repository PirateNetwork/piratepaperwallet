mod version;

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, ValueEnum};
use piratepaperlib::paper::{self, Pool, SeedSource, WalletOptions};
use piratepaperlib::pdf;

#[derive(Clone, Copy, ValueEnum)]
enum CliPool {
    Ironwood,
    Sapling,
}

impl From<CliPool> for Pool {
    fn from(pool: CliPool) -> Self {
        match pool {
            CliPool::Ironwood => Pool::Ironwood,
            CliPool::Sapling => Pool::Sapling,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum OutputFormat {
    Json,
    Pdf,
}

#[derive(Parser)]
#[command(name = "piratepaperwallet", version = version::version(), about = "Offline Pirate Chain Ironwood paper wallet generator", after_help = "Ironwood funds can be received after the October 3, 2026 network activation.\nFor recovery, import the extended spending key into an Ironwood-enabled wallet.\nThis generator uses ZIP-32 compatible with full node 6.0.7+ and the updated light wallet.\nUse --pool sapling to recover an older Sapling paper wallet.")]
struct Args {
    /// Shielded pool. Sapling is for recovering legacy paper wallets.
    #[arg(long, value_enum, default_value = "ironwood")]
    pool: CliPool,

    /// JSON or printable PDF output.
    #[arg(short, long, value_enum, default_value = "json")]
    format: OutputFormat,

    /// Output file (required for PDF).
    #[arg(value_name = "OUTPUT")]
    output: Option<PathBuf>,

    /// Output file, alternate to the positional OUTPUT argument.
    #[arg(short = 'o', long = "output", conflicts_with = "output")]
    output_option: Option<PathBuf>,

    /// Add user entropy to OS randomness.
    #[arg(short, long, conflicts_with_all = ["hdseed", "phrase", "partialphrase", "vanity"])]
    entropy: Option<String>,

    /// Recover accounts from a 24-word English BIP39 phrase.
    #[arg(short, long, conflicts_with_all = ["hdseed", "partialphrase", "vanity"])]
    phrase: Option<String>,

    /// Recover candidate accounts from a 23-word phrase with one missing word.
    #[arg(long, conflicts_with_all = ["hdseed", "phrase", "vanity"])]
    partialphrase: Option<String>,

    /// Recover accounts from the 32-byte paper wallet HD seed in hex.
    #[arg(short = 's', long)]
    hdseed: Option<String>,

    /// Search for a default address whose characters after pirate1 or zs1 start with PREFIX.
    #[arg(long, conflicts_with_all = ["hdseed", "phrase", "partialphrase"])]
    vanity: Option<String>,

    /// Number of workers for vanity search.
    #[arg(long, default_value_t = 1)]
    threads: usize,

    /// Number of shielded accounts to generate.
    #[arg(
        short = 'z',
        long = "zaddrs",
        visible_alias = "count",
        default_value_t = 1
    )]
    count: u32,

    /// ZIP-32 coin type. Mainnet Pirate Chain uses 141.
    #[arg(short = 't', long = "cointype", default_value_t = 141)]
    coin_type: u32,

    /// Give each generated account a separate HD seed.
    #[arg(short = 'n', long)]
    nohd: bool,

    /// Derive from the raw HD seed without BIP39 stretching.
    #[arg(short = 'b', long)]
    nobip39: bool,
}

fn run() -> Result<(), String> {
    let args = Args::parse();
    let output = args.output_option.or(args.output);
    if matches!(args.format, OutputFormat::Pdf) && output.is_none() {
        return Err("PDF output requires a file name".to_owned());
    }
    if args.vanity.is_none() && args.threads != 1 {
        return Err("--threads is only used with --vanity".to_owned());
    }
    let options = WalletOptions {
        pool: args.pool.into(),
        count: args.count,
        coin_type: args.coin_type,
        nohd: args.nohd,
        nobip39: args.nobip39,
    };
    let records = if let Some(prefix) = args.vanity {
        paper::generate_vanity_wallet(&prefix, args.threads, options)?
    } else if let Some(phrase) = args.phrase {
        paper::generate_wallet(SeedSource::Phrase(&phrase), options)?
    } else if let Some(phrase) = args.partialphrase {
        paper::generate_wallet(SeedSource::PartialPhrase(&phrase), options)?
    } else if let Some(seed) = args.hdseed {
        let seed = hex::decode(seed).map_err(|_| "HD seed must be hexadecimal".to_owned())?;
        paper::generate_wallet(SeedSource::HdSeed(&seed), options)?
    } else {
        let entropy = if let Some(entropy) = args.entropy {
            entropy
        } else {
            eprint!("Optional extra entropy (press Enter to skip): ");
            io::stderr().flush().map_err(|e| e.to_string())?;
            let mut entropy = String::new();
            io::stdin()
                .read_line(&mut entropy)
                .map_err(|e| e.to_string())?;
            entropy
        };
        paper::generate_wallet(SeedSource::Random(entropy.as_bytes()), options)?
    };
    match args.format {
        OutputFormat::Json => {
            let json = paper::to_json(&records)?;
            if let Some(path) = output {
                let mut file_options = OpenOptions::new();
                file_options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    file_options.mode(0o600);
                }
                let mut file = file_options
                    .open(&path)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                if let Err(error) = file.write_all(json.as_bytes()) {
                    drop(file);
                    let cleanup = std::fs::remove_file(&path);
                    return Err(match cleanup {
                        Ok(()) => format!("{}: {error}", path.display()),
                        Err(cleanup_error) => format!(
                            "{}: {error}; could not remove incomplete output: {cleanup_error}",
                            path.display()
                        ),
                    });
                }
                println!("Wrote {}", path.display());
            } else {
                println!("{json}");
            }
        }
        OutputFormat::Pdf => {
            let path = output.expect("PDF path validated above");
            pdf::save_to_pdf(&records, &path)?;
            println!("Wrote {}", path.display());
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        }
    }
}
