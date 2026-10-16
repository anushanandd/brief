//! Businesses from Overture Maps' places (https://docs.overturemaps.org/guides/places/), read
//! straight from its public GeoParquet release over HTTP: the release catalog names the files
//! and their extents, and of each file meeting an area only the footer, then the row groups
//! whose bounds meet the area, then only the columns Brief keeps, are downloaded. Only the
//! area's coordinates leave the Mac, never a merchant's name.
//!
//! The data comes from Meta, Microsoft, Foursquare, AllThePlaces, and others, under
//! CDLA-Permissive-2.0, Apache-2.0, and CC0-1.0, credited in Settings.
use std::{collections::BTreeSet, io::Cursor, sync::Arc};

use bytes::Bytes;
use parquet::{
    errors::ParquetError,
    file::{
        metadata::{ParquetMetaDataReader, RowGroupMetaData},
        reader::{ChunkReader, FileReader, Length},
        serialized_reader::{ReadOptionsBuilder, SerializedFileReader},
        statistics::Statistics,
    },
    record::{Field, Row},
    schema::types::{Type, TypePtr},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const CATALOG: &str = "https://stac.overturemaps.org";

/// A box in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

impl Area {
    pub fn around(lat: f64, lon: f64, half: f64) -> Self {
        Self {
            west: lon - half,
            south: lat - half,
            east: lon + half,
            north: lat + half,
        }
    }

    fn meets(&self, other: &Area) -> bool {
        self.west <= other.east
            && other.west <= self.east
            && self.south <= other.north
            && other.south <= self.north
    }

    fn contains(&self, lat: f64, lon: f64) -> bool {
        (self.south..=self.north).contains(&lat) && (self.west..=self.east).contains(&lon)
    }
}

/// One business or landmark as Brief keeps it.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OverturePlace {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// The chain it belongs to, when Overture knows one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    /// How likely the place exists, from 0 to 1.
    pub confidence: f64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub closed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locality: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub postcode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
    pub lat: f64,
    pub lon: f64,
}

/// A monthly release and the files of its places.
pub struct Release {
    pub id: String,
    files: Vec<(Area, String)>,
}

/// The leaf columns Brief reads; nothing else is downloaded.
const COLUMNS: [&str; 15] = [
    "id",
    "names.primary",
    "confidence",
    "basic_category",
    "brand.names.primary",
    "operating_status",
    "phones.list.element",
    "websites.list.element",
    "addresses.list.element.freeform",
    "addresses.list.element.locality",
    "addresses.list.element.postcode",
    "bbox.xmin",
    "bbox.xmax",
    "bbox.ymin",
    "bbox.ymax",
];

/// Row groups this close together are fetched in one request.
const MERGE_GAP: u64 = 256 * 1024;

fn unavailable(_: impl std::fmt::Debug) -> String {
    "Overture Maps couldn't be reached".into()
}

async fn json(client: &reqwest::Client, url: &str) -> Result<Value, String> {
    client
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(unavailable)?
        .json()
        .await
        .map_err(unavailable)
}

fn area(value: &Value) -> Option<Area> {
    let numbers = value
        .as_array()?
        .iter()
        .map(Value::as_f64)
        .collect::<Option<Vec<_>>>()?;
    let [west, south, east, north] = numbers[..] else {
        return None;
    };
    Some(Area {
        west,
        south,
        east,
        north,
    })
}

/// The latest release of places, from Overture's catalog.
pub async fn latest(client: &reqwest::Client) -> Result<Release, String> {
    let catalog = json(client, &format!("{CATALOG}/catalog.json")).await?;
    let id = catalog["latest"]
        .as_str()
        .filter(|id| {
            id.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
        })
        .ok_or("Overture's catalog has no current release")?
        .to_owned();
    let collection = json(
        client,
        &format!("{CATALOG}/{id}/places/place/collection.json"),
    )
    .await?;
    let items = collection["links"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|link| link["rel"] == "item")
        .filter_map(|link| link["href"].as_str())
        .collect::<Vec<_>>();
    let mut files = Vec::new();
    for href in items {
        if !href.starts_with(CATALOG) {
            continue;
        }
        let item = json(client, href).await?;
        let (Some(extent), Some(url)) =
            (area(&item["bbox"]), item["assets"]["aws"]["href"].as_str())
        else {
            continue;
        };
        if url.starts_with("https://overturemaps-us-west-2.s3.us-west-2.amazonaws.com/") {
            files.push((extent, url.to_owned()));
        }
    }
    if files.is_empty() {
        return Err("Overture's catalog lists no places".into());
    }
    Ok(Release { id, files })
}

/// The bytes of a file Brief fetched, at their offsets; reading elsewhere is an error.
struct Fetched {
    length: u64,
    ranges: Vec<(u64, Bytes)>,
}

impl Fetched {
    fn slice(&self, start: u64, length: usize) -> parquet::errors::Result<Bytes> {
        self.ranges
            .iter()
            .find(|(offset, bytes)| {
                start >= *offset && start + length as u64 <= offset + bytes.len() as u64
            })
            .map(|(offset, bytes)| {
                let from = (start - offset) as usize;
                bytes.slice(from..from + length)
            })
            .ok_or_else(|| ParquetError::General("Read outside the fetched ranges".into()))
    }
}

impl Length for Fetched {
    fn len(&self) -> u64 {
        self.length
    }
}

impl ChunkReader for Fetched {
    type T = Cursor<Bytes>;

    fn get_read(&self, start: u64) -> parquet::errors::Result<Self::T> {
        let (offset, bytes) = self
            .ranges
            .iter()
            .find(|(offset, bytes)| start >= *offset && start < offset + bytes.len() as u64)
            .ok_or_else(|| ParquetError::General("Read outside the fetched ranges".into()))?;
        Ok(Cursor::new(bytes.slice((start - offset) as usize..)))
    }

    fn get_bytes(&self, start: u64, length: usize) -> parquet::errors::Result<Bytes> {
        self.slice(start, length)
    }
}

/// `bytes=from-to` of `url`, or its last `-n` bytes, with the file's length. A dropped
/// connection is retried, since a long read over a home network sometimes is.
async fn range(client: &reqwest::Client, url: &str, spec: &str) -> Result<(Bytes, u64), String> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        let read = async {
            let response = client
                .get(url)
                .header(reqwest::header::RANGE, format!("bytes={spec}"))
                .timeout(std::time::Duration::from_secs(120))
                .send()
                .await?
                .error_for_status()?;
            let length = response
                .headers()
                .get(reqwest::header::CONTENT_RANGE)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.rsplit('/').next()?.parse::<u64>().ok());
            Ok::<_, reqwest::Error>((response.bytes().await?, length))
        };
        match read.await {
            Ok((bytes, Some(length))) => return Ok((bytes, length)),
            Ok((_, None)) => return Err("Overture's file server doesn't serve ranges".into()),
            Err(error) if attempt >= 3 || error.is_status() => return Err(unavailable(error)),
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(500 * attempt)).await,
        }
    }
}

fn bound(row_group: &RowGroupMetaData, column: &str, max: bool) -> Option<f64> {
    let statistics = row_group
        .columns()
        .iter()
        .find(|chunk| chunk.column_path().string() == column)?
        .statistics()?;
    match statistics {
        Statistics::Float(values) if max => values.max_opt().map(|value| f64::from(*value)),
        Statistics::Float(values) => values.min_opt().map(|value| f64::from(*value)),
        Statistics::Double(values) if max => values.max_opt().copied(),
        Statistics::Double(values) => values.min_opt().copied(),
        _ => None,
    }
}

/// The row group's extent, from its column statistics; unknown counts as meeting the area.
fn meets(row_group: &RowGroupMetaData, area: &Area) -> bool {
    let extent = (|| {
        Some(Area {
            west: bound(row_group, "bbox.xmin", false)?,
            east: bound(row_group, "bbox.xmax", true)?,
            south: bound(row_group, "bbox.ymin", false)?,
            north: bound(row_group, "bbox.ymax", true)?,
        })
    })();
    extent.is_none_or(|extent| extent.meets(area))
}

/// The file schema with only the fields leading to `COLUMNS`.
fn projection(schema: &TypePtr, path: &[&str]) -> Option<TypePtr> {
    if !schema.is_group() {
        let leaf = path.join(".");
        return COLUMNS.contains(&leaf.as_str()).then(|| schema.clone());
    }
    let fields = schema
        .get_fields()
        .iter()
        .filter_map(|field| {
            let mut child = path.to_vec();
            child.push(field.name());
            projection(field, &child)
        })
        .collect::<Vec<_>>();
    if fields.is_empty() {
        return None;
    }
    let info = schema.get_basic_info();
    let mut builder = Type::group_type_builder(info.name())
        .with_fields(fields)
        .with_converted_type(info.converted_type())
        .with_logical_type(info.logical_type_ref().cloned());
    if info.has_repetition() {
        builder = builder.with_repetition(info.repetition());
    }
    if info.has_id() {
        builder = builder.with_id(Some(info.id()));
    }
    builder.build().ok().map(Arc::new)
}

fn field<'a>(row: &'a Row, name: &str) -> Option<&'a Field> {
    row.get_column_iter()
        .find(|(field, _)| field.as_str() == name)
        .map(|(_, value)| value)
}

fn text(field: Option<&Field>) -> Option<String> {
    match field? {
        Field::Str(value) => Some(value.trim().to_owned()).filter(|value| !value.is_empty()),
        _ => None,
    }
}

fn number(field: Option<&Field>) -> Option<f64> {
    match field? {
        Field::Float(value) => Some(f64::from(*value)),
        Field::Double(value) => Some(*value),
        _ => None,
    }
}

fn group<'a>(row: &'a Row, name: &str) -> Option<&'a Row> {
    match field(row, name)? {
        Field::Group(group) => Some(group),
        _ => None,
    }
}

fn first<'a>(row: &'a Row, name: &str) -> Option<&'a Field> {
    match field(row, name)? {
        Field::ListInternal(list) => list.elements().first(),
        _ => None,
    }
}

fn place(row: &Row) -> Option<OverturePlace> {
    let bbox = group(row, "bbox")?;
    let lat = (number(field(bbox, "ymin"))? + number(field(bbox, "ymax"))?) / 2.0;
    let lon = (number(field(bbox, "xmin"))? + number(field(bbox, "xmax"))?) / 2.0;
    let address = match first(row, "addresses") {
        Some(Field::Group(address)) => Some(address),
        _ => None,
    };
    let in_address = |name: &str| address.and_then(|address| text(field(address, name)));
    Some(OverturePlace {
        id: text(field(row, "id"))?,
        name: group(row, "names").and_then(|names| text(field(names, "primary")))?,
        category: text(field(row, "basic_category")),
        brand: group(row, "brand")
            .and_then(|brand| group(brand, "names"))
            .and_then(|names| text(field(names, "primary"))),
        confidence: number(field(row, "confidence")).unwrap_or(0.0),
        closed: text(field(row, "operating_status")).as_deref() == Some("permanently_closed"),
        address: in_address("freeform"),
        locality: in_address("locality"),
        postcode: in_address("postcode"),
        phone: text(first(row, "phones")),
        website: text(first(row, "websites")),
        lat,
        lon,
    })
}

/// The places in `area` from one file: its footer, then the column chunks Brief reads of the
/// row groups that meet the area.
async fn read_file(
    client: &reqwest::Client,
    url: &str,
    area: Area,
) -> Result<Vec<OverturePlace>, String> {
    let (tail, length) = range(client, url, "-65536").await?;
    if tail.len() < 8 || &tail[tail.len() - 4..] != b"PAR1" {
        return Err("An Overture file isn't Parquet".into());
    }
    let footer = u32::from_le_bytes(
        tail[tail.len() - 8..tail.len() - 4]
            .try_into()
            .unwrap_or_default(),
    ) as u64;
    let footer_start = length - footer - 8;
    let footer_bytes = if footer + 8 <= tail.len() as u64 {
        tail.slice(tail.len() - footer as usize - 8..)
    } else {
        range(client, url, &format!("{footer_start}-{}", length - 1))
            .await?
            .0
    };
    let metadata = ParquetMetaDataReader::decode_metadata(&footer_bytes[..footer as usize])
        .map_err(|_| "An Overture file's footer can't be read")?;
    let selected = metadata
        .row_groups()
        .iter()
        .enumerate()
        .filter(|(_, row_group)| meets(row_group, &area))
        .map(|(index, _)| index)
        .collect::<BTreeSet<_>>();
    if selected.is_empty() {
        return Ok(Vec::new());
    }
    let mut spans = selected
        .iter()
        .flat_map(|&index| metadata.row_group(index).columns())
        .filter(|chunk| COLUMNS.contains(&chunk.column_path().string().as_str()))
        .map(|chunk| {
            let (start, length) = chunk.byte_range();
            (start, start + length)
        })
        .collect::<Vec<_>>();
    spans.sort_unstable();
    let mut merged = Vec::<(u64, u64)>::new();
    for (start, end) in spans {
        match merged.last_mut() {
            Some(last) if start <= last.1 + MERGE_GAP => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    let mut ranges = vec![(footer_start, footer_bytes)];
    for (start, end) in merged {
        let (bytes, _) = range(client, url, &format!("{start}-{}", end - 1)).await?;
        ranges.push((start, bytes));
    }
    let fetched = Fetched { length, ranges };
    tokio::task::spawn_blocking(move || {
        let options = ReadOptionsBuilder::new()
            .with_predicate(Box::new(move |_, index| selected.contains(&index)))
            .build();
        let reader = SerializedFileReader::new_with_options(fetched, options)
            .map_err(|_| "An Overture file can't be read")?;
        let schema = Arc::new(reader.metadata().file_metadata().schema().clone());
        let projected = projection(&schema, &[]).ok_or("An Overture file has no places")?;
        let rows = reader
            .get_row_iter(Some(projected.as_ref().clone()))
            .map_err(|_| "An Overture file can't be read")?;
        let mut places = Vec::new();
        for row in rows {
            let row = row.map_err(|_| "An Overture file can't be read")?;
            if let Some(place) = place(&row).filter(|place| area.contains(place.lat, place.lon)) {
                places.push(place);
            }
        }
        Ok(places)
    })
    .await
    .map_err(|_| "Reading Overture's places stopped".to_owned())?
}

/// Every place in `area` in this release.
pub async fn read(
    client: &reqwest::Client,
    release: &Release,
    area: Area,
) -> Result<Vec<OverturePlace>, String> {
    let mut places = Vec::new();
    for (extent, url) in &release.files {
        if extent.meets(&area) {
            places.extend(read_file(client, url, area).await?);
        }
    }
    Ok(places)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads a real area from the current release. Run on demand: it needs the network.
    #[tokio::test]
    #[ignore]
    async fn reads_an_area_from_the_current_release() {
        let client = reqwest::Client::builder()
            .user_agent("Brief/0.1")
            .build()
            .unwrap();
        let release = latest(&client).await.unwrap();
        let started = std::time::Instant::now();
        let places = read(&client, &release, Area::around(47.37, 8.54, 0.01))
            .await
            .unwrap();
        eprintln!(
            "{} places in {:?} from release {}",
            places.len(),
            started.elapsed(),
            release.id
        );
        assert!(places.len() > 100);
        assert!(places.iter().all(|place| !place.name.is_empty()));
    }
}
