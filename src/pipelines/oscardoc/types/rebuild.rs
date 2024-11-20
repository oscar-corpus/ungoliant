/*! Rebuild file writer/schema
Each (avro) record is an  `(shard_id, array of (shard) records)`.

# Rebuild files

Each lang has its avro file.
Each record corresponds to a shard, and contains a list of "slimmed" documents.

Those slim documents contain:
- language identification related metadata,
- record id,
- line start/end for each WARC Record. Note that `line_start and line_end` are _included_,
so a document that has `(line_start, line_end) == (10, 10)` has a single line that is at offset 10.

!*/

use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
};

use avro_rs::{AvroResult, Codec, Schema, Writer};
use log::error;
use oxilangtag::LanguageTag;
use serde::Deserialize;
use serde::Serialize;
use structopt::lazy_static::lazy_static;

use crate::error::Error;

use crate::pipelines::oscardoc::types::{Location, Metadata};

lazy_static! {
    static ref SCHEMA: Schema = {

      // schema of Identification struct
        let identification_schema = r#"
      {"name":"identification", "type":"record", "fields": [
        {"name": "label", "type":"string"},
        {"name": "prob", "type":"float"}
      ]}
"#;
      // schema of Metadata struct
        let metadata_schema = r#"
{
  "type":"record",
  "name":"metadata_record",
  "fields":[
    {"name":"identification", "type":"identification"},
    {"name":"harmful_pp", "type":["null", "float"]},
    {"name":"tlsh", "type":["null", "string"]},
    {"name":"quality_warnings", "type":["null", {"type": "array", "items":"string"}]},
    {"name":"categories", "type":["null", {"type": "array", "items":"string"}]},
    {"name": "sentence_identifications", "type":"array", "items":[
      "null",
      "identification"
    ]}
  ]
}
"#;
  // schema of RebuildInformation struct
        let rebuild_schema = r#"
{
  "type":"record",
  "name":"rebuild_information",
  "fields":[
    {"name": "shard_id", "type":"long"},
    {"name": "record_id", "type":"string"},
    {"name": "line_start", "type":"long"},
    {"name": "line_end", "type":"long"},
    {"name": "loc_in_shard", "type":"long"},
    {"name":"metadata", "type":"metadata_record"}
  ]
}
"#;
  // schema of ShardResult struct
        let schema = r#"
{
  "type":"record",
  "name":"shard_result",
  "fields":[
    {"name": "shard_id", "type":"long"},
    {"name": "rebuild_info", "type":"array", "items":"rebuild_information"}
  ]
}
"#;

        Schema::parse_list(&[
            identification_schema,
            metadata_schema,
            rebuild_schema,
            schema,
        ])
        .unwrap()[3]
            .clone()
    };
}

/// Holds the same fields as [Location], adding [Metadata].
///
/// Should be transformed into a struct that holds two attributes rather than copying some.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct RebuildInformation {
    shard_id: usize,
    record_id: String,
    line_start: usize,
    line_end: usize,
    loc_in_shard: usize,
    metadata: Metadata,
}

impl RebuildInformation {
    pub fn new(location: Location, metadata: Metadata) -> Self {
        Self {
            shard_id: location.shard_id(),
            // TODO: Useless borrow here.
            record_id: location.record_id().to_owned(),
            line_start: location.line_start(),
            line_end: location.line_end(),
            loc_in_shard: location.loc_in_shard(),
            metadata,
        }
    }

    /// Convert into a ([Location], [Metadata]) tuple.
    pub fn into_raw_parts(self) -> (Location, Metadata) {
        (
            Location::new(
                self.shard_id,
                self.record_id,
                self.line_start,
                self.line_end,
                self.loc_in_shard,
            ),
            self.metadata,
        )
    }
    /// Get a reference to the rebuild information's loc in shard.
    pub fn loc_in_shard(&self) -> usize {
        self.loc_in_shard
    }

    /// Get a reference to the rebuild information's record id.
    pub fn record_id(&self) -> &str {
        self.record_id.as_ref()
    }

    /// Get a reference to the rebuild information's line start.
    pub fn line_start(&self) -> usize {
        self.line_start
    }

    /// Get a reference to the rebuild information's line end.
    pub fn line_end(&self) -> usize {
        self.line_end
    }

    /// Get a reference to the rebuild information's metadata.
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    /// Get a reference to the rebuild information's shard id.
    pub fn shard_id(&self) -> usize {
        self.shard_id
    }
}

/// Holds multiple [RebuildInformation] for a single shard.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct ShardResult {
    shard_id: i64,
    rebuild_info: Vec<RebuildInformation>,
}

impl ShardResult {
    /// Merges `locations` and `metadata` into [RebuildInformation].
    pub fn new(shard_id: i64, locations: Vec<Location>, metadata: Vec<Metadata>) -> Self {
        let rebuild_info = locations
            .into_iter()
            .zip(metadata)
            .map(|(loc, meta)| RebuildInformation::new(loc, meta))
            .collect();
        Self {
            shard_id,
            rebuild_info,
        }
    }

    /// order by location in shard.
    /// This destroys the order of document, but is necessary for the rebuilding process to be efficient.
    #[inline]
    pub fn sort(&mut self) {
        self.rebuild_info
            .sort_unstable_by(|a, b| a.loc_in_shard.cmp(&b.loc_in_shard))
    }

    /// extract owned parts of struct: (`shard_id`, `Vec<RebuildInformation>`)
    pub fn into_raw_parts(self) -> (i64, Vec<RebuildInformation>) {
        (self.shard_id, self.rebuild_info)
    }
    /// Get a reference to the shard result's shard id.
    pub fn shard_id(&self) -> i64 {
        self.shard_id
    }

    /// Get a reference to the shard result's rebuild info.
    pub fn rebuild_info(&self) -> &[RebuildInformation] {
        self.rebuild_info.as_ref()
    }
}
/// Holds an Avro writer.
pub struct RebuildWriter<'a, T> {
    schema: &'a Schema,
    writer: Writer<'a, T>,
}

impl<'a, T: std::io::Write> RebuildWriter<'a, T> {
    /// Create a new rebuilder.
    pub fn new(schema: &'a Schema, writer: T) -> Self {
        Self {
            schema,
            writer: Writer::with_codec(schema, writer, Codec::Snappy),
        }
    }

    /// Append a single serializable value (`value` must implement [Serialize]).
    ///
    /// This function is not guaranteed to perform a write operation
    /// See documentation of [avro_rs::Writer] for more information.
    pub fn append_ser<S: Serialize>(&mut self, value: S) -> AvroResult<usize> {
        self.writer.append_ser(value)
    }

    /// Append from an interator of values, each implementing [Serialize].
    ///
    /// This function is not guaranteed to perform a write operation
    /// See documentation of [avro_rs::Writer] for more information.
    pub fn extend_ser<I, U: Serialize>(&mut self, values: I) -> AvroResult<usize>
    where
        I: IntoIterator<Item = U>,
    {
        self.writer.extend_ser(values)
    }

    /// Flush the underlying buffer.
    ///
    /// See [avro_rs::Writer] for more information.
    pub fn flush(&mut self) -> AvroResult<usize> {
        self.writer.flush()
    }
}

impl<'a> RebuildWriter<'a, File> {
    /// Create a writer on `dst` file.
    /// Errors if provided path already exists.
    pub fn from_path(dst: &Path) -> Result<Self, Error> {
        let schema = &SCHEMA;
        let dest_file = File::create(dst)?;
        Ok(Self::new(schema, dest_file))
    }
}

/// Holds mutex-protected [RebuildWriter] for each [Lang].
// pub struct RebuildWriters<'a, T>(HashMap<LanguageTag<String>, Arc<Mutex<RebuildWriter<'a, T>>>>);
pub struct RebuildWriters<'a, T> {
    inner: Arc<RwLock<HashMap<LanguageTag<String>, Arc<Mutex<RebuildWriter<'a, T>>>>>>,
}

impl<'a, T> RebuildWriters<'a, T> {
    pub fn writers(
        &'a self,
    ) -> std::sync::RwLockReadGuard<HashMap<LanguageTag<String>, Arc<Mutex<RebuildWriter<T>>>>>
    {
        self.inner.read().unwrap()
    }

    pub fn contains(&'a self, k: &LanguageTag<String>) -> bool {
        let r_lock = self.inner.read().unwrap();
        r_lock.contains_key(k)
    }
}

impl<'a> RebuildWriters<'a, File> {
    #[inline]
    fn forge_dst(dst: &Path, lang: &LanguageTag<String>) -> PathBuf {
        let mut p = PathBuf::from(dst);
        p.push(format!("{}.avro", lang.as_str()));

        p
    }

    pub fn insert(&'a self, root_dir: &Path, k: &LanguageTag<String>) -> Result<(), Error> {
        let mut wlock = self.inner.write().unwrap();
        let (lang, new_writer) = Self::new_writer_mutex(root_dir, k.clone())?;
        wlock.entry(lang).or_insert(new_writer);
        Ok(())
    }

    #[inline]
    /// Convinience function that creates a new ([Lang], `Arc<Mutex<RebuildWriter>>`]) pair.
    fn new_writer_mutex(
        dst: &Path,
        lang: LanguageTag<String>,
    ) -> Result<(LanguageTag<String>, Arc<Mutex<RebuildWriter<'a, File>>>), Error> {
        // let lang = Lang::from_str(lang).unwrap();
        let path = Self::forge_dst(dst, &lang);
        let rw = RebuildWriter::from_path(&path)?;
        let rw_mutex = Arc::new(Mutex::new(rw));
        Ok((lang, rw_mutex))
    }

    /// Use `dst` as a root path for avro files storage.
    ///
    /// Each language will have a possibly empty avro file, at `<dst>/<lang>.avro`.
    pub fn with_dst(dst: &Path) -> Result<Self, Error> {
        if !dst.exists() {
            std::fs::create_dir(dst)?;
        }
        if dst.is_file() {
            error!("rebuild destination must be an empty folder!");
        };

        if dst.read_dir()?.next().is_some() {
            error!("rebuild destination folder must be empty!");
        }

        Ok(RebuildWriters {
            inner: Arc::new(RwLock::new(HashMap::new())),
        })
    }
}

#[cfg(test)]
mod tests {

    use std::{
        collections::HashMap,
        fs::File,
        sync::{Arc, RwLock},
    };

    use oxilangtag::LanguageTag;

    use crate::pipelines::oscardoc::types::{Location, Metadata};

    use super::{RebuildInformation, RebuildWriter, RebuildWriters, ShardResult};

    #[test]
    fn rebuild_information_into_raw_parts() {
        let loc = Location::default();
        let m = Metadata::default();
        let ri = RebuildInformation::new(loc.clone(), m.clone());
        let (loc2, m2) = ri.into_raw_parts();

        assert_eq!(loc, loc2);
        assert_eq!(m, m2);
    }
    #[test]
    fn test_ser_empty() {
        let sr = ShardResult::new(0, Vec::new(), Vec::new());
        println!("{:#?}", sr);
        let buf = Vec::new();
        let mut rw = RebuildWriter::new(&super::SCHEMA, buf);

        rw.append_ser(sr).unwrap();
    }

    #[test]
    fn test_sort() {
        let record_ids = ["record1", "record2", "record3"];
        let locs_in_shard: [usize; 3] = [3, 0, 4];

        let mut locs = Vec::with_capacity(record_ids.len());
        for (loc, id) in locs_in_shard.into_iter().zip(record_ids) {
            let loc = Location::new(1, id.to_string(), 0, 10, loc);
            locs.push(loc);
        }
        let metas = vec![Metadata::default(); 3];

        let mut sr = ShardResult::new(1, locs, metas);

        // unsorted, will be sorted manually
        let mut locs_unsorted: Vec<_> = sr
            .rebuild_info()
            .iter()
            .map(|rb| rb.loc_in_shard())
            .collect();

        // call sorting
        sr.sort();

        // these ones should be sorted now
        let locs_sorted: Vec<_> = sr
            .rebuild_info()
            .iter()
            .map(|rb| rb.loc_in_shard())
            .collect();

        // ensure inequality before sorting, in case of empty or 1 sized vectors
        assert_ne!(locs_unsorted, locs_sorted);

        // sort manually, ground truth
        locs_unsorted.sort();

        assert_eq!(locs_unsorted, locs_sorted);
    }

    #[test]
    fn test_into_raw_parts() {
        let record_ids = ["record1", "record2", "record3"];
        let locs_in_shard: [usize; 3] = [3, 0, 4];

        let mut locs = Vec::with_capacity(record_ids.len());
        for (loc, id) in locs_in_shard.into_iter().zip(record_ids) {
            let loc = Location::new(1, id.to_string(), 0, 10, loc);
            locs.push(loc);
        }
        let metas = vec![Metadata::default(); 3];

        let mut sr = ShardResult::new(1, locs, metas);

        let (shard_id, rebuild_info) = sr.clone().into_raw_parts();

        let from_raw_parts = ShardResult {
            shard_id,
            rebuild_info,
        };

        assert_eq!(sr, from_raw_parts)
    }

    #[test]
    fn test_ser() {
        let meta = vec![Metadata::default()];
        let loc = vec![Location::default()];
        let sr = ShardResult::new(0, loc, meta);
        println!("{:#?}", sr);
        println!("{:#?}", *super::SCHEMA);
        let mut buf = Vec::new();
        let mut rw = RebuildWriter::new(&super::SCHEMA, &mut buf);

        rw.append_ser(&sr).unwrap();
        rw.flush().unwrap();

        let ar = avro_rs::Reader::with_schema(&super::SCHEMA, &buf[..]).unwrap();
        let result: Vec<ShardResult> = ar
            .map(|r| avro_rs::from_value::<ShardResult>(&r.unwrap()).unwrap())
            .collect();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], sr);
    }

    #[test]
    fn test_rebuild_writers_contains() {
        let rbw = RebuildWriters::<usize> {
            inner: Arc::new(RwLock::new(HashMap::new())),
        };

        assert!(!rbw.contains(&LanguageTag::parse("fr".to_string()).unwrap()));

        // ensure no panic here
        rbw.writers();
    }

    #[test]
    fn test_rebuild_writers_insert() {
        let rbw = RebuildWriters::<File> {
            inner: Arc::new(RwLock::new(HashMap::new())),
        };

        let lang = LanguageTag::parse("fr".to_string()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let rbw = RebuildWriters::with_dst(dir.path()).unwrap();
        assert!(!rbw.contains(&lang));
        rbw.insert(dir.path(), &lang).unwrap();
        assert!(rbw.contains(&lang));
    }
}
