// Activar el asignador de memoria Jemalloc
#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

use std::error::Error;
use std::net::SocketAddr;
use clap::Parser;
use agredadb_server::server;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, default_value_t = 50051)]
    port: u16,

    #[arg(long, default_value = "dbms")]
    mode: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let args = Args::parse();
    
    println!("🚀 AgredaDB Server v3.0 Starting...");
    println!("   - Mode: {}", args.mode);

    let addr: SocketAddr = format!("0.0.0.0:{}", args.port).parse()?;
    server::start_server(addr).await?;

    Ok(())
}
