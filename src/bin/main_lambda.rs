#![allow(deprecated)]

use std::sync::Arc;

use ::tonic::transport::Uri;
use clap::Parser;
use collection::shards::channel_service::ChannelService;
use qdrant::common::helpers::{
    create_general_purpose_runtime, create_search_runtime, create_update_runtime,
};
use qdrant::common::telemetry::TelemetryCollector;
use qdrant::common::telemetry_reporting::TelemetryReporter;
use qdrant::greeting::welcome;
use qdrant::settings::Settings;
use qdrant::startup::{
    remove_started_file_indicator, setup_panic_hook, touch_started_file_indicator,
};
use storage::content_manager::consensus::persistent::Persistent;
use storage::content_manager::toc::TableOfContent;
use storage::dispatcher::Dispatcher;
#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

/// Qdrant (read: quadrant ) is a vector similarity search engine.
/// It provides a production-ready service with a convenient API to store, search, and manage points - vectors with an additional payload.
///
/// This CLI starts a Qdrant peer/server.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Uri of the peer to bootstrap from in case of multi-peer deployment.
    /// If not specified - this peer will be considered as a first in a new deployment.
    #[arg(long, value_parser, value_name = "URI")]
    bootstrap: Option<Uri>,
    /// Uri of this peer.
    /// Other peers should be able to reach it by this uri.
    ///
    /// This value has to be supplied if this is the first peer in a new deployment.
    ///
    /// In case this is not the first peer and it bootstraps the value is optional.
    /// If not supplied then qdrant will take internal grpc port from config and derive the IP address of this peer on bootstrap peer (receiving side)
    #[arg(long, value_parser, value_name = "URI")]
    uri: Option<Uri>,

    /// Force snapshot re-creation
    /// If provided - existing collections will be replaced with snapshots.
    /// Default is to not recreate from snapshots.
    #[arg(short, long, action, default_value_t = false)]
    force_snapshot: bool,

    /// List of paths to snapshot files.
    /// Format: <snapshot_file_path>:<target_collection_name>
    ///
    /// WARN: Do not use this option if you are recovering collection in existing distributed cluster.
    /// Use `/collections/<collection-name>/snapshots/recover` API instead.
    #[arg(long, value_name = "PATH:NAME", alias = "collection-snapshot")]
    snapshot: Option<Vec<String>>,

    /// Path to snapshot of multiple collections.
    /// Format: <snapshot_file_path>
    ///
    /// WARN: Do not use this option if you are recovering collection in existing distributed cluster.
    /// Use `/collections/<collection-name>/snapshots/recover` API instead.
    #[arg(long, value_name = "PATH")]
    storage_snapshot: Option<String>,

    /// Path to an alternative configuration file.
    /// Format: <config_file_path>
    ///
    /// Default path : config/config.yaml
    #[arg(long, value_name = "PATH")]
    config_path: Option<String>,

    /// Disable telemetry sending to developers
    /// If provided - telemetry collection will be disabled.
    /// Read more: <https://qdrant.tech/documentation/guides/telemetry>
    #[arg(long, action, default_value_t = false)]
    disable_telemetry: bool,

    /// Run stacktrace collector. Used for debugging.
    #[arg(long, action, default_value_t = false)]
    stacktrace: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Run backtrace collector, expected to used by `rstack` crate
    if args.stacktrace {
        #[cfg(all(target_os = "linux", feature = "stacktrace"))]
        {
            let _ = rstack_self::child();
        }
        return Ok(());
    }

    remove_started_file_indicator();

    let settings = Settings::new(args.config_path)?;

    let reporting_enabled = !settings.telemetry_disabled && !args.disable_telemetry;

    let reporting_id = TelemetryCollector::generate_id();

    qdrant::tracing::setup(&settings.log_level)?;

    setup_panic_hook(reporting_enabled, reporting_id.to_string());

    memory::madvise::set_global(settings.storage.mmap_advice);
    segment::vector_storage::common::set_async_scorer(settings.storage.async_scorer);

    welcome(&settings);

    if let Some(recovery_warning) = &settings.storage.recovery_mode {
        log::warn!("Qdrant is loaded in recovery mode: {}", recovery_warning);
        log::warn!(
            "Read more: https://qdrant.tech/documentation/guides/administration/#recovery-mode"
        );
    }

    // Validate as soon as possible, but we must initialize logging first
    settings.validate_and_warn();

    // Saved state of the consensus.
    let persistent_consensus_state =
        Persistent::load_or_init(&settings.storage.storage_path, args.bootstrap.is_none())?;

    // Create and own search runtime out of the scope of async context to ensure correct
    // destruction of it
    let search_runtime = create_search_runtime(settings.storage.performance.max_search_threads)
        .expect("Can't search create runtime.");

    let update_runtime =
        create_update_runtime(settings.storage.performance.max_optimization_threads)
            .expect("Can't optimizer create runtime.");

    let general_runtime =
        create_general_purpose_runtime().expect("Can't optimizer general purpose runtime.");
    let runtime_handle = general_runtime.handle().clone();

    // Table of content manages the list of collections.
    // It is a main entry point for the storage.
    let toc = TableOfContent::new_sync(
        &settings.storage,
        search_runtime,
        update_runtime,
        general_runtime,
        ChannelService::new(settings.service.http_port),
        persistent_consensus_state.this_peer_id(),
        None,
    )
    .await;

    toc.clear_all_tmp_directories()?;

    let toc_arc = Arc::new(toc);

    // Router for external queries.
    // It decides if query should go directly to the ToC or through the consensus.
    let dispatcher = Dispatcher::new(toc_arc.clone());

    let (telemetry_collector, dispatcher_arc) = {
        log::info!("Distributed mode disabled");
        let dispatcher_arc = Arc::new(dispatcher);

        // Monitoring and telemetry.
        let telemetry_collector =
            TelemetryCollector::new(settings.clone(), dispatcher_arc.clone(), reporting_id);
        (telemetry_collector, dispatcher_arc)
    };

    //
    // Telemetry reporting
    //

    let reporting_id = telemetry_collector.reporting_id();
    let telemetry_collector = Arc::new(tokio::sync::Mutex::new(telemetry_collector));

    if reporting_enabled {
        log::info!("Telemetry reporting enabled, id: {}", reporting_id);

        runtime_handle.spawn(TelemetryReporter::run(telemetry_collector.clone()));
    } else {
        log::info!("Telemetry reporting disabled");
    }

    //
    // REST API server, currently standalone mode only supports web
    //

    #[cfg(feature = "web")]
    {
        touch_started_file_indicator();
        let dispatcher_arc = dispatcher_arc.clone();
        let settings = settings.clone();
        let _ =
            qdrant::actix::init_lambda(dispatcher_arc.clone(), telemetry_collector, None, settings)
                .await;
    }

    //
    // gRPC server
    //

    log::info!("gRPC endpoint disabled");

    //
    // service debug
    //

    log::info!("service debug disabled");

    drop(toc_arc);
    drop(settings);
    Ok(())
}
