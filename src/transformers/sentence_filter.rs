//! Sentence transformers
//!
use std::ops::RangeInclusive;

use itertools::Itertools;
use log::debug;
use warc::BufferedBody;
use warc::Record;

use crate::{
    filtering::{sentence::Length, Filter},
    pipelines::oscardoc::types::Document,
};

use super::{Annotate, Transform};

pub struct ShortSentences {
    filter: Length,
    threshold: f32,
}

impl ShortSentences {
    pub fn new(filter: Length, threshold: f32) -> Self {
        Self { filter, threshold }
    }
}

impl Annotate<Document> for ShortSentences {
    fn annotate(&self, doc: &mut Document) {
        let filter_results: Vec<bool> = doc
            .content()
            .lines()
            .map(|line| self.filter.detect(line))
            .collect();

        let nb_lines = filter_results.len();
        // TODO: replace as by some try_into
        let threshold = (self.threshold * nb_lines as f32) as usize;
        // count falses
        let nb_short_lines = filter_results.iter().filter(|result| !**result).count();

        if nb_short_lines > threshold {
            debug!("record {} flagged for short sentences", doc.warc_id());
            doc.metadata_mut()
                .add_annotation("short_sentences".to_string());
        }
    }
}
impl Default for ShortSentences {
    fn default() -> Self {
        Self {
            filter: Default::default(),
            threshold: 0.5,
        }
    }
}
/// Convolution-based head/foot sentence removeer
///
/// The idea is to take surrounding sentence length into account.
/// With a dumb filter, the following length sentence:
///  ```1 1 1 100 1 1 1 1 1 100 100 100 100 100 1 1 1 1```
/// would keep
/// ```100 1 1 1 1 1 100 100 100 100 100```
///  but we'd like to only get
/// ```100 100 100 100 100```
/// The first idea is to iterate over windows, and to sum then divide by window size, keeping the same indices
/// and then filter out.
/// Basically, we convolve with a `[... 1/3 1/3 1/3 ...]` filter.
/// We could try having filters that are more sensible at the start?
pub struct Conv {
    conv_size: usize,
    rss: RemoveShortSentences,
}

impl Conv {
    pub fn new(conv_size: usize, rss: RemoveShortSentences) -> Self {
        Self { conv_size, rss }
    }

    /// transform document and return it, along with a vector containing the indices of kept sentences.
    pub fn transform_idx(&self, mut doc: Document) -> (Document, Vec<RangeInclusive<usize>>) {
        let lines: Vec<&str> = doc.content().lines().collect();
        let line_lengths: Vec<f32> = lines.iter().map(|line| line.len() as f32).collect();
        //add padding
        let padding_size = self.conv_size.div_euclid(2);
        let padding_val_start = *line_lengths.first().unwrap();
        let padding_val_end = *line_lengths.last().unwrap();
        let line_lengths = [
            vec![padding_val_start; padding_size],
            line_lengths,
            vec![padding_val_end; padding_size],
        ]
        .concat();
        //end add padding

        // convolve
        let convolved_lengths: Vec<f32> = line_lengths
            .windows(self.conv_size)
            .map(|lengths| lengths.iter().sum::<f32>() / self.conv_size as f32)
            .collect();

        // filter/remove lines
        // this iterator contains (line_index, (line, convolved_length)).
        // let i: Vec<(usize, (&&str, &f32))> = lines
        let i = lines.iter().zip(convolved_lengths.iter()).enumerate();

        let min_length = *self.rss.filter_min_length() as f32;

        // skip beginning sentences
        let i: Vec<_> = i
            .skip_while(|(_, (_, convolved_length))| convolved_length < &&min_length)
            .collect();

        // skip ending sentences
        let i: Vec<_> = i
            .into_iter()
            .rev()
            .skip_while(|(_, (_, convolved_length))| convolved_length < &&min_length)
            .collect();

        // ensure that there's enough to keep
        // if not, return the intact document and empty ranges.
        // TODO: ensure that it can't be processed further down?
        match (i.first(), i.last()) {
            (Some(end), Some(start)) => {
                let ranges = vec![start.0..=end.0];
                let sentences = i
                    .into_iter()
                    .rev()
                    .map(|(_, (sentence, _))| sentence)
                    .join("\n");

                doc.set_content(sentences);
                (doc, ranges)
            }
            _ => (doc, Vec::new()),
        }
    }
}

impl Default for Conv {
    /// Convolution window size is `5`, minimum length of sentences is `100`.
    fn default() -> Self {
        Self {
            conv_size: 5,
            rss: Default::default(),
        }
    }
}

/// Removes short sentences that are before/after a contiguous chunk of the file.
///
/// The idea is to remove contiguous short sentences that are located before and after a main body.
///
/// By default the short sentence threshold is at 100
/// Example:
/// ```text
/// foo
/// bar
/// baz
/// xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
/// quux
/// xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
/// xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
/// baz
/// bar
/// foo
/// ```
///
/// will be transformed into
///
/// ```text
/// xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
/// quux
/// xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
/// xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
/// ```
#[derive(Default)]
pub struct RemoveShortSentences {
    filter: Length,
}

impl RemoveShortSentences {
    /// Use a custom min_length for long sentences.
    fn new(min_length: usize) -> Self {
        Self {
            filter: Length::with_min_size(min_length),
        }
    }

    /// extracts indices of the document content, ignoring short lines at start/end.
    fn extract_indices<'a>(&self, lines: std::str::Lines<'a>) -> Vec<(usize, &'a str)> {
        let s: Vec<(usize, &str)> = lines
            .enumerate()
            .skip_while(|(_, sentence)| !self.filter.detect(sentence))
            .collect();
        let s: Vec<(usize, &str)> = s
            .into_iter()
            .rev()
            .skip_while(|(_, sentence)| !self.filter.detect(sentence))
            .collect();
        s
    }

    /// build content from extracted indices, returning a unique String along with the indices
    /// that can be used to rebuild the content.
    fn build_content(s: Vec<(usize, &str)>) -> (String, Vec<RangeInclusive<usize>>) {
        match (s.first(), s.last()) {
            (Some(end), Some(start)) => {
                let ranges = vec![start.0..=end.0];
                let sentences = s.into_iter().rev().map(|(_, sentence)| sentence).join("\n");

                (sentences, ranges)
            }
            _ => {
                // set content to empty string
                (String::new(), Vec::new())
            }
        }
    }
    /// get filter detection threshold
    fn filter_min_length(&self) -> &usize {
        self.filter.min_size()
    }
}

impl Transform<Document> for RemoveShortSentences {
    fn transform(&self, doc: &mut Document) -> Vec<RangeInclusive<usize>> {
        let lines = doc.content().lines();

        // TODO: fuse those two methods?
        let s = self.extract_indices(lines);
        let (content, ranges) = Self::build_content(s);

        doc.set_content(content);

        ranges
    }
}

impl Transform<Record<BufferedBody>> for RemoveShortSentences {
    fn transform(&self, doc: &mut Record<BufferedBody>) -> Vec<RangeInclusive<usize>> {
        let stringified = String::from_utf8_lossy(doc.body());
        let lines = stringified.lines();
        let s = self.extract_indices(lines);

        // if we did not get start or end, we return an empty vector
        // meaning that we keed nothing. (analogous to transform_own)
        let (content, ranges) = Self::build_content(s);
        doc.replace_body(content);
        ranges
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::filtering::sentence::Length;
    use crate::pipelines::oscardoc::types::{Document, Metadata};
    use crate::transformers::{Annotate, Transform};

    use super::{RemoveShortSentences, ShortSentences};

    fn gen_valid() -> (Document, String) {
        let content = r"foo
bar
baz

xxxxxxxxxxx
quux
xxxxxxxxxxx
xxxxxxxxxxx
foo
bar
baz
"
        .to_string();

        let expected_content = r"xxxxxxxxxxx
quux
xxxxxxxxxxx
xxxxxxxxxxx"
            .to_string();
        let headers = HashMap::new();
        let metadata = Metadata::default();
        let doc = Document::new(content, headers, metadata);

        (doc, expected_content)
    }

    fn gen_valid_long() -> (Document, String) {
        let content = r"foo
bar
baz
very long sentence about cookies, privacy and blah blah blah blah blah blah blah blah
xx
xx
xx
xx
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
tiny sentence in main content
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
again
hehe
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
xx
xx
xx
xx
xx
xx
very long sentence about cookies, privacy and blah blah blah blah blah blah blah blah
xx
xx
"
        .to_string();

        let expected_content = r"MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
tiny sentence in main content
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
again
hehe
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT 
MAIN CONTENT  MAIN CONTENT MAIN CONTENT MAIN CONTENT MAIN CONTENT"
            .to_string();
        let headers = HashMap::new();
        let metadata = Metadata::default();
        let doc = Document::new(content, headers, metadata);

        (doc, expected_content)
    }

    fn gen_invalid() -> Document {
        let content = r"foo
bar
baz

quux
foo
bar
baz
"
        .to_string();
        let headers = HashMap::new();
        let metadata = Metadata::default();

        Document::new(content, headers, metadata)
    }
    #[test]
    fn test_rss_default() {
        let rss = RemoveShortSentences::default();
        assert_eq!(rss.filter_min_length(), &100);
    }
    #[test]
    fn test_rss() {
        let (mut doc, expected_content) = gen_valid();
        let rss = RemoveShortSentences::new(10);

        rss.transform(&mut doc);

        println!("{:#?}", doc);

        assert_eq!(doc.content(), &expected_content);
    }

    #[test]
    fn test_rss_empty() {
        let content = r"foo
bar
baz

quux
foo
bar
baz
"
        .to_string();

        let headers = HashMap::new();
        let metadata = Metadata::default();
        let mut doc = Document::new(content, headers, metadata);

        let rss = RemoveShortSentences::new(10);

        rss.transform(&mut doc);

        println!("{:#?}", doc);

        assert_eq!(doc.content(), "");
    }

    #[test]
    fn test_rss_idx() {
        let (mut doc, _) = gen_valid();
        let expected_range = vec![4..=7];
        let rss = RemoveShortSentences::new(10);

        let range_transformed = rss.transform(&mut doc);

        println!("{:#?}", range_transformed);

        assert_eq!(range_transformed, expected_range);
    }

    #[test]
    fn test_rss_idx_invalid() {
        let mut doc = gen_invalid();
        let expected_ranges = Vec::new();

        let rss = RemoveShortSentences::new(10);

        let range_transformed = rss.transform(&mut doc);

        assert_eq!(range_transformed, expected_ranges);
    }

    // #[test]
    // fn test_conv() {
    //     let (doc, expected) = gen_valid_long();
    //     let c = Conv::new(3, RemoveShortSentences::new(60));
    //     println!(
    //         "{:?}",
    //         doc.content()
    //             .lines()
    //             .map(|line| line.len())
    //             .collect::<Vec<usize>>()
    //     );
    //     println!("{:#?}", c.transform_idx(doc));
    // }

    //     #[test]
    //     fn test_annotate_short() {
    //         let (mut doc, _) = gen_valid_long();
    //         let content = r#"Long enough sentence here :)
    // tiny one
    // tiny one
    // tiny one
    // Long enough sentence here :)"#;
    //         doc.set_content(content.to_string());
    //         let a = ShortSentences::new(Length::with_min_size(10), 0.5);
    //         let annotation = "short_sentences";
    //         a.annotate(&mut doc);
    //         assert!(doc
    //             .metadata()
    //             .annotation()
    //             .unwrap()
    //             .contains(&String::from(annotation)))
    //     }

    #[test]
    fn test_no_annotation() {
        let (mut doc, _) = gen_valid_long();
        let content = r#"Long enough sentence here :)
tiny one
Long enough sentence here :)
Long enough sentence here :)
Long enough sentence here :)
tiny one
tiny one
Long enough sentence here :)"#;
        doc.set_content(content.to_string());
        let a = ShortSentences::new(Length::with_min_size(10), 0.5);
        a.annotate(&mut doc);
        // this fails if doc is annotated with something else
        assert!(doc.metadata().annotation().is_none())
    }

    #[test]
    fn test_single_sentence() {
        let (mut doc, _) = gen_valid_long();
        let content = r#"Ti Pebrero 29 ket ti maika-60 nga aldaw iti bisiesto a tawen iti kalendario a Gregoriano, nga addaan pay nabati a 306 nga al-aldaw tapno maungpot ti tawen."#;
        doc.set_content(content.to_string());

        let _a = ShortSentences::new(Length::with_min_size(10), 0.5);
        let a = ShortSentences::default();
        a.annotate(&mut doc);
        // this fails if doc is annotated with something else
        assert!(doc.metadata().annotation().is_none())
    }
}
