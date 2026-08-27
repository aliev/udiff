use crate::batch::Batch;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::io::{self, Write};

pub struct BatchStream<W> {
    writer: W,
}

impl<W: Write> BatchStream<W> {
    pub fn start(mut writer: W) -> Result<Self> {
        write_message(
            &mut writer,
            &Message::Hello {
                protocol: "diffwatch",
                version: 1,
            },
        )?;
        Ok(Self { writer })
    }

    pub fn write_batch(
        &mut self,
        batch: &Batch,
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
    ) -> Result<()> {
        write_message(
            &mut self.writer,
            &Message::Batch {
                number: batch.number,
                started_at,
                finished_at,
                diff: batch.render_unified_diff(),
            },
        )
    }
}

impl BatchStream<io::Stdout> {
    pub fn stdout() -> Result<Self> {
        Self::start(io::stdout())
    }
}

fn write_message(writer: &mut impl Write, message: &Message) -> Result<()> {
    serde_json::to_writer(&mut *writer, message).context("cannot encode diffwatch stream")?;
    writer.write_all(b"\n")?;
    writer.flush().context("cannot flush diffwatch stream")
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Message {
    Hello {
        protocol: &'static str,
        version: u8,
    },
    Batch {
        number: u64,
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
        diff: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{FileChange, FileContent};
    use std::path::PathBuf;

    #[test]
    fn writes_one_json_object_per_protocol_message() {
        let now = Utc::now();
        let batch = Batch {
            number: 2,
            changes: vec![FileChange::Added {
                path: PathBuf::from("new.txt"),
                after: FileContent::Text("hello\n".into()),
            }],
        };
        let mut bytes = Vec::new();
        {
            let mut stream = BatchStream::start(&mut bytes).unwrap();
            stream.write_batch(&batch, now, now).unwrap();
        }

        let lines = String::from_utf8(bytes).unwrap();
        let messages = lines
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["type"], "hello");
        assert_eq!(messages[1]["number"], 2);
        assert!(messages[1]["diff"]
            .as_str()
            .unwrap()
            .contains("+++ b/new.txt"));
    }
}
