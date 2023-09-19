#![allow(dead_code, unused_imports)] // `schema_generator` and `#[cfg(...)]` attributes produce warnings :/

use std::fmt::Write as _;
use std::io::{self, IsTerminal as _};
use std::str::FromStr as _;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;
use tokio::sync::RwLock;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{filter, fmt, reload, Registry};

pub fn setup(config: &LoggerConfig) -> anyhow::Result<()> {
    tracing_log::LogTracer::init()?;

    let default_logger = fmt::Layer::new()
        .with_ansi(config.default.color.to_bool())
        .with_span_events(config.default.span_events.clone().into())
        .with_filter(filter(config.default.log_level.as_deref().unwrap_or("")));

    let reg = tracing_subscriber::registry().with(default_logger);

    // Use `console` or `console-subscriber` feature to enable `console-subscriber`
    //
    // Note, that `console-subscriber` requires manually enabling
    // `--cfg tokio_unstable` rust flags during compilation!
    //
    // Otherwise `console_subscriber::spawn` call panics!
    //
    // See https://docs.rs/tokio/latest/tokio/#unstable-features
    #[cfg(all(feature = "console-subscriber", tokio_unstable))]
    let reg = reg.with(console_subscriber::spawn());

    #[cfg(all(feature = "console-subscriber", not(tokio_unstable)))]
    eprintln!(
        "`console-subscriber` requires manually enabling \
         `--cfg tokio_unstable` rust flags during compilation!"
    );

    // Use `tracy` or `tracing-tracy` feature to enable `tracing-tracy`
    #[cfg(feature = "tracing-tracy")]
    let reg = reg.with(tracing_tracy::TracyLayer::new().with_filter(
        tracing_subscriber::filter::filter_fn(|metadata| metadata.is_span()),
    ));

    tracing::subscriber::set_global_default(reg)?;

    Ok(())
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct LoggerConfig {
    #[serde(flatten)]
    default: DefaultLoggerConfig,
}

impl LoggerConfig {
    pub fn with_top_level_directive(&mut self, log_level: Option<String>) -> &mut Self {
        if self.default.log_level.is_some() && log_level.is_some() {
            // TODO: Warn if both top-level `log_level` and `LoggerConfig::log_level` directives are used
            eprintln!("TODO");
        }

        self.default.log_level = self.default.log_level.take().or(log_level);
        self
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct DefaultLoggerConfig {
    log_level: Option<String>,
    span_events: SpanEvents,
    color: Color,
}

#[derive(Clone, Debug, Deserialize, SmartDefault)]
#[serde(from = "helpers::SpanEvents")]
pub struct SpanEvents {
    #[default(fmt::format::FmtSpan::NONE)]
    events: fmt::format::FmtSpan,
}

impl From<fmt::format::FmtSpan> for SpanEvents {
    fn from(events: fmt::format::FmtSpan) -> Self {
        Self { events }
    }
}

impl From<SpanEvents> for fmt::format::FmtSpan {
    fn from(events: SpanEvents) -> Self {
        events.events
    }
}

#[derive(Copy, Clone, Debug, Deserialize, SmartDefault)]
#[serde(from = "helpers::Color")]
pub enum Color {
    #[default]
    Auto,
    Enable,
    Disable,
}

impl Color {
    pub fn to_bool(self) -> bool {
        match self {
            Self::Auto => io::stdout().is_terminal(),
            Self::Enable => true,
            Self::Disable => false,
        }
    }
}

mod helpers {
    use super::*;

    #[derive(Clone, Debug, Deserialize)]
    #[serde(untagged)]
    pub enum SpanEvents {
        Some(Vec<SpanEvent>),
        None(NoneTag),
        Null,
    }

    impl SpanEvents {
        pub fn to_fmt_span(&self) -> fmt::format::FmtSpan {
            self.as_slice()
                .iter()
                .copied()
                .fold(fmt::format::FmtSpan::NONE, |events, event| {
                    events | event.to_fmt_span()
                })
        }

        fn as_slice(&self) -> &[SpanEvent] {
            match self {
                SpanEvents::Some(events) => events,
                _ => &[],
            }
        }
    }

    impl From<SpanEvents> for super::SpanEvents {
        fn from(events: SpanEvents) -> Self {
            events.to_fmt_span().into()
        }
    }

    #[derive(Copy, Clone, Debug, Deserialize)]
    #[serde(rename_all = "lowercase")]
    pub enum SpanEvent {
        New,
        Enter,
        Exit,
        Close,
    }

    impl SpanEvent {
        pub fn to_fmt_span(self) -> fmt::format::FmtSpan {
            match self {
                SpanEvent::New => fmt::format::FmtSpan::NEW,
                SpanEvent::Enter => fmt::format::FmtSpan::ENTER,
                SpanEvent::Exit => fmt::format::FmtSpan::EXIT,
                SpanEvent::Close => fmt::format::FmtSpan::CLOSE,
            }
        }
    }

    #[derive(Copy, Clone, Debug, Deserialize)]
    #[serde(rename_all = "lowercase")]
    pub enum NoneTag {
        None,
    }

    #[derive(Copy, Clone, Debug, Deserialize)]
    #[serde(untagged)]
    pub enum Color {
        Auto(AutoTag),
        Bool(bool),
    }

    impl From<Color> for super::Color {
        fn from(color: Color) -> Self {
            match color {
                Color::Auto(_) => Self::Auto,
                Color::Bool(true) => Self::Enable,
                Color::Bool(false) => Self::Disable,
            }
        }
    }

    #[derive(Copy, Clone, Debug, Deserialize, Serialize)]
    #[serde(rename_all = "lowercase")]
    pub enum AutoTag {
        Auto,
    }
}

const DEFAULT_LOG_LEVEL: log::LevelFilter = log::LevelFilter::Info;

const DEFAULT_FILTERS: &[(&str, log::LevelFilter)] = &[
    ("hyper", log::LevelFilter::Info),
    ("h2", log::LevelFilter::Error),
    ("tower", log::LevelFilter::Warn),
    ("rustls", log::LevelFilter::Info),
    ("wal", log::LevelFilter::Warn),
    ("raft", log::LevelFilter::Warn),
];

fn filter(user_filters: &str) -> filter::EnvFilter {
    let mut filter = String::new();

    let user_log_level = user_filters
        .rsplit(',')
        .find_map(|dir| log::LevelFilter::from_str(dir).ok());

    if user_log_level.is_none() {
        write!(&mut filter, "{DEFAULT_LOG_LEVEL}").unwrap(); // Writing into `String` never fails
    }

    for (target, log_level) in DEFAULT_FILTERS.iter().copied() {
        if user_log_level.unwrap_or(DEFAULT_LOG_LEVEL) > log_level {
            let comma = if filter.is_empty() { "" } else { "," };
            write!(&mut filter, "{comma}{target}={log_level}").unwrap(); // Writing into `String` never fails
        }
    }

    let comma = if filter.is_empty() { "" } else { "," };
    write!(&mut filter, "{comma}{user_filters}").unwrap(); // Writing into `String` never fails

    filter::EnvFilter::builder()
        .with_regex(false)
        .parse_lossy(filter)
}
