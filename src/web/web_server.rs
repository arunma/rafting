use std::error::Error;
use std::net::SocketAddr;
use anyhow::{anyhow, Context};
use axum::{Json, Router};
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, error, info};
use crate::errors::RaftResult;
use crate::web::{ClientEvent, SetCommand};

pub struct WebServer {}

impl WebServer {
    pub async fn start_server(
        node_id: &str,
        address: SocketAddr,
        client_to_server_tx: UnboundedSender<(ClientEvent, oneshot::Sender<RaftResult<ClientEvent>>)>,
    ) -> Result<(), Box<dyn Error>> {
        info!("Initializing web services on {node_id} at {address:?}...");

        let app_state = AppState { client_to_server_tx };
        let router = Router::new()
            .route("/command", post(command_handler))
            .with_state(app_state);

        let listener = TcpListener::bind(address)
            .await
            .context("Unable to bind to the specified host and port")?;

        axum::serve(listener, router)
            .await
            .map_err(|e| {
                error!("Unable to start server at address : {address:?} due to {e:?}");
                //RaftError::ApplicationStartup(format!("Unable to start server at address : {address:?} due to {e:?}"))
                anyhow!("Unable to start server at address : {address:?} due to {e:?}").into()
            })
    }
}

#[derive(Debug, Clone)]
pub struct AppState {
    client_to_server_tx: UnboundedSender<(ClientEvent, oneshot::Sender<RaftResult<ClientEvent>>)>,
}

async fn command_handler(
    State(AppState { client_to_server_tx }): State<AppState>,
    Json(command): Json<SetCommand>,
) -> (StatusCode, String) {
    //TODO - Implement IntoResponse for RaftError (and therefore RaftResult) and move away from this tuple response
    debug!("Received client command: {:?}", command);
    let (tx, rx) = oneshot::channel::<RaftResult<ClientEvent>>();
    let event = ClientEvent::CommandRequestEvent(command);
    //FIXME - Modify this `expect` when we change the return type to Result
    match client_to_server_tx.send((event, tx)) {
        Ok(_) => {}
        Err(e) => {
            error!("Unable to send command to the server. Server is probably initializing or the request is directed to a non-leader node {e:?}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Unable to send command to the server. Server is probably initializing or the request is directed to a non-leader node".to_string());
        }
    }
    match rx.await {
        Ok(Ok(ClientEvent::CommandResponseEvent(resp))) => {
            debug!("Received client response: {:?}", resp);
            (StatusCode::OK, resp.to_string())
        }
        Err(e) => {
            error!("Error while receiving response from the server while executing the command: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Error while receiving response from the server while executing the command: {:?}", e))
        }
        _ => {
            error!("Unhandled event in command_handler");
            (StatusCode::INTERNAL_SERVER_ERROR, "Unhandled event in command_handler".to_string())
        }
    }
}