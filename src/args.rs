use std::path::PathBuf;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(
author = "Arun Manivannan",
version = "0.0.1",
about = "Rafting - A Raft implementation in Rust",
arg_required_else_help = true
)]
pub struct Args {
    #[arg(short = 'n', long = "node-id", help = "Node identifier. (Refer to cluster_config.yaml for node ids)", required = true)]
    pub node_id: String,

    #[arg(short = 'c', long = "cluster-config", value_name = "FILE", required = true)]
    pub config: PathBuf,
}