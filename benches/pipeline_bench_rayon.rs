use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

use rayon::prelude::*;
use ungoliant::identifiers::FastText;
use ungoliant::sources::commoncrawl::Wet;

const NB_RECORDS: usize = 250;
// bench protocol:
//
// We take 4 files (TODO: 8 in order to have files>cores/threads)
// We take 100 records per file
//
// - Full sequential
// - Sequential on WET and Records, Parallel on Sentences
// - Sequential on WET and Sentences, Parallel on Records
// - Parallel on WET, Sequential on Sentences and Records

// Full sequential
fn sequential(nb_shards: usize) {
    let cls = FastText::new_lid().unwrap();
    let results = std::fs::read_dir("results/")
        .unwrap()
        .map(|d| Wet::from_path_gzip(d.unwrap().path()).unwrap())
        .take(nb_shards);

    for wetfile in results {
        for record in wetfile.iter.take(NB_RECORDS) {
            let record = record.unwrap();
            let body = String::from_utf8(record.body().to_vec()).ok();
            if let Some(sentences) = body {
                let sentences = sentences.lines().filter(|line| line.chars().count() > 100);
                for sentence in sentences {
                    cls.predict(sentence);
                }
            }
        }
    }
}

// Sequential on WET files and records, concurrent on lines.
fn parallel_on_sentences(nb_shards: usize) {
    let cls = FastText::new_lid().unwrap();
    let results = std::fs::read_dir("results/")
        .unwrap()
        .map(|d| Wet::from_path_gzip(d.unwrap().path()).unwrap())
        .take(nb_shards);

    for wetfile in results {
        for record in wetfile.iter.take(NB_RECORDS) {
            let record = record.unwrap();
            let body = String::from_utf8(record.body().to_vec()).ok();
            if let Some(sentences) = body {
                let sentences: Vec<&str> = sentences
                    .lines()
                    .filter(|line| line.chars().count() > 100)
                    .collect();
                sentences.par_iter().for_each(|sentence| {
                    cls.predict(sentence);
                });
            }
        }
    }
}

// parallel on records
fn parallel_on_records(nb_shards: usize) {
    let cls = FastText::new_lid().unwrap();
    let results = std::fs::read_dir("results/")
        .unwrap()
        .map(|d| Wet::from_path_gzip(d.unwrap().path()).unwrap())
        .take(nb_shards);
    for wetfile in results {
        let records = wetfile.iter.into_iter().take(NB_RECORDS).par_bridge();
        records.for_each(|record| {
            let record = record.unwrap();
            let body = String::from_utf8(record.body().to_vec()).ok();
            if let Some(sentences) = body {
                let sentences: Vec<&str> = sentences
                    .lines()
                    .filter(|line| line.chars().count() > 100)
                    .collect();
                sentences.iter().for_each(|sentence| {
                    cls.predict(sentence);
                });
            }
        });
    }
}

// parallel on WET
fn parallel_on_shards(nb_shards: usize) {
    let cls = FastText::new_lid().unwrap();
    let results = std::fs::read_dir("results/")
        .unwrap()
        .map(|d| Wet::from_path_gzip(d.unwrap().path()).unwrap())
        .take(nb_shards);
    let results = results.par_bridge();
    results.for_each(|wetfile| {
        let records = wetfile.iter.take(NB_RECORDS);
        for record in records {
            let record = record.unwrap();
            let body = String::from_utf8(record.body().to_vec()).ok();
            if let Some(sentences) = body {
                for sentence in sentences.lines().filter(|line| line.chars().count() > 100) {
                    cls.predict(sentence);
                }
            }
        }
    });
}

// parallel on WET and sentences
fn parallel_on_shards_and_sentences(nb_shards: usize) {
    let cls = FastText::new_lid().unwrap();
    let results = std::fs::read_dir("results/")
        .unwrap()
        .map(|d| Wet::from_path_gzip(d.unwrap().path()).unwrap())
        .take(nb_shards);

    let results = results.par_bridge();
    results.for_each(|wetfile| {
        let records = wetfile.iter.take(NB_RECORDS);
        for record in records {
            let record = record.unwrap();
            let body = String::from_utf8(record.body().to_vec()).ok();
            if let Some(sentences) = body {
                let sentences = sentences
                    .lines()
                    .filter(|line| line.chars().count() > 100)
                    .par_bridge();
                sentences.for_each(|s| {
                    cls.predict(s);
                });
            }
        }
    });
}

fn parallel_all(nb_shards: usize) {
    let cls = FastText::new_lid().unwrap();
    let results = std::fs::read_dir("results/")
        .unwrap()
        .map(|d| Wet::from_path_gzip(d.unwrap().path()).unwrap())
        .take(nb_shards);

    let results = results.par_bridge();
    results.for_each(|wetfile| {
        let records = wetfile.iter.take(NB_RECORDS).par_bridge();
        records.for_each(|record| {
            let record = record.unwrap();
            let body = String::from_utf8(record.body().to_vec()).ok();
            if let Some(sentences) = body {
                let sentences = sentences
                    .lines()
                    .filter(|line| line.chars().count() > 100)
                    .par_bridge();
                sentences.for_each(|s| {
                    cls.predict(s);
                });
            }
        });
    });
}

fn bench_pipelines(c: &mut Criterion) {
    let mut group = c.benchmark_group("Pipeline");
    // for nb_shards in vec![1, 10, 25] {
    for nb_shards in vec![25] {
        group.bench_with_input(
            BenchmarkId::new("parallel on records", nb_shards),
            &nb_shards,
            |b, nb_shards| b.iter(|| parallel_on_records(*nb_shards)),
        );
        group.bench_with_input(
            BenchmarkId::new("parallel on shards", nb_shards),
            &nb_shards,
            |b, nb_shards| b.iter(|| parallel_on_shards(*nb_shards)),
        );
        group.bench_with_input(
            BenchmarkId::new("parallel on all", nb_shards),
            &nb_shards,
            |b, nb_shards| b.iter(|| parallel_all(*nb_shards)),
        );
    }
    group.finish();
}

criterion_group!(benches, bench_pipelines);
criterion_main!(benches);
