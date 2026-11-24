mod cli;
mod db;
mod format;
mod service;
mod spotify;
mod web;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    cli::run().await
}
