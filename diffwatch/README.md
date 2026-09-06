# diffwatch

`diffwatch` watches a directory and records file changes as a sequence of

unified-diff batches. A batch closes after a configurable period without file
events. It does not use Git and does not require cooperation from the process
editing the files.

Each batch is rendered as a standard multi-file unified diff, with explicit
file boundaries understood by interactive viewers such as μdiff.

## Run

```sh
cargo test
cargo run -p diffwatch --release -- . --settle 2
```

Text files up to 2 MiB receive line-by-line diffs. Larger files and binary
files are tracked using BLAKE3 fingerprints without retaining their contents
in memory. The threshold can be changed with `--max-text-bytes`.

Diffs are printed to the terminal and stored under
`.diffwatch/session-<timestamp>/0001.diff`, `0002.diff`, and so on. Each diff
has a matching JSON metadata file containing its timestamps and changed-file
list. `session.json` describes the watcher run.

Press `Ctrl-C` to stop cleanly. If a batch is active when the signal arrives,
its current filesystem state is recorded before exit.

## Review history

```sh
# List newest sessions first.
cargo run -p diffwatch -- history .

# Print batch 3 from the latest session.
cargo run -p diffwatch -- show 3 .

# Print it from a specific session.
cargo run -p diffwatch -- show 3 . --session session-20260826T203012.123Z
```

The initial implementation takes a complete snapshot after each quiet period.
This favors correctness and a simple debugging model. Incremental rescanning
can be added later without changing the snapshot/batch API.

`.gitignore` is respected when present only as a convenient source of ignore
rules. Git itself is never invoked and the watched directory need not be a Git
repository. Ignore rules are applied both to snapshots and to filesystem
events, so build output does not keep an activity batch open. Restart
`diffwatch` after editing `.gitignore` to load the new rules.

The `.git/` metadata directory and the Diffwatch journal are always excluded,
even when they are not listed in `.gitignore`.
