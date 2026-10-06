#![forbid(unsafe_code)]

use std::io::{self, IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use slopcop::config::Config;
use slopcop::git;
use slopcop::init;
use slopcop::papertrail;
use slopcop::reporting::{
    HtmlContext, write_github, write_html, write_json, write_sarif, write_text,
};
use slopcop::rules::metadata_registry;
use slopcop::{ScanOptions, ScanResult, Severity, SourceFile, scan_paths, scan_sources};

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
    CommitMessage(CommitMessageArgs),
    Explain {
        rule_id: String,
    },
    History(HistoryArgs),
    Init,
    #[cfg(feature = "lsp")]
    Lsp,
    Rules,
}

#[derive(Debug, Args)]
struct CommitMessageArgs {
    #[arg(value_name = "FILE")]
    message_file: PathBuf,

    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,

    #[arg(long)]
    quiet: bool,
}

#[derive(Debug, Args)]
struct HistoryArgs {
    #[arg(long, value_name = "REV")]
    base: Option<String>,

    #[arg(long, default_value_t = 50, value_name = "COUNT")]
    max_count: usize,

    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,

    #[arg(long)]
    quiet: bool,
}

#[derive(Debug, Args)]
struct CheckArgs {
    #[arg(value_name = "PATH", default_value = ".")]
    paths: Vec<PathBuf>,

    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,

    #[arg(long)]
    quiet: bool,

    /// Read one file from standard input instead of scanning paths.
    #[arg(
        long,
        requires = "stdin_filename",
        conflicts_with_all = ["paths", "staged", "diff", "changed"]
    )]
    stdin: bool,

    /// Path that standard input is linted as. It selects the language, the configuration, and
    /// the path filters; the file does not need to exist.
    #[arg(long, value_name = "PATH", requires = "stdin")]
    stdin_filename: Option<PathBuf>,

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
    Html,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Benchmark) => run_benchmark(),
        Some(Command::Check(args)) => load_and_check(&args, cli.config.as_deref()),
        Some(Command::CommitMessage(args)) => {
            load_and_check_commit_message(&args, cli.config.as_deref())
        }
        Some(Command::Explain { rule_id }) => explain(&rule_id),
        Some(Command::History(args)) => load_and_check_history(&args, cli.config.as_deref()),
        Some(Command::Init) => initialize(),
        #[cfg(feature = "lsp")]
        Some(Command::Lsp) => serve_language_server(),
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

#[cfg(feature = "lsp")]
fn serve_language_server() -> ExitCode {
    match slopcop::lsp::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("slopcop: language server failed: {error}");
            ExitCode::from(3)
        }
    }
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

fn load_and_check(args: &CheckArgs, config_path: Option<&Path>) -> ExitCode {
    let config = if let Some(filename) = &args.stdin_filename {
        // Editors pipe buffers from arbitrary working directories, so start beside the file.
        let start = filename
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        Config::load_from(config_path, Some(start)).map_err(|error| eprintln!("slopcop: {error}"))
    } else {
        load_config(config_path)
    };
    let Ok(config) = config else {
        return ExitCode::from(2);
    };
    run_check(args, config)
}

fn load_and_check_commit_message(args: &CommitMessageArgs, config_path: Option<&Path>) -> ExitCode {
    let Ok(config) = load_config(config_path) else {
        return ExitCode::from(2);
    };
    let result = match papertrail::scan_message_file(&args.message_file, &config) {
        Ok(result) => result,
        Err(error) => return papertrail_error(&error),
    };
    write_result(&result, args.format, args.quiet, config.fail_level)
}

fn load_and_check_history(args: &HistoryArgs, config_path: Option<&Path>) -> ExitCode {
    if args.max_count == 0 {
        eprintln!("slopcop: --max-count must be greater than zero");
        return ExitCode::from(2);
    }
    let Ok(config) = load_config(config_path) else {
        return ExitCode::from(2);
    };
    let result = match papertrail::scan_history(args.base.as_deref(), args.max_count, &config) {
        Ok(result) => result,
        Err(error) => return papertrail_error(&error),
    };
    write_result(&result, args.format, args.quiet, config.fail_level)
}

fn load_config(path: Option<&Path>) -> Result<Config, ()> {
    Config::load(path).map_err(|error| {
        eprintln!("slopcop: {error}");
    })
}

fn run_check(args: &CheckArgs, config: Config) -> ExitCode {
    let fail_level = config.fail_level;
    let options = ScanOptions {
        max_file_size: config.max_file_size,
        config,
    };
    let (mut result, changed_lines) = if let Some(path) = args.stdin_filename.clone() {
        let mut bytes = Vec::new();
        if let Err(error) = io::stdin().lock().read_to_end(&mut bytes) {
            eprintln!("slopcop: could not read standard input: {error}");
            return ExitCode::from(3);
        }
        (
            scan_sources(vec![SourceFile { path, bytes }], &options),
            None,
        )
    } else if args.git.staged {
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

    write_result(&result, args.format, args.quiet, fail_level)
}

fn write_result(
    result: &ScanResult,
    format: OutputFormat,
    quiet: bool,
    fail_level: Severity,
) -> ExitCode {
    let stdout = io::stdout();
    let color = stdout.is_terminal() && std::env::var_os("NO_COLOR").is_none();
    let output = match format {
        OutputFormat::Text => {
            write_text(stdout.lock(), result, quiet, color).map_err(|error| error.to_string())
        }
        OutputFormat::Json => write_json(stdout.lock(), result).map_err(|error| error.to_string()),
        OutputFormat::Sarif => {
            write_sarif(stdout.lock(), result).map_err(|error| error.to_string())
        }
        OutputFormat::Github => {
            write_github(stdout.lock(), &result.findings).map_err(|error| error.to_string())
        }
        OutputFormat::Html => write_html(stdout.lock(), result, &HtmlContext::default())
            .map_err(|error| error.to_string()),
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

fn papertrail_error(error: &papertrail::PapertrailError) -> ExitCode {
    eprintln!("slopcop: {error}");
    ExitCode::from(2)
}

fn explain(rule_id: &str) -> ExitCode {
    let rules = metadata_registry();
    let Some(metadata) = rules
        .iter()
        .find(|metadata| metadata.id.eq_ignore_ascii_case(rule_id))
    else {
        eprintln!("slopcop: unknown rule {rule_id}");
        return ExitCode::from(2);
    };
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
    for metadata in metadata_registry() {
        println!(
            "{}\t{}\t{}",
            metadata.id, metadata.default_severity, metadata.description
        );
    }
    ExitCode::SUCCESS
}
