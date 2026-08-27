use crate::batch::Batch;
use crate::config::{DEFAULT_MAX_TEXT_BYTES, DEFAULT_SETTLE_SECONDS};
use crate::path_filter::PathFilter;
use crate::snapshot::{CaptureOptions, Snapshot};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct WatchOptions {
    pub root: PathBuf,
    pub settle: Duration,
    pub max_text_bytes: u64,
}

impl WatchOptions {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            settle: Duration::from_secs_f64(DEFAULT_SETTLE_SECONDS),
            max_text_bytes: DEFAULT_MAX_TEXT_BYTES,
        }
    }
}

#[derive(Debug)]
pub enum WatchEvent {
    Batch {
        number: u64,
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
        unified_diff: String,
    },
    Warning(String),
    Stopped,
}

/// Owns a background watcher. Dropping the handle stops its worker thread.
pub struct WatchHandle {
    events: mpsc::Receiver<WatchEvent>,
    stopping: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl WatchHandle {
    pub fn start(options: WatchOptions) -> Result<Self> {
        anyhow::ensure!(
            !options.settle.is_zero(),
            "settle duration must be positive"
        );
        let root = options
            .root
            .canonicalize()
            .with_context(|| format!("cannot open {}", options.root.display()))?;
        let excluded_journal = root.join(".diffwatch");
        let baseline = Snapshot::capture_with_options(
            &root,
            &excluded_journal,
            CaptureOptions {
                max_text_bytes: options.max_text_bytes,
            },
        )?;
        let path_filter = PathFilter::new(&root, &excluded_journal)?;
        let (raw_sender, raw_events) = mpsc::channel();
        let mut watcher: RecommendedWatcher =
            notify::recommended_watcher(move |event: notify::Result<Event>| {
                let _ = raw_sender.send(event);
            })?;
        watcher.watch(&root, RecursiveMode::Recursive)?;

        let (event_sender, events) = mpsc::channel();
        let stopping = Arc::new(AtomicBool::new(false));
        let worker_stopping = Arc::clone(&stopping);
        let worker = thread::spawn(move || {
            Worker {
                _watcher: watcher,
                raw_events,
                output: event_sender,
                worker_stopping,
                root,
                path_filter,
                baseline,
                options,
            }
            .run();
        });
        Ok(Self {
            events,
            stopping,
            worker: Some(worker),
        })
    }

    pub fn try_recv(&self) -> std::result::Result<WatchEvent, TryRecvError> {
        self.events.try_recv()
    }

    pub fn recv_timeout(
        &self,
        timeout: Duration,
    ) -> std::result::Result<WatchEvent, RecvTimeoutError> {
        self.events.recv_timeout(timeout)
    }
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct Worker {
    _watcher: RecommendedWatcher,
    raw_events: mpsc::Receiver<notify::Result<Event>>,
    output: mpsc::Sender<WatchEvent>,
    worker_stopping: Arc<AtomicBool>,
    root: PathBuf,
    path_filter: PathFilter,
    baseline: Snapshot,
    options: WatchOptions,
}

impl Worker {
    fn run(mut self) {
        let capture_options = CaptureOptions {
            max_text_bytes: self.options.max_text_bytes,
        };
        let mut batch_number = 1;
        while !self.worker_stopping.load(Ordering::SeqCst) {
            let event = match self.raw_events.recv_timeout(Duration::from_millis(100)) {
                Ok(Ok(event)) => event,
                Ok(Err(error)) => {
                    if self
                        .output
                        .send(WatchEvent::Warning(error.to_string()))
                        .is_err()
                    {
                        return;
                    }
                    continue;
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            };
            if !self.path_filter.includes_any(&event.paths) {
                continue;
            }

            let started_at = Utc::now();
            wait_until_quiet(
                &self.raw_events,
                &self.output,
                &self.worker_stopping,
                &self.path_filter,
                self.options.settle,
            );
            let current = match Snapshot::capture_with_options(
                &self.root,
                &self.root.join(".diffwatch"),
                capture_options,
            ) {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    if self
                        .output
                        .send(WatchEvent::Warning(format!("snapshot: {error:#}")))
                        .is_err()
                    {
                        return;
                    }
                    continue;
                }
            };
            let batch = Batch::between(batch_number, &self.baseline, &current);
            self.baseline = current;
            if batch.is_empty() {
                continue;
            }
            let event = WatchEvent::Batch {
                number: batch_number,
                started_at,
                finished_at: Utc::now(),
                unified_diff: batch.render_unified_diff(),
            };
            if self.output.send(event).is_err() {
                return;
            }
            batch_number += 1;
        }
        let _ = self.output.send(WatchEvent::Stopped);
    }
}

fn wait_until_quiet(
    events: &mpsc::Receiver<notify::Result<Event>>,
    output: &mpsc::Sender<WatchEvent>,
    stopping: &AtomicBool,
    path_filter: &PathFilter,
    settle: Duration,
) {
    let mut deadline = Instant::now() + settle;
    while !stopping.load(Ordering::SeqCst) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match events.recv_timeout(remaining) {
            Ok(Ok(event)) if path_filter.includes_any(&event.paths) => {
                deadline = Instant::now() + settle;
            }
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                let _ = output.send(WatchEvent::Warning(error.to_string()));
            }
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    #[test]
    fn options_have_memory_only_watch_defaults() {
        let options = WatchOptions::new("project");
        assert_eq!(options.root, Path::new("project"));
        assert_eq!(options.settle, Duration::from_secs(2));
        assert_eq!(options.max_text_bytes, 2 * 1024 * 1024);
    }

    #[test]
    fn watches_multiple_files_without_creating_a_journal() {
        let directory = tempdir().unwrap();
        let mut options = WatchOptions::new(directory.path());
        options.settle = Duration::from_millis(50);
        let watcher = WatchHandle::start(options).unwrap();

        fs::write(directory.path().join("first.txt"), "first\n").unwrap();
        fs::write(directory.path().join("second.txt"), "second\n").unwrap();

        let event = watcher.recv_timeout(Duration::from_secs(5)).unwrap();
        let WatchEvent::Batch { unified_diff, .. } = event else {
            panic!("expected a completed batch");
        };
        assert!(unified_diff.contains("b/first.txt"));
        assert!(unified_diff.contains("b/second.txt"));
        assert!(!directory.path().join(".diffwatch").exists());
    }
}
