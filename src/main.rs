//! Command-line entry point for cpa-rs.

use std::env;
use std::error::Error;
use std::path::PathBuf;

use cpa_rs::{app, config::Config};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let path = match (args.next(), args.next(), args.next()) {
        (None, None, None) => PathBuf::from("config.yaml"),
        (Some(flag), Some(path), None) if flag == "--config" => PathBuf::from(path),
        _ => return Err("usage: cpa-rs [--config <path>]".into()),
    };
    let config = Config::load(path)?;
    let host = config.server.host.trim();
    let host = if host.is_empty() { "0.0.0.0" } else { host };
    let host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    let listener = TcpListener::bind((host, config.server.port)).await?;
    let router = app::router(config)?;
    println!("cpa-rs listening on {}", listener.local_addr()?);
    axum::serve(listener, router).await?;
    Ok(())
}
