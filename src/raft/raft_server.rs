use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::oneshot::Sender;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::{debug, error, info};

use crate::errors::{RaftError, RaftResult};
use crate::raft::raft_node::RaftNode;
use crate::rpc::rpc_peer_network::PeerNetwork;
use crate::rpc::rpc_server::RaftGrpcServerStub;
use crate::rpc::RaftEvent;
use crate::web::ClientEvent;

pub struct RaftServer {}

impl RaftServer {
    //TODO - Add id and peers from config
    pub async fn start_server(
        node_id: &str,
        address: SocketAddr,
        peers: HashMap<String, String>,
        node_from_client_rx: UnboundedReceiver<(ClientEvent, oneshot::Sender<RaftResult<ClientEvent>>)>,
        tick_interval_ms: u64,
    ) -> Result<(), Box<dyn Error>> {
        info!("Initializing grpc services on {node_id} at {address:?}...");

        //CHANNELS
        //Establishes connectivity between GRPC server and the Raft Node
        let (server_to_node_tx, node_from_server_rx) =
            mpsc::unbounded_channel::<(RaftEvent, Option<Sender<RaftResult<RaftEvent>>>)>();
        /* Establishes connectivity between peers and node. The reason behind this indirection is to keep raft module independent of the RPC module.
          The alternative would be to make PeerNetwork available in the raft `node`.
          The key advantage that we could derive by this indirection is the ease of testing, since we will essentially be dealing only with channels in the node.
        */
        let (node_to_peers_tx, peers_from_node_rx) = mpsc::unbounded_channel();

        //GRPC Server initialization
        let grpc_server = RaftGrpcServerStub::new(server_to_node_tx);
        let grpc_handle = tokio::spawn(grpc_server.run(address));

        let peer_network = Arc::new(Mutex::new(PeerNetwork::new(
            node_id.to_string(),
        )));
        let peer_clone = peers.clone();
        //Initializing peer network
        let peer_handle = {
            let peer_network = peer_network.clone();
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
                tick_interval_ms,
                peers_from_node_rx,
                node_from_server_rx,
                node_from_client_rx,
                peer_network.clone(),
            )
                .await
        });

        debug!("Starting server at {:?}", address);
        let result = tokio::try_join!(node_handle, grpc_handle, peer_handle);

        if let Err(e) = result {
            panic!("Error in tokio::join!: {:?}", e)
        }
        info!("printing after tokio join");
        Ok(())
    }

    async fn run(
        mut node: RaftNode,
        tick_interval_ms: u64,
        mut peers_from_node_rx: mpsc::UnboundedReceiver<RaftEvent>,
        mut node_from_server_rx: mpsc::UnboundedReceiver<(
            RaftEvent,
            Option<oneshot::Sender<RaftResult<RaftEvent>>>,
        )>,
        mut node_from_client_rx: UnboundedReceiver<(ClientEvent, oneshot::Sender<RaftResult<ClientEvent>>)>,
        peer_network: Arc<Mutex<PeerNetwork>>,
    ) -> RaftResult<()> {
        let mut ticker = tokio::time::interval(Duration::from_millis(tick_interval_ms));
        loop {
            tokio::select! {
                //Every tick
                _ = ticker.tick() => node = node.tick()?,
                /*
                The following messages are received from Node and is meant for the peers in the cluster.
                So, we use the client stubs in the PeerNetwork to send the messages to the peers.
                 */
                Some(event) = peers_from_node_rx.recv() => {
                    match event.clone() {
                        RaftEvent::AppendEntriesRequestEvent(req) => {
                            debug!("AppendEntries request to be send to peers from {} using request: {req:?}", node.id());

                            let response = peer_network
                            .lock()
                            .await
                            .append_entries(req)
                            .await?;
                            let response_event = RaftEvent::AppendEntriesResponseEvent(response);
                            node = node.step((response_event, None))?;
                        },
                        RaftEvent::PeerVotesRequestEvent(req) => {
                            info!("Requesting peer votes from {} using request: {req:?}", node.id());
                            let responses = peer_network
                                .lock()
                                .await
                                .request_vote(req)
                                .await?;
                            let response_event = RaftEvent::PeerVotesResponseEvent(responses);
                            node = node.step((response_event, None))?;
                        },
                        _ => {
                            error!("Some unexpected match pattern came up in node_from_server_rx stream");
                            return Err(RaftError::InternalServerErrorWithContext("Some unexpected match pattern came up in node_from_server_rx stream".to_string()))
                        }
                    }
                },
                Some(event) = node_from_server_rx.recv() => {
                    node = node.step(event)?;
                },
                Some(event) = node_from_client_rx.recv() => {
                    node.handle_client_request(event)?;
                }

            }
        }
    }
}
