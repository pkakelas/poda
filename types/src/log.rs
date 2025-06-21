use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
pub use tracing::{info, error, warn, debug, trace};

pub fn init_logging() {
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| {
                    // Show only workspace crates, hide external deps
                    "dispencer=info,dispenser=info,storage_provider=info,challenger=info,kzg=info,merkle_tree=info,pod=info,types=info".into()
                })
        )
        .with(
            fmt::Layer::new()
                .with_timer(())
                .with_thread_ids(false)
                .with_thread_names(false)
                .with_file(false)
                .with_line_number(false)
                .with_target(true)
        )
        .init();
}