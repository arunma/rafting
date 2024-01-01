fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Compiling protos...");
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile(&["proto/raft.proto"],
                 &["proto"])?;
    println!("Finished compiling protos");
    Ok(())
}