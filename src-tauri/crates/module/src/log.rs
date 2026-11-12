//! A block's `tracing` lines, as inside Prism: in Prism's log (`module <id>: …`), through `host`.
//! Installed once, when the module is first configured; a block logs with the `tracing` crate
//! (`tracing::warn!("…")`), the same as this one's.

use std::fmt::Write;

use tracing::field::{Field, Visit};
use tracing::level_filters::LevelFilter;
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Level, Metadata, Subscriber};

/// Its events to Prism's log, at their level, info and above (Prism's own default: a debug line
/// would cross into Prism for nothing). Spans are not followed: none is enabled.
struct ToHost;

/// An event's message, then its other fields as `name=value`.
#[derive(Default)]
struct Line(String);

impl Visit for Line {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if !self.0.is_empty() {
            self.0.push(' ');
        }
        let _ = match field.name() {
            "message" => write!(self.0, "{value:?}"),
            name => write!(self.0, "{name}={value:?}"),
        };
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            if !self.0.is_empty() {
                self.0.push(' ');
            }
            self.0.push_str(value);
        } else {
            self.record_debug(field, &value);
        }
    }
}

impl Subscriber for ToHost {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.is_event() && *metadata.level() <= Level::INFO
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        Some(LevelFilter::INFO)
    }

    /// Never called: no span is enabled.
    fn new_span(&self, _span: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }

    fn record(&self, _span: &Id, _values: &Record<'_>) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut line = Line::default();
        event.record(&mut line);
        let level = match *event.metadata().level() {
            Level::ERROR => 0,
            Level::WARN => 1,
            _ => 2,
        };
        crate::host::log(level, &line.0);
    }

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}
}

/// Its `tracing` lines to Prism's log, from now on (once: set again, it stays as it is).
pub(crate) fn install() {
    let _ = tracing::subscriber::set_global_default(ToHost);
}
