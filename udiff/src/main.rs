mod app;
mod comment;
mod highlight;
mod input;
mod model;
mod terminal;
mod theme;

use anyhow::Result;
use app::App;
#[cfg(feature = "watch")]
use input::WatchSource;
use input::{DiffSource, StdinDiffSource};
#[cfg(feature = "watch")]
use std::path::PathBuf;
use terminal::TerminalRuntime;

#[derive(Clone, Debug, Eq, PartialEq)]
enum InputMode {
    Static,
    #[cfg(feature = "watch")]
    Watch(PathBuf),
}

impl InputMode {
    fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Self> {
        let arguments = arguments.into_iter().collect::<Vec<_>>();
        match arguments.as_slice() {
            [] => Ok(Self::Static),
            #[cfg(feature = "watch")]
            [flag, root] if flag == "--watch" => Ok(Self::Watch(root.into())),
            #[cfg(feature = "watch")]
            [flag] if flag == "--watch" => anyhow::bail!("--watch requires a directory"),
            #[cfg(not(feature = "watch"))]
            [flag, ..] if flag == "--watch" => {
                anyhow::bail!("--watch is unavailable in this build (enable the `watch` feature)")
            }
            [argument, ..] => anyhow::bail!("unexpected argument: {argument}"),
        }
    }
}

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("udiff: {error:#}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<i32> {
    theme::init();
    match InputMode::parse(std::env::args().skip(1))? {
        #[cfg(feature = "watch")]
        InputMode::Watch(root) => view_watch(root),
        InputMode::Static => view_stdin(),
    }
}

fn view_stdin() -> Result<i32> {
    let raw = StdinDiffSource.read()?;
    view_diff(&raw)
}

#[cfg(feature = "watch")]
fn view_watch(root: PathBuf) -> Result<i32> {
    let context = StdinDiffSource.read_optional()?;
    let initial = context
        .filter(|raw| !raw.trim().is_empty())
        .map(|raw| {
            let files = model::parse_unified_diff(&raw);
            anyhow::ensure!(
                !files.is_empty(),
                "piped context contains no supported diff"
            );
            Ok(App::new_watching_context(files))
        })
        .transpose()?;
    let source = WatchSource::start(root)?;
    TerminalRuntime::run_watching(source, initial)?;
    Ok(0)
}

fn view_diff(raw: &str) -> Result<i32> {
    let files = model::parse_unified_diff(raw);
    if files.is_empty() {
        eprintln!("udiff: nothing to view");
        return Ok(0);
    }

    let app = App::new(files, Vec::new());
    TerminalRuntime::run(app)?;
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_mode_accepts_only_the_documented_invocations() {
        assert_eq!(InputMode::parse(Vec::new()).unwrap(), InputMode::Static);
        assert!(InputMode::parse(["--unknown".to_owned()]).is_err());
    }

    #[cfg(feature = "watch")]
    #[test]
    fn input_mode_accepts_watch_when_enabled() {
        assert_eq!(
            InputMode::parse(["--watch".to_owned(), "project".to_owned()]).unwrap(),
            InputMode::Watch(PathBuf::from("project"))
        );
        assert!(InputMode::parse(["--watch".to_owned()]).is_err());
    }

    #[cfg(not(feature = "watch"))]
    #[test]
    fn input_mode_explains_when_watch_is_disabled() {
        let error = InputMode::parse(["--watch".to_owned(), "project".to_owned()]).unwrap_err();
        assert_eq!(
            error.to_string(),
            "--watch is unavailable in this build (enable the `watch` feature)"
        );
    }
}
