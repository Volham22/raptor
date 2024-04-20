use tracing_appender::rolling::RollingFileAppender;
use tracing_subscriber::fmt::format::FmtSpan;

use crate::config;

pub fn init_logging(cli_conf: &config::CliConfig) {
    tracing_subscriber::fmt()
        .pretty()
        .with_span_events(FmtSpan::NEW)
        .with_max_level(cli_conf.level)
        .with_thread_names(true)
        .init();
}

pub fn init_logging_file(cli_conf: &config::CliConfig, appender: RollingFileAppender) {
    tracing_subscriber::fmt()
        .compact()
        .with_span_events(FmtSpan::EXIT)
        .with_max_level(cli_conf.level)
        .with_writer(appender)
        .with_thread_names(true)
        .init();
}
