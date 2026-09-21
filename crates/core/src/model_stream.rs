use anyhow::{ensure, Result};
use serde_json::Value;

pub enum Item {
    Data(Value),
    Done,
}

/// Incremental UTF-8 framing shared by model adapters. Network chunks need not
/// align with characters, lines, or server-sent events.
pub struct Decoder {
    ndjson: bool,
    pending: Vec<u8>,
    data: Vec<String>,
    event_size: usize,
    total: usize,
}
impl Decoder {
    pub fn new(ndjson: bool) -> Self {
        Self {
            ndjson,
            pending: Vec::new(),
            data: Vec::new(),
            event_size: 0,
            total: 0,
        }
    }
    fn dispatch(&mut self, out: &mut Vec<Item>) -> Result<()> {
        if self.data.is_empty() {
            return Ok(());
        }
        let data = self.data.join("\n");
        self.data.clear();
        self.event_size = 0;
        if data.trim() == "[DONE]" {
            out.push(Item::Done);
        } else {
            out.push(Item::Data(serde_json::from_str(&data)?));
        }
        Ok(())
    }
    fn line(&mut self, bytes: &[u8], out: &mut Vec<Item>) -> Result<()> {
        let line = std::str::from_utf8(bytes)?.trim_end_matches(['\r', '\n']);
        if self.ndjson {
            if !line.trim().is_empty() {
                out.push(Item::Data(serde_json::from_str(line)?));
            }
        } else if line.is_empty() {
            self.dispatch(out)?;
        } else if let Some(data) = line.strip_prefix("data:") {
            let data = data.strip_prefix(' ').unwrap_or(data);
            self.event_size += data.len();
            ensure!(
                self.event_size <= 4_000_000,
                "Evento do modelo excede limite"
            );
            self.data.push(data.into());
        }
        Ok(())
    }
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Item>> {
        self.total += bytes.len();
        ensure!(
            self.total <= 32_000_000,
            "Streaming do modelo excede limite"
        );
        self.pending.extend_from_slice(bytes);
        let mut out = Vec::new();
        while let Some(i) = self.pending.iter().position(|b| *b == b'\n') {
            let line: Vec<_> = self.pending.drain(..=i).collect();
            self.line(&line, &mut out)?;
        }
        ensure!(
            self.pending.len() <= 4_000_000,
            "Linha do modelo excede limite"
        );
        Ok(out)
    }
    pub fn finish(&mut self) -> Result<Vec<Item>> {
        let mut out = Vec::new();
        let remaining = std::mem::take(&mut self.pending);
        if !remaining.is_empty() {
            self.line(&remaining, &mut out)?;
        }
        self.dispatch(&mut out)?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf8_and_multiline_events_survive_arbitrary_network_chunks() {
        let bytes =
            "event: delta\r\ndata: {\"text\":\r\ndata: \"ação🔥\"}\r\n\r\ndata: [DONE]".as_bytes();
        let mut d = Decoder::new(false);
        let mut items = Vec::new();
        for byte in bytes {
            items.extend(d.push(&[*byte]).unwrap());
        }
        items.extend(d.finish().unwrap());
        assert!(matches!(&items[0], Item::Data(v) if v["text"] == "ação🔥"));
        assert!(matches!(&items[1], Item::Done));
        assert_eq!(items.len(), 2);
    }
    #[test]
    fn truncated_json_is_never_dispatched() {
        let mut d = Decoder::new(true);
        d.push(b"{\"done\":tr").unwrap();
        assert!(d.finish().is_err());
    }
}
