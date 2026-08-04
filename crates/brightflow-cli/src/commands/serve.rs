//! `serve` / `run-all`: build the API ServeConfig from flags and env.

use brightflow_api::ServeConfig;

pub(crate) fn build_serve_config(
    host: Option<&str>,
    port: Option<u16>,
    dataset: Option<String>,
    store: Option<&str>,
    tables: Option<String>,
    connector_configs: Option<&str>,
    database_url: Option<&str>,
) -> ServeConfig {
    let mut config = ServeConfig::from_env();
    // CLI flags become explicit path overrides — value-passing, because
    // set_var inside the running tokio runtime is a thread-safety hazard.
    let mut paths = brightflow_core::WorkspacePaths::from_env();
    if let Some(store) = store {
        paths = paths.with_store(store);
    }
    if let Some(configs) = connector_configs {
        paths = paths.with_connector_configs(configs);
    }
    if let Some(url) = database_url {
        paths = paths.with_auth_url(url);
    }
    config.paths = paths;

    if let Some(host) = host {
        config.host = host.parse().unwrap_or(std::net::Ipv4Addr::LOCALHOST);
    }
    if let Some(port) = port {
        config.port = port;
    }
    if dataset.is_some() {
        config.default_dataset = dataset;
    }
    if let Some(tables) = tables {
        config.tables = Some(tables.split(',').map(|t| t.trim().to_string()).collect());
    }

    config
}
