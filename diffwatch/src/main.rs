use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::{Args as ClapArgs, Parser, Subcommand};
use diffwatch::batch::Batch;
use diffwatch::config::{DEFAULT_MAX_TEXT_BYTES, DEFAULT_SETTLE_SECONDS};
use diffwatch::history::{list_sessions, read_batch};
use diffwatch::journal::Journal;
use diffwatch::path_filter::PathFilter;
use diffwatch::snapshot::{CaptureOptions, Snapshot};
use diffwatch::stream::BatchStream;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    #[command(flatten)]
    watch: WatchArgs,
}

#[derive(Debug, ClapArgs)]
struct WatchArgs {
    /// Directory to watch.
    #[arg(default_value = ".")]
    root: PathBuf,

    /// Seconds without filesystem events required to close a batch.
    #[arg(long, default_value_t = DEFAULT_SETTLE_SECONDS)]
    settle: f64,

    /// Journal directory. Relative paths are resolved inside the watched root.
    #[arg(long, default_value = ".diffwatch")]
    journal: PathBuf,

    /// Maximum text file size retained for line diffs, in bytes.
    #[arg(long, default_value_t = DEFAULT_MAX_TEXT_BYTES)]
    max_text_bytes: u64,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List recorded watcher sessions.
    History(HistoryArgs),
    /// Print one batch from the latest or selected session.
    Show(ShowArgs),
}

#[derive(Debug, ClapArgs)]
struct HistoryArgs {
    #[arg(default_value = ".")]
    root: PathBuf,

    #[arg(long, default_value = ".diffwatch")]
    journal: PathBuf,
}

#[derive(Debug, ClapArgs)]
struct ShowArgs {
    /// Batch number to print.
    number: u64,

    #[arg(default_value = ".")]
    root: PathBuf,

    #[arg(long, default_value = ".diffwatch")]
    journal: PathBuf,

    /// Session directory name. Defaults to the latest session.
    #[arg(long)]
    session: Option<String>,
}

enum WatchOutput {
    Human,
    Stream(BatchStream<io::Stdout>),
}

impl WatchOutput {
    fn detect() -> Result<Self> {
        if io::stdout().is_terminal() {
            Ok(Self::Human)
        } else {
            Ok(Self::Stream(BatchStream::stdout()?))
        }
    }

    fn status(&self, message: &str) {
        match self {
            Self::Human => println!("{message}"),
            Self::Stream(_) => eprintln!("{message}"),
        }
    }

    fn batch(
        &mut self,
        batch: &Batch,
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
    ) -> Result<()> {
        match self {
            Self::Human => print!("\n{}", batch.render()),
            Self::Stream(stream) => stream.write_batch(batch, started_at, finished_at)?,
        }
        Ok(())
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    match args.command {
        Some(Command::History(args)) => return show_history(args),
        Some(Command::Show(args)) => return show_batch(args),
        None => {}
    }
    watch(args.watch)
}

fn watch(args: WatchArgs) -> Result<()> {
    if !args.settle.is_finite() || args.settle <= 0.0 {
        anyhow::bail!("--settle must be a positive number");
    }

    let root = args
        .root
        .canonicalize()
        .with_context(|| format!("cannot open {}", args.root.display()))?;
    let journal_root = absolute_journal_path(&root, &args.journal);
    let settle = Duration::from_secs_f64(args.settle);
    let capture_options = CaptureOptions {
        max_text_bytes: args.max_text_bytes,
    };
    let mut baseline = Snapshot::capture_with_options(&root, &journal_root, capture_options)?;
    let path_filter = PathFilter::new(&root, &journal_root)?;
    let journal = Journal::create(&journal_root, Utc::now(), args.settle)?;
    let mut output = WatchOutput::detect()?;
    let (sender, receiver) = mpsc::channel();
    let mut watcher: RecommendedWatcher =
        notify::recommended_watcher(move |event: notify::Result<Event>| {
            let _ = sender.send(event);
        })?;
    watcher.watch(&root, RecursiveMode::Recursive)?;

    let stopping = Arc::new(AtomicBool::new(false));
    let stopping_on_signal = Arc::clone(&stopping);
    ctrlc::set_handler(move || stopping_on_signal.store(true, Ordering::SeqCst))
        .context("cannot install Ctrl-C handler")?;

    let status = format!(
        "Watching {} (batch closes after {:.1}s of silence)\nJournal: {}",
        root.display(),
        args.settle,
        journal.directory().display()
    );
    output.status(&status);

    let mut batch_number = 1;
    while !stopping.load(Ordering::SeqCst) {
        let event = match receiver.recv_timeout(Duration::from_millis(200)) {
            Ok(event) => event?,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => anyhow::bail!("filesystem watcher stopped"),
        };
        if !path_filter.includes_any(&event.paths) {
            continue;
        }
        let batch_started_at = Utc::now();

        // Any relevant event opens a batch. Each subsequent event resets the
        // quiet-period timer. We rescan only after the batch has settled.
        while !stopping.load(Ordering::SeqCst) {
            match receiver.recv_timeout(settle) {
                Ok(Ok(event)) if !path_filter.includes_any(&event.paths) => {}
                Ok(Ok(_)) => {}
                Ok(Err(error)) => eprintln!("watch warning: {error}"),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => anyhow::bail!("filesystem watcher stopped"),
            }
        }

        let current = Snapshot::capture_with_options(&root, &journal_root, capture_options)?;
        let batch = Batch::between(batch_number, &baseline, &current);
        baseline = current;
        if batch.is_empty() {
            continue;
        }

        let finished_at = Utc::now();
        output.batch(&batch, batch_started_at, finished_at)?;
        journal.write_batch(&batch, batch_started_at, finished_at)?;
        batch_number += 1;
    }

    output.status(&format!(
        "Stopped. {} batch(es) recorded.",
        batch_number - 1
    ));
    Ok(())
}

fn show_history(args: HistoryArgs) -> Result<()> {
    let root = canonical_root(&args.root)?;
    let journal_root = absolute_journal_path(&root, &args.journal);
    let sessions = list_sessions(&journal_root)?;
    if sessions.is_empty() {
        println!("No diffwatch sessions found in {}", journal_root.display());
        return Ok(());
    }
    for session in sessions {
        println!("{}  {} batch(es)", session.name, session.batch_count);
    }
    Ok(())
}

fn show_batch(args: ShowArgs) -> Result<()> {
    let root = canonical_root(&args.root)?;
    let journal_root = absolute_journal_path(&root, &args.journal);
    print!(
        "{}",
        read_batch(&journal_root, args.session.as_deref(), args.number)?
    );
    Ok(())
}

fn canonical_root(root: &Path) -> Result<PathBuf> {
    root.canonicalize()
        .with_context(|| format!("cannot open {}", root.display()))
}

fn absolute_journal_path(root: &Path, journal: &Path) -> PathBuf {
    if journal.is_absolute() {
        journal.to_path_buf()
    } else {
        root.join(journal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn existing_watch_syntax_is_preserved() {
        let args = Args::try_parse_from(["diffwatch", "project", "--settle", "1.5"]).unwrap();
        assert!(args.command.is_none());
        assert_eq!(args.watch.root, PathBuf::from("project"));
        assert_eq!(args.watch.settle, 1.5);
    }

    #[test]
    fn parses_history_subcommand() {
        let args = Args::try_parse_from(["diffwatch", "history", "project"]).unwrap();
        assert!(matches!(args.command, Some(Command::History(_))));
    }
}
