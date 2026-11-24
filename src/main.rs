mod cli;
mod db;
mod format;
mod spotify;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    cli::run().await
}
