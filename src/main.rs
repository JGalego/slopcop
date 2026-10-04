#![forbid(unsafe_code)]

use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use slopcop::config::Config;
use slopcop::git;
use slopcop::init;
use slopcop::reporting::{write_github, write_json, write_sarif, write_text};
use slopcop::rules::registry;
use slopcop::{ScanOptions, scan_paths, scan_sources};

#[derive(Debug, Parser)]
#[command(name = "slopcop", version, about = "Slop stops here.")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[command(flatten)]
    scan: CheckArgs,

    #[arg(long, global = true, value_name = "FILE")]
    config: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Benchmark,
    Check(CheckArgs),
    Explain { rule_id: String },
    Init,
    Rules,
}

#[derive(Debug, Args)]
struct CheckArgs {
    #[arg(value_name = "PATH", default_value = ".")]
    paths: Vec<PathBuf>,

    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,

    #[arg(long)]
    quiet: bool,

    #[command(flatten)]
    git: GitArgs,
}

#[derive(Debug, Args)]
struct GitArgs {
    #[arg(long, conflicts_with_all = ["diff", "changed"])]
    staged: bool,

    #[arg(long, conflicts_with_all = ["staged", "changed"])]
    diff: bool,

    #[arg(long, conflicts_with_all = ["staged", "diff"])]
    changed: bool,

    #[arg(long, value_name = "REV", requires = "changed")]
    base: Option<String>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
    Sarif,
    Github,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Benchmark) => run_benchmark(),
        Some(Command::Check(args)) => load_and_check(&args, cli.config.as_deref()),
        Some(Command::Explain { rule_id }) => explain(&rule_id),
        Some(Command::Init) => initialize(),
        Some(Command::Rules) => list_rules(),
        None => load_and_check(&cli.scan, cli.config.as_deref()),
    }
}

fn run_benchmark() -> ExitCode {
    println!("slopcop benchmark\n");
    for measurement in slopcop::benchmark::run() {
        println!(
            "{:<12} {:>8.1} ms  {:>8.0} files/s  {:>7.1} MB/s  {} files",
            measurement.name,
            measurement.elapsed.as_secs_f64() * 1_000.0,
            measurement.files_per_second(),
            measurement.megabytes_per_second(),
            measurement.files,
        );
    }
    ExitCode::SUCCESS
}

fn initialize() -> ExitCode {
    let directory = match std::env::current_dir() {
        Ok(directory) => directory,
        Err(error) => {
            eprintln!("slopcop: could not determine current directory: {error}");
            return ExitCode::from(3);
        }
    };
    match init::initialize(&directory) {
        Ok(result) => {
            for path in result.created {
                println!("created {}", path.display());
            }
            for path in result.existing {
                println!("kept {}", path.display());
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("slopcop: {error}");
            ExitCode::from(3)
        }
    }
}

fn load_and_check(args: &CheckArgs, config_path: Option<&std::path::Path>) -> ExitCode {
    let config = match Config::load(config_path) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("slopcop: {error}");
            return ExitCode::from(2);
        }
    };
    run_check(args, config)
}

fn run_check(args: &CheckArgs, config: Config) -> ExitCode {
    let fail_level = config.fail_level;
    let options = ScanOptions {
        max_file_size: config.max_file_size,
        config,
    };
    let (mut result, changed_lines) = if args.git.staged {
        match git::staged(&args.paths) {
            Ok(selection) => {
                let git::Selection {
                    sources,
                    changed_lines,
                } = selection;
                (scan_sources(sources, &options), changed_lines)
            }
            Err(error) => return git_error(&error),
        }
    } else if args.git.diff || args.git.changed {
        let base = args
            .git
            .changed
            .then(|| args.git.base.as_deref().unwrap_or("origin/main"));
        match git::diff(&args.paths, base) {
            Ok(selection) => {
                let git::Selection {
                    sources,
                    changed_lines,
                } = selection;
                (scan_sources(sources, &options), changed_lines)
            }
            Err(error) => return git_error(&error),
        }
    } else {
        match scan_paths(&args.paths, &options) {
            Ok(result) => (result, None),
            Err(error) => {
                eprintln!("slopcop: {error}");
                return ExitCode::from(if error.is_usage_error() { 2 } else { 3 });
            }
        }
    };
    if let Some(changed_lines) = &changed_lines {
        result.findings.retain(|finding| {
            changed_lines
                .get(&options.config.path_filter.absolute(&finding.path))
                .is_some_and(|ranges| {
                    ranges.iter().any(|range| {
                        finding.location.line >= range.start && finding.location.line <= range.end
                    })
                })
        });
    }

    let stdout = io::stdout();
    let color = stdout.is_terminal() && std::env::var_os("NO_COLOR").is_none();
    let output = match args.format {
        OutputFormat::Text => {
            write_text(stdout.lock(), &result, args.quiet, color).map_err(|error| error.to_string())
        }
        OutputFormat::Json => write_json(stdout.lock(), &result).map_err(|error| error.to_string()),
        OutputFormat::Sarif => {
            write_sarif(stdout.lock(), &result).map_err(|error| error.to_string())
        }
        OutputFormat::Github => {
            write_github(stdout.lock(), &result.findings).map_err(|error| error.to_string())
        }
    };
    if let Err(error) = output {
        eprintln!("slopcop: could not write output: {error}");
        return ExitCode::from(3);
    }

    if result
        .findings
        .iter()
        .all(|finding| finding.severity < fail_level)
    {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn git_error(error: &git::GitError) -> ExitCode {
    eprintln!("slopcop: {error}");
    ExitCode::from(2)
}

fn explain(rule_id: &str) -> ExitCode {
    let rules = registry();
    let Some(rule) = rules
        .iter()
        .find(|rule| rule.metadata().id.eq_ignore_ascii_case(rule_id))
    else {
        eprintln!("slopcop: unknown rule {rule_id}");
        return ExitCode::from(2);
    };
    let metadata = rule.metadata();
    println!("{}: {}", metadata.id, metadata.description);
    println!("module: {}", metadata.module);
    println!("default severity: {}", metadata.default_severity);
    println!("\n{}", metadata.rationale);
    println!("\nExamples:");
    for example in metadata.examples {
        println!("  {example}");
    }
    println!("\nSuggestion: {}", metadata.suggestion);
    println!("\nFalse positives: {}", metadata.false_positives);
    println!(
        "\nConfigure with `[slopcop.rules]` and `{} = \"info|warning|error|off\"`.",
        metadata.id
    );
    ExitCode::SUCCESS
}

fn list_rules() -> ExitCode {
    for rule in registry() {
        let metadata = rule.metadata();
        println!(
            "{}\t{}\t{}",
            metadata.id, metadata.default_severity, metadata.description
        );
    }
    ExitCode::SUCCESS
}
