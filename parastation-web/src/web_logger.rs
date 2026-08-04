/*
 * @file /parastation-web/src/web_logger.rs
 * @brief
 * Logger trait implementation for the web frontend, routing log messages to the browser console and error logs to the
 * browser console error log. Also implements tty_putchar() by routing characters to a browser text area.
 *
 * -----
 */

use js_sys::Object;
use parastation_core::logging::Logger;
use std::cell::RefCell;
use wasm_bindgen::{JsCast, JsValue};

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
            post_tty_line(&buf);
            buf.clear();
        }
    }
}

/// Post a completed tty line to the main thread via postMessage
fn post_tty_line(line: &str) {
    let scope: web_sys::DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();

    let msg = Object::new();
    let _ = js_sys::Reflect::set(&msg, &"type".into(), &"tty".into());
    let _ = js_sys::Reflect::set(&msg, &"payload".into(), &JsValue::from_str(line));

    let _ = scope.post_message(&msg);
}
