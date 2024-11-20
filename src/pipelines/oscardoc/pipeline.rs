//! OSCAR Schema v2 generation pipeline
//!
//! OSCAR Schema v2 is a document-oriented corpus schema
//! enhanced with metadata coming from CommonCrawl.
//!
//! The CommonCrawl dump is composed of shards,
//! Each shard is composed of records,
//! Each record is composed of a metadata header and a body containing sentences.
//!
//! # Processing
//! 1. Each record passes through a quality filter that by default checks the content distribution between
//!   short and long sentences, discarding records where the content is primarly in short sentences. (sentence = newline-separated string)
//! 1. The remaining ones get identified both by line and as a whole (we keep the language that has the most information (=bytes)).
//! 1. We pass the records in the adult content annotator
//! 1. We remove remaining short sentences at start/end[^1]
//! 1. We then write documents in files.
//!
//! [^1]: We should do this after step 1: better efficiency.
use std::fs::File;
use std::path::Path;

use std::{collections::HashMap, path::PathBuf};

use crate::error::Error;
use crate::filtering::{record, Filter};
use crate::identifiers::identification::Identification;
use crate::identifiers::model::{FastText, FastTextBuilder, Predict};
use crate::identifiers::StrictMultilingual;
use crate::pipelines::oscardoc::types::Location;
use crate::pipelines::oscardoc::types::RebuildWriters;
use oscar_io::v3::{Document, Metadata, WriterTrait};

use crate::pipelines::oscardoc::types::{LocationBuilder, ShardResult};
use crate::pipelines::pipeline::Pipeline;
use crate::sources::commoncrawl::Wet;

use crate::transformers::{
    self, Annotate, Annotator, ContentDetector, Header, Noisy, ShortSentences, TinyDocument,
    Transform, LSH,
};
#[cfg(feature = "kenlm")]
use crate::transformers::{AdultDetector, AdultDetectorBuilder, Models};
use log::{debug, error, info, log_enabled, warn};
use oxilangtag::LanguageTag;
use rayon::prelude::*;
use ut1_blocklist::MultipleBlocklist;
use warc::BufferedBody;
use warc::{Record, WarcHeader};

use crate::io::LangFilesDoc;

const DOC_THRESHOLD: f32 = 0.6f32;

// TODO: Implement structopt directly here.
pub struct OscarDoc {
    src: PathBuf,
    dst: PathBuf,
    lid_path: PathBuf,
    blocklist: Option<PathBuf>,
    kenlms_path: Option<PathBuf>,
}

impl OscarDoc {
    pub fn new(
        src: PathBuf,
        dst: PathBuf,
        lid_path: PathBuf,
        blocklist: Option<PathBuf>,
        kenlms_path: Option<PathBuf>,
    ) -> Self {
        if blocklist.is_none() {
            warn!("No blocklist folder specified! No adult content tagging will be done.");
        }

        debug!("using blocklist {:?}", blocklist);
        Self {
            src,
            dst,
            lid_path,
            blocklist,
            kenlms_path,
        }
    }

    /// list files in source folder,
    /// filter out errors from fs and from gzip/wet.
    ///
    /// This means that invalid gz files and invalid
    /// wet files are discarded silently
    fn get_paths_iter(&self) -> Result<impl Iterator<Item = PathBuf>, Error> {
        let results = std::fs::read_dir(&self.src)?
            .filter_map(|shard| {
                shard.map_or_else(
                    |e| {
                        error!("error reading shard directory: {}", e);
                        None
                    },
                    Some,
                )
            })
            .map(|shard| shard.path());
        Ok(results)
    }

    /// Extract shard number from a CC shard path.
    fn get_shard_number(shard_path: &Path) -> Result<usize, Error> {
        let shard_number = shard_path.file_stem();
        let shard_number = shard_number
            .and_then(|s| s.to_str())
            .and_then(|s| s.split('.').next())
            .map(|s| s.parse::<usize>());

        match shard_number {
            Some(Ok(sn)) => Ok(sn),
            Some(Err(e)) => Err(Error::Custom(format!("{:?}", e))),
            None => Err(Error::Custom(format!(
                "Couldn't extract shard number from {:?}",
                shard_path
            ))),
        }
    }

    /// Process a shard.
    ///
    /// This opens the shard, filters/identifies all documents and then
    /// returns the shard id, along with a [Vec] of documents and their relative location (for rebuilding)
    fn process_shard(
        shard_path: &Path,
        identifier: &FastText,
        filter: Option<record::FilterKind>,
        annotator: &Annotator<Document>,
    ) -> Result<(usize, Vec<(Document, Location)>), Error> {
        info!("working on shard: {:?}", shard_path);

        // get shard number
        let shard_id = Self::get_shard_number(shard_path)?;

        let shard = Wet::from_path_gzip(shard_path)?;
        let record_iter = shard.iter.enumerate().par_bridge();

        // only get valid records, print errors
        let record_iter = record_iter.filter_map(|(idx, record)| match record {
            Ok(r) => Some((idx, r)),
            Err(e) => {
                error!("{:?}", e);
                None
            }
        });

        // begin creation of location
        // We fill what we can fill now: shard_id, location_in_shard and record_id.
        let record_iter = record_iter.map(|(idx, record)| {
            let mut loc = LocationBuilder::default();
            loc.set_shard_id(shard_id);
            loc.set_loc_in_shard(idx);
            loc.set_record_id(record.warc_id().to_string());

            (loc, record)
        });

        // remove short sentences, discarding documents that only have short sentences
        let length_filter = transformers::RemoveShortSentences::default();
        let record_iter = record_iter.filter_map(|(mut loc, mut record)| {
            let bounds = length_filter.transform(&mut record);
            match bounds.len() {
                0 => {
                    debug!("record {} has no sentences kept", record.warc_id());
                    None
                }
                1 => {
                    loc.set_line_start(*bounds[0].start());
                    loc.set_line_end(*bounds[0].end());
                    Some((loc, record))
                }
                _ => {
                    warn!(
                        "record {} has more than one chunk of sentences kept",
                        record.warc_id()
                    );
                    loc.set_line_start(*bounds[0].start());
                    loc.set_line_end(*bounds[0].end());
                    Some((loc, record))
                }
            }
        });

        // get specified filter or resort to default filter kind
        let f = filter.unwrap_or_default();

        // get iterator on filtered records.
        // only get records that are valid *and* pass the filter.
        let record_iter = record_iter.filter_map(|(idx, record)| {
            if f.detect(&record) {
                Some((idx, record))
            } else {
                None
            }
        });

        // identify
        let record_iter = record_iter
            .map(|(loc, record)| (loc, Self::process_record(record, identifier)))
            .filter_map(|(loc, res)| match res {
                Ok(Some(res)) => Some((loc, res)),
                Ok(None) => None,
                Err(e) => {
                    error!("{:?}", e);
                    None
                }
            });

        // annotate
        let record_iter = record_iter.map(|(loc, mut r)| {
            annotator.annotate(&mut r);
            (r, loc.build().unwrap())
        });

        // remove documents that are both tiny and noisy
        let record_iter = record_iter.filter_map(|(r, loc): (Document, Location)| {
            if r.metadata().annotation() == Some(&vec!["noisy".to_string(), "tiny".to_string()]) {
                debug!("removed document {:?} for noisy+tiny", r.warc_id());
                None
            } else {
                Some((r, loc))
            }
        });

        let records: Vec<(_, _)> = record_iter.collect();
        info!("Shard {}: Got {} documents", shard_id, records.len());

        Ok((shard_id, records))
    }

    /// process a record
    /// identify each line of the document
    /// then compute the most present identification
    fn process_record(
        record: Record<BufferedBody>,
        identifier: &FastText,
    ) -> Result<Option<Document>, Error> {
        // get lines
        let (headers, body) = record.into_raw_parts();
        let body = String::from_utf8_lossy(&body);
        let lines = body.lines();

        // get the id for each line, the byte/prob count and the total byte count of the document
        let w_ids = identifier.weighted_ids(lines)?;
        let ids = w_ids.line_ids();
        let lang_count = w_ids.lang_bins();
        let total_count = w_ids.total_size();

        //TODO fix multilingual
        // see if the record meets multilingual criteria
        let multilingual = StrictMultilingual::default().detect(ids);

        let ids: Vec<_> = ids
            .iter()
            .map(|id| id.clone().map(|_id| _id.into_inner()))
            .collect();

        if multilingual {
            //TODO: fix prob on multilingual documents
            let document_identification =
                Identification::new(LanguageTag::parse("multi".to_string())?, 0.5);

            let metadata = Metadata::new(&document_identification, ids.as_slice());
            let doc = Document::new(body.into_owned(), headers.headers, metadata);

            return Ok(Some(doc));
        }

        // figure out document language
        // count bytes per language, get language that got most bytes
        let document_language = lang_count.iter().max_by_key(|(_, (v, _))| *v);

        // build a document and return it if the document language is not the unknown one.
        if let Some((Some(id), (lang_byte_count, confidence))) = document_language {
            // build an Identification with prob = number of bytes from most identified language / total number of bytes
            debug!(
                "{:?}: {}/{} (c:{})",
                id, lang_byte_count, total_count, confidence
            );

            if confidence < &DOC_THRESHOLD {
                return Ok(None);
            }

            // create id
            let document_identification = Identification::new(id.clone(), *confidence);

            // create doc and metadata
            let metadata = Metadata::new(&document_identification, ids.as_slice());
            let doc = Document::new(body.into_owned(), headers.headers, metadata);

            debug!("{} : {:?}", doc.warc_id(), doc.identification());
            Ok(Some(doc))
        } else {
            if log_enabled!(log::Level::Debug) {
                debug!(
                    "{:?} : NONE",
                    headers
                        .headers
                        .get(&WarcHeader::RecordID)
                        .map(|x| Some(String::from_utf8_lossy(x)))
                );
                debug!("{:?}", &lang_count);
                debug!("{}", &body);
            }
            Ok(None)
        }
    }

    /// Gets a vector of documents and outputs a hashmap listing the documents per language
    fn sort_by_lang(
        documents: Vec<(Document, Location)>,
    ) -> HashMap<LanguageTag<String>, Vec<(Document, Location)>> {
        let mut ret = HashMap::new();
        for (document, location) in documents.into_iter() {
            let e = ret
                .entry(document.identification().label().clone()) //TODO: since we take ownership of documents, we could avoid cloning and taking value itself.
                .or_insert_with(Vec::new);
            e.push((document, location));
        }

        ret
    }

    /// run kenlm models on data, adding perplexity.
    #[cfg(feature = "kenlm")]
    fn run_kenlms(
        models: &Models,
        base_model_path: &Path,
        documents: &mut HashMap<LanguageTag<String>, Vec<(Document, Location)>>,
    ) {
        debug!("Running kenlms");
        for (lang, docs) in documents {
            // attempt to load model for provided lang.
            // It's okay if it's not possible.
            if !models.is_loaded(lang) {
                if let Err(e) = models.load(lang) {
                    debug!("Couldn't load model for lang {lang:?}: {e:?}");
                }
            }

            // TODO: Possible problem here, if between load and get the HM is modified.
            // Add a way of dealing with that?
            // possibly creating a scope and then using "direct" method calls rather than
            // calls that use read/write locks internally.
            if let Some(model) = models.models().get(lang.as_ref()) {
                let model = model.read().unwrap();
                for (doc, _) in docs {
                    model.annotate(doc);
                }
            } else {
                error!("Could not annotate using model {lang}: No model");
            }
        }
    }

    /// concurrently write documets
    fn write_documents<'a>(
        langfiles: &LangFilesDoc,
        avrowriters: &'a RebuildWriters<'a, File>,
        rebuild_root_dir: &Path,
        shard_id: usize,
        documents: HashMap<LanguageTag<String>, Vec<(Document, Location)>>,
    ) -> Result<(), Error> {
        let errors: Vec<Error> = documents
            .into_par_iter()
            .map(|(lang, docs)| {
                info!("[{}]: {} documents", lang, docs.len());

                // check if langfiles has an opened file for provided language
                if !langfiles.contains(&lang) {
                    langfiles.insert_writer(lang.clone())?;
                };
                let writers = langfiles.writers();
                let writer = writers.get(&lang).unwrap();

                if !avrowriters.contains(&lang) {
                    avrowriters.insert(rebuild_root_dir, &lang)?;
                }
                let avrowriters_lock = avrowriters.writers();
                let avrowriter = avrowriters_lock.get(&lang).unwrap();
                let mut writer_lock = writer.lock().unwrap();
                let mut avrowriter_lock = avrowriter.lock().unwrap();

                // divide the documents iterator into two iterators
                let (docs, locations): (Vec<_>, Vec<_>) =
                    docs.into_iter().map(|(doc, loc)| (doc, loc)).unzip();

                // clone metadata
                let metadata_cloned = docs.iter().map(|doc| doc.metadata().clone()).collect();
                let mut sr = ShardResult::new(shard_id as i64, locations, metadata_cloned);
                sr.sort();

                // write docs and rebuild files
                writer_lock.write(docs)?;
                avrowriter_lock.append_ser(sr)?;

                //TODO: not sure that we need the flush
                avrowriter_lock.flush()?;

                Ok(())
            })
            // only collect errors
            .filter_map(|x| match x {
                Ok(_) => None,
                Err(e) => Some(e),
            })
            .collect();

        for error in errors {
            error!("{:?}", error);
        }

        Ok(())
    }
}

impl Pipeline<()> for OscarDoc {
    fn version() -> &'static str {
        "2.0.0"
    }

    fn run(&self) -> Result<(), Error> {
        // let errors;

        let cls = FastTextBuilder::default()
            .path(&self.lid_path)
            .k(1)
            .threshold(0.8)
            .build()?;

        if !self.dst.exists() {
            warn!("Destination file does not exist. Creating");
            std::fs::create_dir(&self.dst)?;
        }

        if !self.dst.is_dir() {
            panic!("Destination has to be a directory: {:?}", self.dst);
        }
        let results = self.get_paths_iter()?;

        // convert to parallel iterator
        // /!\: We use par_bridge, that is suboptimal
        //      compared to implementing IntoParallelIterator
        //      ourselves.
        let results = results.enumerate().par_bridge();

        let langfiles = LangFilesDoc::new(&self.dst, None);
        #[cfg(feature = "kenlm")]
        let kenlms = if let Some(kenlms_path) = &self.kenlms_path {
            if !kenlms_path.is_dir() {
                panic!("KenLMs path must exist and be a dir! {kenlms_path:?}");
            }
            Models::from_dir(kenlms_path)?
        } else {
            /*  TODO: Remove panic here.
                We should either:
                    - Have an "appendable" switch which enables Models to find binaries at runtime (with write lock cost)
                    - Crash on no kenlms provided OR have a warning to indicate that no kenlm annotations will be done.
            */
            panic!("No kenlms path provided but feature turned on!");
        };

        let annotator = {
            let mut annotator = Annotator::default();
            annotator
                .add(Box::<TinyDocument>::default())
                .add(Box::<ShortSentences>::default())
                .add(Box::<Header>::default())
                .add(Box::<LSH>::default())
                .add(Box::<Noisy>::default());

            // add ut1 blocklists for categories
            if let Some(path) = &self.blocklist {
                let bl = MultipleBlocklist::from_dir(path)?;
                annotator.add(Box::new(ContentDetector::new(bl)));
            }

            annotator
        };

        let mut dst_rebuild = self.dst.clone();
        dst_rebuild.push("rebuild");

        let rebuild_files = RebuildWriters::with_dst(&dst_rebuild)?;

        //iterate over shards
        let shards_results =
            results.map(|(idx, shard)| (idx, Self::process_shard(&shard, &cls, None, &annotator)));

        // for each shard result, sort by lang and write concurrently.
        shards_results.for_each(|(idx, shard_result)| {
            if let Ok((shard_id, shard_result)) = shard_result {
                let mut hm = Self::sort_by_lang(shard_result);

                // run kenlms after identification so that shard results are already
                // sorted by language.
                #[cfg(feature = "kenlm")]
                if let Some(kenlms_path) = &self.kenlms_path {
                    Self::run_kenlms(&kenlms, kenlms_path, &mut hm);
                }

                Self::write_documents(&langfiles, &rebuild_files, &dst_rebuild, shard_id, hm)
                    .unwrap();
            } else {
                error!("Error with shard idx {}:{:?}", idx, shard_result);
            }
        });

        Ok(())
    }
}
