use std::path::Path;
use std::time::Duration;
// use std::time::Duration;

use anyhow::{anyhow, Context};
use clap::Parser;
use hls_m3u8::{MediaPlaylist, MediaSegment};
use lazy_static::lazy_static;
use lru::LruCache;
use regex::Regex;
use reqwest::header::CONTENT_TYPE;
use reqwest::{StatusCode, Url};
use uuid::Uuid;

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

    let mut segment_number_filter = SegmentNumberFilter::new();

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
                        m3u8.segments
                            .iter()
                            .filter(|(_, segment)| segment_number_filter.need_download(segment))
                            .for_each(|(_, segment)| {
                                match KostaRadioSegmentInfo::try_from(segment) {
                                    Ok(info) => {
                                        match info.suggested_content_kind() {
                                            SuggestedSegmentContentKind::None => {
                                                log::info!("Segment#{} SKIPPED: unknown kind, artist={}, title={}", segment.number(), info.artist, info.title);
                                            }
                                            SuggestedSegmentContentKind::Talk => {
                                                log::info!("Segment#{} DOWNLOAD: likely talk, artist: {}, title: {}", segment.number(), info.artist, info.title);
                                            },
                                            SuggestedSegmentContentKind::Advertisement => {
                                                log::info!("Segment#{} DOWNLOAD: likely advertisment, artist: {}, title: {}", segment.number(), info.artist, info.title);
                                            },
                                            SuggestedSegmentContentKind::Music => {
                                                log::info!("Segment#{} DOWNLOAD: likely music, artist: {}, title: {}", segment.number(), info.artist, info.title);
                                            },
                                        }
                                        log::debug!("Segment#{} info: {info:?}", segment.number());
                                    }
                                    Err(e) => {
                                        // It could be an advertisement.
                                        // #EXTINF:10,offset=0,adContext=''
                                        if let Some(title) = segment.duration.title() {
                                            if title.contains("adContext=") {
                                                log::info!("Segment#{} DOWNLOAD: advertisment: title={title}", segment.number());
                                            }
                                        } else {
                                            // Happens at the first download and sometimes in the middle then section changes. ignore.
                                            log::info!("Segment#{} SKIPPED: no info: {e:#?}", segment.number());
                                            log::debug!(
                                                "Segment#{} title={:?}",
                                                segment.number(),
                                                segment.duration.title()
                                            );
                                        }

                                    }
                                }
                            });
                        tokio::time::sleep(m3u8.duration() / 2).await;
                    }
                }
            }
            _ => {
                log::error!("Failed to get playlist {}", response.text().await?);
                break;
            }
        }
    }
    Ok(())
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
        let seen = number <= self.last_seen_number;
        self.last_seen_number = number;
        !seen
    }
}

struct UrlFilter {
    lru_cache: LruCache<Url, ()>,
}

#[allow(dead_code)]
impl UrlFilter {
    fn new() -> Self {
        Self {
            lru_cache: LruCache::new(10),
        }
    }
}

impl SegmentDownloadFilter for UrlFilter {
    fn need_download(&mut self, segment: &MediaSegment) -> bool {
        if let Ok(url) = segment.uri().parse() {
            self.lru_cache.put(url, ()).is_none()
        } else {
            false
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

enum SuggestedSegmentContentKind {
    None,
    Talk,
    Advertisement,
    Music,
}
