use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::net::SocketAddr;

use tracing::info;
use tracing_subscriber::util::SubscriberInitExt;

use rafting::raft::server::RaftServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    info!("In main");
    setup_logger();
    //TODO - Accept argument as cli parameter. For now, args will do
    let args: Vec<String> = env::args().collect();
    //let address = "[::1]:7070".parse()?;
    info!("Args: {:?}", &args);
    let id = &args[1];
    let addr = &args[2];
    let peer_id = &args[3];
    let peer_addr = &args[4];
    let peers = HashMap::from([(peer_id.to_string(), peer_addr.to_string())]);
    let address: SocketAddr = addr.as_str().parse().expect("Invalid address");
    RaftServer::start_server(id, address, peers).await?;
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

