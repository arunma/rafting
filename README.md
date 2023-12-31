# Rafting: A Rust Implementation of the Raft Consensus Algorithm

## Overview

**Rafting** is a learning project that aims to provide a concise implementation of the Raft consensus algorithm in Rust.

## What it is

- **Educational:** This project is intended to be a learning resource for understanding the Raft consensus algorithm.
- **Minimal:** This project implements the essential features of the Raft algorithm, including
    - leader election
    - log replication
    - log consistency
    - client interaction with the cluster (using REST)
    - in-memory storage
- **Tested:** This project is tested using a detailed set of unit tests.

## What it isn't

- **Production-ready:** This project is not intended to be used in production.
- **Comprehensive:** This project is not a complete implementation of the Raft algorithm. It is missing features such as
    - membership changes
    - log compaction
    - snapshotting
    - proxying of client requests to the leader
    - persistent storage

## Implementation detail

- **gRPC Integration:** Utilizes gRPC for Remote Procedure Calls (RPC) between nodes. Protocol definitions are specified in `proto/raft.proto`.
  The `rpc` module houses server and client stubs for gRPC-based interactions with the cluster members.
- **Raft Module:** The internal interactions within the node is orchestrated through tokio. The `raft` module is designed to be agnostic to the RPC
  mechanism (although the models are reused in places), communicating
  exclusively through `tokio` channels with the `rpc` module.
- **REST API:** The `rafting` binary exposes a REST API for interacting with the cluster (leader only). The API is implemented using the `axum` web
  framework.

## Demo

![](https://github.com/arunma/rafting/blob/master/rafting.gif)

## Getting Started

### Installation

Clone the repository:

```bash
git clone https://github.com/yourusername/rafting.git
cd rafting
```

Build and run the project:

```bash
cargo build
cargo run
```

## Project Structure

- **`proto/raft.proto`:** protobuf file specifying gRPC service definitions.
- **`src/rpc`:** Module containing server and client stubs for gRPC-based interactions.
- **`src/raft`:** Module implementing the Raft consensus algorithm.
- **`src/main.rs`:** Main entry point for the project.

## Usage

### Running a three-node cluster locally : (node1, node2, node3)

To add more node, add a new entry in the `cluster_config.yaml` file.

```bash
cargo run -- -n node1 -c config/cluster_config.yaml
cargo run -- -n node2 -c config/cluster_config.yaml
cargo run -- -n node3 -c config/cluster_config.yaml
```

### Release mode

Optionally, you could run it in release mode by running:

```bash
cargo build --release
```

and then

```bash
./target/release/rafting -n node1 -c config/cluster_config.yaml
./target/release/rafting -n node2 -c config/cluster_config.yaml
./target/release/rafting -n node3 -c config/cluster_config.yaml
```

### Interacting with the cluster

```bash
curl -i -X POST -H  "Content-Type: application/json" --request POST --data '{"key":"a","value":"1"}'  http://<LEADER_NODE>:<WEB_PORT>/command
curl -i -X POST -H  "Content-Type: application/json" --request POST --data '{"key":"b","value":"2"}'  http://<LEADER_NODE>:<WEB_PORT>/command
curl -i -X POST -H  "Content-Type: application/json" --request POST --data '{"key":"c","value":"3"}'  http://<LEADER_NODE>:<WEB_PORT>/command
```

## Contributing

Contributions to Rafting are welcome! Whether you want to fix a bug, add a feature, or improve documentation, feel free to open an issue or submit a
pull request.

## License

Rafting is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## References

- The Raft paper by Diego Ongaro and John Ousterhout - [In Search of an Understandable Consensus Algorithm](https://raft.github.io/raft.pdf).
