/*
 * @file /parastation-web/src/web_logger.rs
 * @brief
 * Logger trait implementation for the web frontend, routing log messages to the browser console and error logs to the
 * browser console error log. Also implements tty_putchar() by routing characters to a browser text area.
 *
 * -----
 */

use parastation_core::logging::Logger;
use std::cell::RefCell;

pub struct WebLogger {
    tty_line_buffer: RefCell<String>,
}

impl WebLogger {
    pub fn new() -> Self {
        Self {
            tty_line_buffer: RefCell::new(String::new()),
        }
    }
}

impl Logger for WebLogger {
    fn log(&self, message: &str) {
        web_sys::console::log_1(&message.into());
    }
    fn elog(&self, message: &str) {
        web_sys::console::error_1(&message.into());
    }
    fn tty_putchar(&self, ch: char) {
        let mut buf = self.tty_line_buffer.borrow_mut();
        buf.push(ch);
        if ch == '\n' {
            append_tty_line(&buf);
            buf.clear();
        }
    }
}

/// Appends a line to the #tty-output element in the page
fn append_tty_line(line: &str) {
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    if let Some(el) = document.get_element_by_id("tty-output") {
        let current = el.text_content().unwrap_or_default();
        el.set_text_content(Some(&format!("{current}{line}")));
    }
}
