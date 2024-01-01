use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result as AResult};
use config::{Config, Environment, FileFormat};
use dotenv::dotenv;
use serde::{Deserialize, Serialize};
use tracing::info;
use crate::errors::RaftResult;


#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub tick_interval_ms: u64,
    pub heartbeat_ticks: u64,
    pub election_min_timeout_ticks: u64,
    pub election_max_timeout_ticks: u64,
    pub cluster: Vec<NodeConfig>,

}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NodeConfig {
    pub node_id: String,
    pub grpc_address: String,
    pub web_address: String,
    pub peers: Vec<String>,
}

impl AppConfig {
    pub fn get_configuration(config_file: &PathBuf) -> RaftResult<AppConfig> {
        dotenv().ok(); //Load .env file. For Prod, create a function and load the injected secrets as environment variables
        let config = Config::builder()
            //Going wild here since we know that the path exists, since the presence of the file is validated by Clap already.
            .add_source(config::File::new(&fs::canonicalize(config_file).unwrap().display().to_string(), FileFormat::Yaml))
            .add_source(
                Environment::with_prefix("RAFTING")
                    .try_parsing(true)
                    .prefix_separator("__")
                    .separator("_"),
            )
            .build().context("Unable to build configuration")?;

        let app_cfg: AppConfig = config.try_deserialize().context("Unable to deserialize configuration")?;
        info!("Loaded configuration: {:?}", app_cfg);

        Ok(app_cfg)
    }

    pub fn peers(&self, node_config: &NodeConfig) -> HashMap<String, String> {
        let peers = node_config.peers.iter().map(|p| {
            let peer_config = self
                .cluster
                .iter()
                .find(|&n| n.node_id == p.to_string())
                .expect(format!("Peer config must be present for peer: {}", p).as_str());
            (peer_config.node_id.to_string(), format!("http://{}", peer_config.grpc_address.to_string()))
        }).collect::<HashMap<String, String>>();

        peers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_and_dotenv_load() {
        let app_cfg = AppConfig::get_configuration(&PathBuf::from("tests/test_cluster_config.yaml")).unwrap();
        assert_eq!(app_cfg.tick_interval_ms, 2000);
        assert_eq!(app_cfg.heartbeat_ticks, 2);
        assert_eq!(app_cfg.election_min_timeout_ticks, 7);
        assert_eq!(app_cfg.election_max_timeout_ticks, 10);
        assert_eq!(app_cfg.cluster.len(), 2);
        assert_eq!(app_cfg.cluster[0].node_id, "node1");
        assert_eq!(app_cfg.cluster[0].grpc_address, "127.0.0.1:7070");
        assert_eq!(app_cfg.cluster[0].web_address, "127.0.0.1:7071");
        assert_eq!(app_cfg.cluster[0].peers.len(), 1);
        assert_eq!(app_cfg.cluster[0].peers[0], "node2");

        assert_eq!(app_cfg.cluster[1].node_id, "node2");
        assert_eq!(app_cfg.cluster[1].grpc_address, "127.0.0.1:8080");
        assert_eq!(app_cfg.cluster[1].web_address, "127.0.0.1:8081");
        assert_eq!(app_cfg.cluster[1].peers.len(), 1);
        assert_eq!(app_cfg.cluster[1].peers[0], "node1");
    }
}
