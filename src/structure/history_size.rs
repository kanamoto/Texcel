use serde::Serialize;
use std::io::{self, Write};

use crate::i18n::Message;

struct CountingWriter(u64);

impl Write for CountingWriter {
    /// Counts bytes without retaining a serialized history copy.
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self.0.saturating_add(bytes.len() as u64);
        Ok(bytes.len())
    }

    /// Completes the size pass without buffering output.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Returns the serialized size of a history value.
pub(crate) fn serialized_size(value: &impl Serialize) -> Result<u64, Message> {
    let mut writer = CountingWriter(0);
    serde_json::to_writer(&mut writer, value)
        .map_err(|error| Message::new("status-history-size-failed").with_text("detail", error))?;
    Ok(writer.0)
}
