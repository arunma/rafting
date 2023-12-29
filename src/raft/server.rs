use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot::Sender;
use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::{error, info};

use crate::errors::{RaftError, RaftResult};
use crate::raft::node::RaftNode;
use crate::raft::TICK_INTERVAL_MS;
use crate::rpc::peer::PeerNetwork;
use crate::rpc::server::RaftGrpcServerStub;
use crate::rpc::RaftEvent;

pub struct RaftServer {
    //peer_network: Arc<Mutex<PeerNetwork>>,
}

impl RaftServer {
    /*    pub fn new(peer_network: Arc<Mutex<PeerNetwork>>) -> Self {
        Self {
            peer_network
        }
    }*/
    //TODO - Add id and peers from config
    pub async fn start_server(
        node_id: &str,
        address: SocketAddr,
        peers: HashMap<String, String>,
    ) -> Result<(), Box<dyn Error>> {
        info!("Initializing services...");

        ///CHANNELS
        //Establishes connectivity between GRPC server and the Raft Node
        let (server_to_node_tx, node_from_server_rx) =
            mpsc::unbounded_channel::<(RaftEvent, Option<Sender<RaftResult<RaftEvent>>>)>();
        /* Establishes connectivity between peers and node. The reason behind this indirection is to keep raft module independent of the RPC module.
          The alternative would be to make PeerNetwork available in the raft `node`.
          The key advantage that we could derive by this indirection is the ease of testing, since we will essentially be dealing only with channels in the node.
        */

        //let (peers_to_node_tx, node_from_peers_rx) = mpsc::unbounded_channel();
        let (node_to_peers_tx, peers_from_node_rx) = mpsc::unbounded_channel();

        //GRPC Server initialization
        let grpc_server = RaftGrpcServerStub::new(server_to_node_tx.clone());
        let grpc_handle = tokio::spawn(grpc_server.run(address));

        let peer_network = Arc::new(Mutex::new(PeerNetwork::new(
            node_id.to_string(),
            server_to_node_tx,
        )));
        let peer_clone = peers.clone();
        //Initializing peer network
        let peer_handle = {
            let mut peer_network = peer_network.clone();
            tokio::spawn(async move {
                let mut peer_network = peer_network.lock().await;
                peer_network.wait_for_peers(peers).await
            })
        };

        //RaftNode initialization
        //let (node_to_server_for_peers_tx, server_from_node_for_peers_rx) = mpsc::unbounded_channel();
        //Had to do this because oneshot sender cannot neither accept a Vec of responses nor be used more than once, obviously.
        //TODO - Source node id from config
        let node_names = peer_clone.keys().cloned().collect::<Vec<String>>();
        let node = RaftNode::new(node_id.to_string(), node_names, node_to_peers_tx); //FIXME - Shouldn't need the node_from_server_rx. Instead let's do step
        let node_handle = tokio::spawn(async move {
            Self::run(
                node,
                peers_from_node_rx,
                node_from_server_rx,
                peer_network.clone(),
            )
                .await
        });

        info!("Starting server at {:?}", address);
        let result = tokio::try_join!(node_handle, grpc_handle, peer_handle);

        if let Err(e) = result {
            panic!("Error in tokio::join!: {:?}", e)
        }
        info!("printing after tokio join");
        Ok(())
    }

    async fn run(
        mut node: RaftNode,
        mut peers_from_node_rx: mpsc::UnboundedReceiver<RaftEvent>,
        mut node_from_server_rx: mpsc::UnboundedReceiver<(
            RaftEvent,
            Option<oneshot::Sender<RaftResult<RaftEvent>>>,
        )>,
        peer_network: Arc<Mutex<PeerNetwork>>,
    ) -> RaftResult<()> {
        let mut ticker = tokio::time::interval(Duration::from_millis(TICK_INTERVAL_MS));
        loop {
            tokio::select! {
                //Every tick
                _ = ticker.tick() => node = node.tick()?,
                Some(event) = peers_from_node_rx.recv() => {
                    match event.clone() {
                        RaftEvent::PeerAppendEntriesRequestEvent(req) => {
                            info!("Append entry sent to peer from {} using request: {req:?}", node.id);
                            let responses = peer_network
                                .lock()
                                .await
                                .append_entries(req)
                                .await?;
                            let response_event = RaftEvent::PeerAppendEntriesResponseEvent(responses);
                            node = node.step((response_event, None)).await?;
                        },
                        RaftEvent::PeerVotesResponseEvent(_) => todo!(),
                        RaftEvent::PeerAppendEntriesResponseEvent(_) => todo!(),
                        RaftEvent::RequestVoteRequestEvent(_) => todo!(),
                        RaftEvent::RequestVoteResponseEvent(_) => todo!(),
                        RaftEvent::AppendEntriesResponseEvent(_) => todo!(),
                        RaftEvent::AppendEntriesRequestEvent(req) => {
                            info!("Append entry sent to peer: {req:?}");
                        },
                        RaftEvent::PeerVotesRequestEvent(req) => {
                            info!("Requesting peer votes from {} using request: {req:?}", node.id);
                            let responses = peer_network
                                .lock()
                                .await
                                .request_vote(req)
                                .await?;
                            let response_event = RaftEvent::PeerVotesResponseEvent(responses);
                            node = node.step((response_event, None)).await?;
                        },

                    }
                                    },
                Some(event) = node_from_server_rx.recv() => {
                    node = node.step(event).await?;
                }
            }
        }
    }
}
