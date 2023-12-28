use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::info;

use crate::raft::node::RaftNode;
use crate::raft::TICK_MS;
use crate::rpc::RaftRequest;
use crate::rpc::server::RaftGrpcServerStub;

pub struct RaftServer {}

impl RaftServer {
    //TODO - Add id and peers from config
    pub async fn start_server(address: SocketAddr) -> Result<(), Box<dyn Error>> {
        info!("Starting server at {:?}", address);
        let (server_tx, server_rx) = mpsc::unbounded_channel();
        let grpc_server = RaftGrpcServerStub::new(server_tx);


        //Build peers
        let mut peers = HashMap::new();
        peers.insert("server2", "http://[::1]:8080");
        //FIXME - enable once node implementation is done
        //let peer_clients = wait_for_peers(&peers).await;

        let (peer_tx, to_peer_rx) = mpsc::unbounded_channel();
        //TODO - Source node id from config
        let mut node = RaftNode::new("node1".to_string(), server_rx, peer_tx);
        Self::run(node, to_peer_rx).await?;

        /*
        let node =???
        Pass the receiver to the node
         */

        info!("Running server at {:?}", address);
        grpc_server.run(address).await?;
        Ok(())
    }

    async fn run(mut node: RaftNode, mut to_peer_rx: UnboundedReceiver<RaftRequest>) -> Result<(), Box<dyn Error>> {
        let mut ticker = tokio::time::interval(Duration::from_millis(TICK_MS));
        loop {
            tokio::select! {
                _= ticker.tick() => node = node.tick().await,
                Some(msg) = to_peer_rx.recv()  => {
                    println!("Printing message received from Node directed towards peers: {msg:?}")

                },


            }
        }
    }
}