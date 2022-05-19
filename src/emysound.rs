use anyhow::{anyhow, Context};
use bytes::Bytes;
use emycloud_client_rs::MediaSource;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct QueryResult {
    id: Uuid,
    coverage: f32,
    artist: Option<String>,
    title: Option<String>,
}

impl TryFrom<&emycloud_client_rs::QueryResult> for QueryResult {
    type Error = anyhow::Error;

    fn try_from(value: &emycloud_client_rs::QueryResult) -> Result<Self, Self::Error> {
        let id = Uuid::try_parse(&value.id).context("Parsing uuid")?;
        let coverage = value
            .audio
            .as_ref()
            .and_then(|audio| audio.coverage.query_coverage)
            .ok_or_else(|| anyhow!("Failed to get coverage"))?;
        let artist = value.track.artist.clone();
        let title = value.track.title.clone();

        Ok(Self {
            id,
            coverage,
            artist,
            title,
        })
    }
}

const MIN_CONFIDENCE: f32 = 0.8f32;

pub async fn query(filename: &str, bytes: &Bytes) -> anyhow::Result<Vec<QueryResult>> {
    let source = MediaSource::Bytes(filename, bytes);

    emycloud_client_rs::query(source, MIN_CONFIDENCE)
        .await
        .context("EmySound::query")?
        .iter()
        .map(|result| result.try_into())
        .inspect(|result| log::debug!("{result:?}"))
        .collect()
}
