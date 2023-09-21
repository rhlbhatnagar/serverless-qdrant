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

pub fn setup(config: &LoggerConfig) -> anyhow::Result<LoggerHandle> {
    tracing_log::LogTracer::init()?;

    let default_logger = Some(fmt::Layer::default()).with_filter(filter::EnvFilter::default());
    let (default_logger, default_logger_handle) = reload::Layer::new(default_logger);
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

    LoggerHandle::new(config.clone(), default_logger_handle)
}

#[derive(Clone)]
pub struct LoggerHandle {
    config: Arc<RwLock<LoggerConfig>>,
    default: DefaultLoggerReloadHandle,
}

type DefaultLoggerReloadHandle = reload::Handle<
    filter::Filtered<Option<fmt::Layer<Registry>>, filter::EnvFilter, Registry>,
    Registry,
>;

impl LoggerHandle {
    pub fn new(config: LoggerConfig, default: DefaultLoggerReloadHandle) -> anyhow::Result<Self> {
        default.modify(|logger| update_default_logger(logger, &config.default))?;

        let handle = Self {
            config: Arc::new(RwLock::new(config)),
            default,
        };

        Ok(handle)
    }

    pub async fn get_config(&self) -> LoggerConfig {
        self.config.read().await.clone()
    }

    pub async fn update_config(&self, diff: LoggerConfigDiff) -> anyhow::Result<()> {
        self.default
            .modify(|logger| update_default_logger(logger, &diff.default))?;

        self.config.write().await.update(diff);

        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
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

    pub fn update(&mut self, diff: LoggerConfigDiff) {
        self.default.update(diff.default);
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct DefaultLoggerConfig {
    log_level: Option<String>,
    span_events: SpanEvents,
    color: Color,
}

impl DefaultLoggerConfig {
    pub fn update(&mut self, diff: DefaultLoggerConfigDiff) {
        if let Some(log_level) = diff.log_level {
            self.log_level = Some(log_level);
        }

        if let Some(span_events) = diff.span_events {
            self.span_events = span_events;
        }

        if let Some(color) = diff.color {
            self.color = color;
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, SmartDefault)]
#[serde(from = "helpers::SpanEvents", into = "helpers::SpanEvents")]
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

#[derive(Copy, Clone, Debug, Deserialize, Serialize, SmartDefault)]
#[serde(from = "helpers::Color", into = "helpers::Color")]
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

    #[derive(Clone, Debug, Deserialize, Serialize)]
    #[serde(untagged)]
    pub enum SpanEvents {
        Some(Vec<SpanEvent>),
        None(NoneTag),
        Null,
    }

    impl SpanEvents {
        pub fn from_fmt_span(events: fmt::format::FmtSpan) -> Self {
            let events = SpanEvent::from_fmt_span(events);

            if !events.is_empty() {
                Self::Some(events)
            } else {
                Self::None(NoneTag::None)
            }
        }

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

    impl From<super::SpanEvents> for SpanEvents {
        fn from(events: super::SpanEvents) -> Self {
            Self::from_fmt_span(events.into())
        }
    }

    impl From<SpanEvents> for super::SpanEvents {
        fn from(events: SpanEvents) -> Self {
            events.to_fmt_span().into()
        }
    }

    #[derive(Copy, Clone, Debug, Deserialize, Serialize)]
    #[serde(rename_all = "lowercase")]
    pub enum SpanEvent {
        New,
        Enter,
        Exit,
        Close,
    }

    impl SpanEvent {
        pub fn from_fmt_span(events: fmt::format::FmtSpan) -> Vec<Self> {
            const EVENTS: &[SpanEvent] = &[
                SpanEvent::New,
                SpanEvent::Enter,
                SpanEvent::Exit,
                SpanEvent::Close,
            ];

            EVENTS
                .iter()
                .copied()
                .filter(|event| events.clone() & event.to_fmt_span() == event.to_fmt_span())
                .collect()
        }

        pub fn to_fmt_span(self) -> fmt::format::FmtSpan {
            match self {
                SpanEvent::New => fmt::format::FmtSpan::NEW,
                SpanEvent::Enter => fmt::format::FmtSpan::ENTER,
                SpanEvent::Exit => fmt::format::FmtSpan::EXIT,
                SpanEvent::Close => fmt::format::FmtSpan::CLOSE,
            }
        }
    }

    #[derive(Copy, Clone, Debug, Deserialize, Serialize)]
    #[serde(rename_all = "lowercase")]
    pub enum NoneTag {
        None,
    }

    #[derive(Copy, Clone, Debug, Deserialize, Serialize)]
    #[serde(untagged)]
    pub enum Color {
        Auto(AutoTag),
        Bool(bool),
    }

    impl From<super::Color> for Color {
        fn from(color: super::Color) -> Self {
            match color {
                super::Color::Auto => Self::Auto(AutoTag::Auto),
                super::Color::Enable => Self::Bool(true),
                super::Color::Disable => Self::Bool(false),
            }
        }
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

#[derive(Clone, Debug, Default, Deserialize)]
pub struct LoggerConfigDiff {
    #[serde(flatten)]
    default: DefaultLoggerConfigDiff,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct DefaultLoggerConfigDiff {
    log_level: Option<String>,
    span_events: Option<SpanEvents>,
    color: Option<Color>,
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

#[rustfmt::skip] // `rustfmt` formats this into unreadable single line :/
type Logger<S, N, F, T, W, SS> = filter::Filtered<
    Option<fmt::Layer<S, N, fmt::format::Format<F, T>, W>>,
    filter::EnvFilter,
    SS,
>;

fn update_default_logger<S, N, F, T, W, SS>(
    logger: &mut Logger<S, N, F, T, W, SS>,
    config: &impl AsLoggerConfigDiff,
) where
    N: for<'writer> fmt::FormatFields<'writer> + 'static,
{
    if let Some(user_filters) = config.log_level() {
        *logger.filter_mut() = filter(user_filters);
    }

    if let Some(span_events) = config.span_events() {
        let mut layer = logger.inner_mut().take().expect("valid logger state");
        layer = layer.with_span_events(span_events.into());
        *logger.inner_mut() = Some(layer);
    }

    if let Some(color) = config.color() {
        logger
            .inner_mut()
            .as_mut()
            .expect("valid logger state")
            .set_ansi(color.to_bool());
    }
}

/// Helper trait to abstract different `*LoggerConfig` and `*LoggerConfigDiff` types
trait AsLoggerConfigDiff {
    fn log_level(&self) -> Option<&str>;
    fn span_events(&self) -> Option<SpanEvents>;

    fn color(&self) -> Option<Color> {
        None
    }
}

impl AsLoggerConfigDiff for DefaultLoggerConfig {
    fn log_level(&self) -> Option<&str> {
        self.log_level.as_deref()
    }

    fn span_events(&self) -> Option<SpanEvents> {
        Some(self.span_events.clone())
    }

    fn color(&self) -> Option<Color> {
        Some(self.color)
    }
}

impl AsLoggerConfigDiff for DefaultLoggerConfigDiff {
    fn log_level(&self) -> Option<&str> {
        self.log_level.as_deref()
    }

    fn span_events(&self) -> Option<SpanEvents> {
        self.span_events.clone()
    }

    fn color(&self) -> Option<Color> {
        self.color
    }
}
