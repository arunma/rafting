fn main() -> Result<(), Box<dyn std::error::Error>>{
    println!("Compiling protos...");
    /*tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile(&["src/proto/raft.proto"],
            &["src/proto"])?;*/
    tonic_build::compile_protos("proto/raft.proto")?;
    println!("Finished compiling protos");
    Ok(())
}