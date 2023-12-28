use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, Mutex};
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::{error, info};

use crate::errors::RaftError;
use crate::raft::node::RaftNode;
use crate::raft::TICK_INTERVAL_MS;
use crate::rpc::peer::PeerNetwork;
use crate::rpc::RaftRequest;
use crate::rpc::server::RaftGrpcServerStub;

pub struct RaftServer {}

impl RaftServer {
    //TODO - Add id and peers from config
    pub async fn start_server(node_id: &str, address: SocketAddr, peers: HashMap<String, String>) -> Result<(), Box<dyn Error>> {
        info!("Starting server at {:?}", address);
        let (server_tx, server_rx) = mpsc::unbounded_channel();
        let grpc_server = RaftGrpcServerStub::new(server_tx);

        let peer_network = Arc::new(Mutex::new(PeerNetwork::default()));
        //FIXME - enable once node implementation is done
        let peer_handle = {
            let mut peer_network = Arc::clone(&peer_network);
            tokio::spawn(async move {
                let mut peer_network = peer_network.lock().await;
                peer_network.wait_for_peers(peers).await
            })
        };


        let (peer_tx, to_peer_rx) = mpsc::unbounded_channel();
        //TODO - Source node id from config
        let node = RaftNode::new(node_id.to_string(), server_rx, peer_tx);
        let node_handle = tokio::spawn(async move {
            Self::run(node, to_peer_rx).await
        });

        /*
        let node =???
        Pass the receiver to the node
         */

        info!("Running server at {:?}", address);
        let grpc_handle = tokio::spawn(grpc_server.run(address));

        let result = tokio::try_join!(
                node_handle,
                grpc_handle,
                peer_handle
            );

        if let Err(e) = result {
            error!("Error in tokio::join!: {:?}", e)
        }


        Ok(())
    }


    async fn run(mut node: RaftNode, mut to_peer_rx: UnboundedReceiver<RaftRequest>) -> Result<(), RaftError> {
        let mut ticker = tokio::time::interval(Duration::from_millis(TICK_INTERVAL_MS));
        loop {
            tokio::select! {
                _= ticker.tick() => node = node.tick()?,
                Some(msg) = to_peer_rx.recv()  => {
                    info!("Printing message received from Node directed towards peers: {msg:?}")


                },


            }
        }
    }
}