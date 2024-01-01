use std::error::Error;
use std::net::SocketAddr;
use clap::Parser;
use tokio::sync::{mpsc, oneshot};

use tracing::info;
use tracing_subscriber::util::SubscriberInitExt;
use rafting::args::Args;
use rafting::config::AppConfig;
use rafting::errors::RaftResult;

use rafting::raft::raft_server::RaftServer;
use rafting::web::ClientEvent;
use rafting::web::web_server::WebServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    info!("In main");
    setup_logger();
    //TODO - Accept argument as cli parameter. For now, args will do
    let args = Args::parse();
    let config = AppConfig::get_configuration(&args.config)?;
    let node_id = &args.node_id;
    let node_config = config.cluster.iter().find(|&n| n.node_id == *node_id).unwrap();
    let grpc_addr = &node_config.grpc_address;
    let web_addr = &node_config.web_address;
    let peers = config.peers(node_config);

    let grpc_address: SocketAddr = grpc_addr.as_str().parse().expect("Invalid address");
    let web_address: SocketAddr = web_addr.as_str().parse().expect("Invalid address");

    let (client_to_node_tx, node_from_client_rx) = mpsc::unbounded_channel::<(ClientEvent, oneshot::Sender<RaftResult<ClientEvent>>)>();
    let raft_server_handle = RaftServer::start_server(node_id, grpc_address, peers, node_from_client_rx, config.tick_interval_ms);
    let web_server_handle = WebServer::start_server(node_id, web_address, client_to_node_tx);

    tokio::try_join!(raft_server_handle, web_server_handle)?;

    Ok(())
}

pub fn setup_logger() {
    let subscriber = tracing_subscriber::fmt()
        .compact()
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .with_target(false)
        .finish();
    subscriber.init()
}

