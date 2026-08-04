/*
 * @file /parastation-web/src/web_file.rs
 * @brief
 * DiscSource implementation for the web frontend. Runs inside a Worker (required, since FileReaderSync is only
 * available there), reading directly from a browser File handle
 *
 * -----
 */

use parastation_core::DiscSource;
use web_sys::{File, FileReaderSync};

pub struct WebFile {
    file: File,
    reader: FileReaderSync,
}

impl WebFile {
    pub fn new(file: File) -> Self {
        // FileReaderSync is only available in a Worker (which is why this entire commit moves the core to a worker)
        let reader = FileReaderSync::new()
            .expect("Failed to construct FileReaderSync, must be called from within a Worker");
        Self { file, reader }
    }
}

impl DiscSource for WebFile {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> usize {
        let blob = self
            .file
            .slice_with_f64_and_f64(offset as f64, (offset + buf.len() as u64) as f64)
            .expect("File.slice failed");

        let array_buffer = match self.reader.read_as_array_buffer(&blob) {
            Ok(ab) => ab,
            Err(_) => return 0,
        };

        let bytes = js_sys::Uint8Array::new(&array_buffer);
        let len = (bytes.length() as usize).min(buf.len());
        bytes.slice(0, len as u32).copy_to(&mut buf[..len]);
        len
    }

    fn len(&self) -> u64 {
        self.file.size() as u64
    }
}
