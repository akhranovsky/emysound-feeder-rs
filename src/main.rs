use std::path::Path;

use anyhow::anyhow;
use clap::Parser;
use error_chain::error_chain;
use hls_m3u8::MediaPlaylist;
use reqwest::header::CONTENT_TYPE;
use reqwest::{StatusCode, Url};

error_chain! {
    foreign_links {
        Io(std::io::Error);
        HttpRequest(reqwest::Error);
    }
}

#[derive(Debug, Parser)]
struct Args {
    /// Stream URL (m3u8 file)
    stream_url: String,
    /// Output directory
    output_dir: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    simplelog::TermLogger::init(
        simplelog::LevelFilter::Debug,
        simplelog::Config::default(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )?;

    let stream_url: Url = args.stream_url.parse()?;
    let output_dir = Path::new(&args.output_dir);

    log::debug!("Fetching {} and saving to {output_dir:?}", &stream_url);

    let client = reqwest::Client::new();
    let request = client.get(stream_url).build()?;

    #[allow(clippy::never_loop)]
    loop {
        let response = client
            .execute(
                request
                    .try_clone()
                    .ok_or_else(|| anyhow!("Failed to clone request"))?,
            )
            .await?;
        match response.status() {
            StatusCode::OK => {
                log::info!("Received stream playlist.");

                if let Some(content_type) = response.headers().get(CONTENT_TYPE) {
                    let content_type = content_type.to_str()?;
                    if content_type == "application/vnd.apple.mpegurl; charset=UTF-8" {
                        let content = response.text().await?;
                        let m3u8 = MediaPlaylist::try_from(content.as_str())?;
                        log::debug!("Playlist: {m3u8:?}");
                    }
                }
                break;
            }
            _ => {
                log::error!("Failed to get playlist {}", response.text().await?);
                break;
            }
        }
    }
    Ok(())
}
