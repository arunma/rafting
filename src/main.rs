use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::net::SocketAddr;
use tokio::sync::{mpsc, oneshot};

use tracing::info;
use tracing_subscriber::util::SubscriberInitExt;
use rafting::errors::RaftResult;

use rafting::raft::raft_server::RaftServer;
use rafting::web::ClientEvent;
use rafting::web::web_server::WebServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    info!("In main");
    setup_logger();
    //TODO - Accept argument as cli parameter. For now, args will do
    let args: Vec<String> = env::args().collect();
    //let address = "[::1]:7070".parse()?;
    info!("Args: {:?}", &args);
    let id = &args[1];
    let grpc_addr = &args[2];
    let web_addr = &args[3];
    let peer_id = &args[4];
    let peer_addr = &args[5];
    let peers = HashMap::from([(peer_id.to_string(), peer_addr.to_string())]);
    let grpc_address: SocketAddr = grpc_addr.as_str().parse().expect("Invalid address");
    let web_address: SocketAddr = web_addr.as_str().parse().expect("Invalid address");

    let (client_to_node_tx, node_from_client_rx) = mpsc::unbounded_channel::<(ClientEvent, oneshot::Sender<RaftResult<ClientEvent>>)>();
    let raft_server_handle = RaftServer::start_server(id, grpc_address, peers, node_from_client_rx);
    let web_server_handle = WebServer::start_server(id, web_address, client_to_node_tx);

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

