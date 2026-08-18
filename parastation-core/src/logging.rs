/*
 * @file /parastation-core/src/logging.rs
 * @brief
 * Platform-agnostic logging and TTY output hooks. Replaces println! and eprintln! with log! and elog!, which are
 * implemented by the frontend.
 *
 * -----
 */

use std::cell::RefCell;

pub trait Logger {
    fn log(&self, message: &str);
    fn elog(&self, message: &str);
    fn tty_putchar(&self, ch: char);
}

thread_local! {
    static LOGGER: RefCell<Option<Box<dyn Logger>>> = RefCell::new(None);
}

/// Register the logger implementation for this thread. Must be called once at startup by the frontend before any
/// log!()/elog!()/tty_putchar() calls
pub fn set_logger(logger: Box<dyn Logger>) {
    LOGGER.with(|l| *l.borrow_mut() = Some(logger));
}

#[doc(hidden)]
pub fn __log(message: &str) {
    LOGGER.with(|l| {
        if let Some(logger) = l.borrow().as_ref() {
            logger.log(message);
        }
    });
}

#[doc(hidden)]
pub fn __elog(message: &str) {
    LOGGER.with(|l| {
        if let Some(logger) = l.borrow().as_ref() {
            logger.elog(message);
        }
    });
}

pub fn tty_putchar(ch: char) {
    LOGGER.with(|l| {
        if let Some(logger) = l.borrow().as_ref() {
            logger.tty_putchar(ch);
        }
    });
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
        $crate::logging::__log(&format!($($arg)*))
    };
}

#[macro_export]
macro_rules! elog {
    ($($arg:tt)*) => {
        $crate::logging::__elog(&format!($($arg)*))
    };
}
