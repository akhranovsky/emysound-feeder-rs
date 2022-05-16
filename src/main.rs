use std::fmt::Display;
use std::time::Duration;
// use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use bytes::Bytes;
use chrono::Utc;
use clap::Parser;
use emycloud_client_rs::MediaSource;
use hls_m3u8::{MediaPlaylist, MediaSegment};
use lazy_static::lazy_static;
use regex::Regex;
use reqwest::header::CONTENT_TYPE;
use reqwest::{StatusCode, Url};
use tokio_stream::StreamExt;
use uuid::Uuid;

mod db;

#[derive(Debug, Parser)]
struct Args {
    /// Stream URL (m3u8 file)
    stream_url: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    simplelog::TermLogger::init(
        simplelog::LevelFilter::Info,
        simplelog::Config::default(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )?;

    let stream_url: Url = args.stream_url.parse()?;

    log::debug!("Fetching {stream_url} ");

    let client = reqwest::Client::new();
    let mut segment_number_filter = SegmentNumberFilter::new();

    loop {
        let response = client.get(stream_url.clone()).send().await?;

        match response.status() {
            StatusCode::OK => {
                log::debug!("Received stream playlist.");

                if let Some(content_type) = response.headers().get(CONTENT_TYPE) {
                    let content_type = content_type.to_str()?;
                    if content_type == "application/vnd.apple.mpegurl; charset=UTF-8" {
                        let content = response.text().await?;
                        let m3u8 = MediaPlaylist::try_from(content.as_str())?;
                        let downloads: Vec<SegmentDownloadInfo> = m3u8.segments
                            .iter()
                            .filter(|(_, segment)| segment_number_filter.need_download(segment))
                            .filter_map(|(_, segment)| {
                                let url: Option<Url> = segment.uri().parse().ok();
                                if url.is_none() {
                                    log::error!("Segment#{} invalid url {}", segment.number(), segment.uri());
                                    return None;
                                }
                                let url = url.unwrap();

                                match KostaRadioSegmentInfo::try_from(segment) {
                                    Ok(info) => {
                                        log::debug!("Segment#{} info: {info:?}", segment.number());
                                        let kind = info.suggested_content_kind();
                                        let download_info = SegmentDownloadInfo{
                                                    url,
                                                    artist: info.artist.clone(),
                                                    title: info.title.clone(),
                                                    kind,
                                                };
                                        match kind {
                                            SuggestedSegmentContentKind::None => {
                                                log::info!("Segment#{} SKIPPED: unknown kind, artist={}, title={}", segment.number(), info.artist, info.title);
                                                log::info!("Segment#{} title={:?}", segment.number(), segment.duration.title());
                                                None
                                            }
                                            SuggestedSegmentContentKind::Talk => {
                                                log::info!("Segment#{} DOWNLOAD: likely talk, artist: {}, title: {}", segment.number(), info.artist, info.title);
                                                Some(download_info)
                                            },
                                            SuggestedSegmentContentKind::Advertisement => {
                                                log::info!("Segment#{} DOWNLOAD: likely advertisment, artist: {}, title: {}", segment.number(), info.artist, info.title);
                                                Some(download_info)
                                            },
                                            SuggestedSegmentContentKind::Music => {
                                                log::info!("Segment#{} DOWNLOAD: likely music, artist: {}, title: {}", segment.number(), info.artist, info.title);
                                                Some(download_info)
                                            },
                                        }
                                    }
                                    Err(e) => {
                                        // It could be an advertisement.
                                        // #EXTINF:10,offset=0,adContext=''
                                        if let Some(title) = segment.duration.title() {
                                            if title.contains("adContext=") {
                                                log::info!("Segment#{} DOWNLOAD: advertisment: title={title}", segment.number());
                                                return Some(SegmentDownloadInfo{ url, artist: "Advertisement".to_string(), title: "Advertisement".to_string() , kind: SuggestedSegmentContentKind::Advertisement });
                                            }
                                            None
                                        } else {
                                            // Happens at the first download and sometimes in the middle then section changes. ignore.
                                            log::info!("Segment#{} SKIPPED: no info: {e:#?}", segment.number());
                                            log::debug!(
                                                "Segment#{} title={:?}",
                                                segment.number(),
                                                segment.duration.title()
                                            );
                                            None
                                        }
                                    }
                                }
                            }).collect();

                        let mut stream = tokio_stream::iter(downloads);
                        while let Some(info) = stream.next().await {
                            match download(&info).await {
                                Ok(bytes) => {
                                    let filename = info.filename();
                                    let source = MediaSource::Bytes(&filename, &bytes);
                                    match emycloud_client_rs::query(source.clone()).await {
                                        Ok(results) => {
                                            let matches: Vec<Uuid> = results
                                                .iter()
                                                .filter_map(|r| {
                                                    Uuid::try_parse(&r.track.id).ok().and_then(
                                                        |id| {
                                                            let coverage = r
                                                                .audio
                                                                .as_ref()
                                                                .and_then(|m| {
                                                                    m.coverage.query_coverage
                                                                })
                                                                .map(|c| (c * 100f32).trunc() as u8)
                                                                .unwrap_or_default();

                                                            log::info!(
                                                        "{} '{}'/'{}' matches '{}'/'{}' {}%",
                                                        info.url,
                                                        info.title,
                                                        info.artist,
                                                        r.track
                                                            .title
                                                            .as_ref()
                                                            .unwrap_or(&"None".to_string()),
                                                        r.track
                                                            .artist
                                                            .as_ref()
                                                            .unwrap_or(&"None".to_string()),
                                                        coverage
                                                    );

                                                            if coverage >= 80 {
                                                                Some(id)
                                                            } else {
                                                                None
                                                            }
                                                        },
                                                    )
                                                })
                                                .collect();

                                            if matches.is_empty() {
                                                match emycloud_client_rs::insert(
                                                    source,
                                                    info.artist.clone(),
                                                    info.title.clone(),
                                                )
                                                .await
                                                {
                                                    Ok(id) => {
                                                        log::info!(
                                                            "Inserted new track '{}'/'{}': {id}",
                                                            info.artist,
                                                            info.title,
                                                        )

                                                        // TODO: Update meta info.
                                                    }
                                                    Err(e) => {
                                                        log::error!("Failed to insert track '{}'/'{}': {e:#}", info.artist, info.title);
                                                    }
                                                }
                                            } else {
                                                for id in &matches {
                                                    log::info!("Update metadata for {id}");
                                                    // TODO: Update meta info.
                                                }
                                            }
                                        }
                                        Err(_) => todo!(),
                                    }
                                }
                                Err(e) => {
                                    log::error!("Failed to download {}: {e:#}", info.url)
                                }
                            }
                        }

                        tokio::time::sleep(m3u8.duration() / 2).await;
                    }
                }
            }
            _ => {
                let msg = format!("Failed to get playlist {}", response.text().await?);
                log::error!("{msg}");
                bail!(msg);
            }
        }
    }
}

async fn download(info: &SegmentDownloadInfo) -> Result<Bytes> {
    let response = reqwest::get(info.url.clone()).await?;

    log::debug!(
        "Downloaded {}, {} bytes",
        info.url,
        response.content_length().unwrap_or_default()
    );

    response.bytes().await.context("Retrieve bytes")

    // log::debug!("Saving to {:?}", path);
    // tokio::fs::write(&path, bytes).await?;

    // path.to_str()
    //     .map(|s| s.to_owned())
    //     .ok_or_else(|| anyhow!("Failed to convert path"))
}

#[derive(Debug, Clone)]
struct SegmentDownloadInfo {
    url: Url,
    artist: String,
    title: String,
    kind: SuggestedSegmentContentKind,
}

impl SegmentDownloadInfo {
    fn filename(&self) -> String {
        format!(
            "{}_{}_{}_{}.{}",
            Utc::now().format("%Y-%m-%d_%H-%M-%S"),
            self.kind,
            self.artist,
            self.title,
            self.url
                .path_segments()
                .and_then(|s| s.last())
                .unwrap_or("unknown")
        )
    }
}
trait SegmentDownloadFilter {
    /// Returs `true` if `segment` should be downloaded.
    fn need_download(&mut self, segment: &MediaSegment) -> bool;
}

struct SegmentNumberFilter {
    last_seen_number: usize,
}

impl SegmentNumberFilter {
    fn new() -> Self {
        Self {
            last_seen_number: 0,
        }
    }
}

impl SegmentDownloadFilter for SegmentNumberFilter {
    fn need_download(&mut self, segment: &MediaSegment) -> bool {
        let number = segment.number();
        if number <= self.last_seen_number {
            false
        } else {
            self.last_seen_number = number;
            true
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
struct KostaRadioSegmentInfo {
    title: String,
    artist: String,
    song_spot: char,
    media_base_id: i64,
    itunes_track_id: i64,
    amg_track_id: i64,
    amg_artist_id: i64,
    ta_id: i64,
    tp_id: i64,
    cartcut_id: i64,
    amg_artwork_url: Option<Url>,
    length: Duration,
    uns_id: i64,
    spot_instance_id: Option<Uuid>,
}

#[allow(dead_code)]
impl KostaRadioSegmentInfo {
    fn is_music(&self) -> bool {
        (self.song_spot == 'M' || self.song_spot == 'F')
            && self.length > Duration::ZERO
            && (self.media_base_id > 0
                || self.itunes_track_id > 0
                || (self.amg_artist_id > 0 && self.amg_track_id > 0)
                || self.amg_artwork_url.is_some())
    }

    fn is_talk(&self) -> bool {
        // song_spot=T MediaBaseId=0 itunesTrackId=0 amgTrackId=0 amgArtistId=0 TAID=0 TPID=0 cartcutId=0 amgArtworkURL="" length="00:00:00" unsID=0 spotInstanceId=-1
        self.song_spot == 'T'
            && self.media_base_id == 0
            && self.itunes_track_id == 0
            && self.amg_artist_id == 0
            && self.amg_track_id == 0
            && self.ta_id == 0
            && self.tp_id == 0
            && self.amg_artwork_url.is_none()
            && self.spot_instance_id.is_none()
            && self.length == Duration::ZERO
    }

    fn is_advertisment(&self) -> bool {
        // #EXTINF:10,offset=0,adContext=''
        // song_spot=F MediaBaseId=0 itunesTrackId=0 amgTrackId=\"-1\" amgArtistId=\"0\" TAID=\"0\" TPID=\"0\" cartcutId=\"0\" amgArtworkURL=\"null\" length=\"00:02:03\" unsID=\"-1\" spotInstanceId=\"688d6785-f34c-35a8-3255-1a9dd167fbd2\""
        self.song_spot == 'F'
            && self.media_base_id == 0
            && self.itunes_track_id == 0
            && self.amg_artist_id == 0
            && self.amg_track_id == -1
            && self.ta_id == 0
            && self.tp_id == 0
            && self.cartcut_id == 0
            && self.amg_artwork_url.is_none()
            && self.spot_instance_id.is_some()
    }

    fn suggested_content_kind(&self) -> SuggestedSegmentContentKind {
        if self.is_music() {
            return SuggestedSegmentContentKind::Music;
        }
        if self.is_talk() {
            return SuggestedSegmentContentKind::Talk;
        }
        if self.is_advertisment() {
            return SuggestedSegmentContentKind::Advertisement;
        }
        SuggestedSegmentContentKind::None
    }
}

impl TryFrom<&str> for KostaRadioSegmentInfo {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        lazy_static! {
            static ref RE: Regex = Regex::new(r#"(?:offset=\d+,)?title="(.+?)",artist="(.+?)",url="song_spot=\\"(\w)\\" MediaBaseId=\\"(-?\d+)\\" itunesTrackId=\\"(-?\d+)\\" amgTrackId=\\"(-?\d+)\\" amgArtistId=\\"(-?\d+)\\" TAID=\\"(-?\d+)\\" TPID=\\"(-?\d+)\\" cartcutId=\\"(-?\d+)\\" amgArtworkURL=\\"(.*?)\\" length=\\"(\d\d:\d\d:\d\d)\\" unsID=\\"(-?\d+)\\" spotInstanceId=\\"(.+?)\\"""#).unwrap();
        }

        let caps = RE
            .captures(value)
            .ok_or_else(|| anyhow!("Failed to match"))?;

        Ok(Self {
            title: caps[1].to_owned(),
            artist: caps[2].to_owned(),
            song_spot: caps[3]
                .chars()
                .next()
                .ok_or_else(|| anyhow!("Failed to parse song_spot"))?,
            media_base_id: caps[4].parse::<i64>()?,
            itunes_track_id: caps[5].parse::<i64>()?,
            amg_track_id: caps[6].parse::<i64>()?,
            amg_artist_id: caps[7].parse::<i64>()?,
            ta_id: caps[8].parse::<i64>()?,
            tp_id: caps[9].parse::<i64>()?,
            cartcut_id: caps[10].parse::<i64>()?,
            amg_artwork_url: caps[11].to_owned().parse().ok(),
            length: chrono::NaiveTime::signed_duration_since(
                chrono::NaiveTime::parse_from_str(&caps[12], "%H:%M:%S")?,
                chrono::NaiveTime::from_hms(0, 0, 0),
            )
            .to_std()?,
            uns_id: caps[13].parse::<i64>()?,
            spot_instance_id: Uuid::try_parse(&caps[14]).ok(),
        })
    }
}

impl TryFrom<&MediaSegment<'_>> for KostaRadioSegmentInfo {
    type Error = anyhow::Error;

    fn try_from(segment: &MediaSegment) -> Result<Self, Self::Error> {
        if let &Some(title) = &segment.duration.title() {
            KostaRadioSegmentInfo::try_from(title.as_ref())
        } else {
            Err(anyhow!("No title"))
        }
    }
}

#[derive(Debug, Copy, Clone)]
enum SuggestedSegmentContentKind {
    None,
    Talk,
    Advertisement,
    Music,
}

impl Display for SuggestedSegmentContentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SuggestedSegmentContentKind::None => f.write_str("none"),
            SuggestedSegmentContentKind::Talk => f.write_str("talk"),
            SuggestedSegmentContentKind::Advertisement => f.write_str("advertisement"),
            SuggestedSegmentContentKind::Music => f.write_str("music"),
        }
    }
}
