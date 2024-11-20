/*! Header/Footer annotators

Annotator that watches for short lines at the beginning/end of documents, adding `footer` and/or `header` annotations.
!*/
use crate::pipelines::oscardoc::types::Document;

use super::Annotate;

/// Header/Footer annotator.
///
/// Checks for short sentences in the beginnning/end of documents, and flags if there's too much.
pub struct Header {
    header_pctg: f64,
    threshold_pctg: f64,
    min_length: usize,
}

impl Default for Header {
    /// Default values are:
    /// - 20% of the document for the header/footer
    /// - flagging if >50% of the sentences are short
    /// - and < 100 lines = short sentence.
    fn default() -> Self {
        Self {
            header_pctg: 0.2,
            threshold_pctg: 0.5,
            min_length: 100,
        }
    }
}

impl Annotate<Document> for Header {
    /// checks lines and adds annotations if applicable.
    fn annotate(&self, doc: &mut Document) {
        let nb_lines = doc.content().lines().count();

        // there could be better ways of casting this.
        let nb_lines_header = (nb_lines as f64 * self.header_pctg).floor();
        let treshold_lines = (nb_lines_header * self.threshold_pctg).floor() as u64;
        let nb_lines_header = nb_lines_header as usize;

        // iterate over the header, counting short lines
        let short_lines_count = self.count_short_lines(doc.content().lines().take(nb_lines_header));
        if short_lines_count > treshold_lines {
            doc.metadata_mut().add_annotation("header".to_string());
        }

        // do the same in reverse order (to get footer)
        let short_lines_count =
            self.count_short_lines(doc.content().lines().rev().take(nb_lines_header));

        if short_lines_count > treshold_lines {
            doc.metadata_mut().add_annotation("footer".to_string());
        }
    }
}

impl Header {
    /// New [Header] with custom values.
    ///
    /// * `header_pctg` is the percentage of the document that is considered a header (on lines, not bytes).
    ///    A 100 line document with a `header_pctg` at 0.20 will consider the header is lines 0..20.
    /// * `threshold_pctg`: percentage of short lines required to be annotated.
    ///    A 100 line document with 20 header lines will get annotated if there's 10 or more short lines, if we're using 0.50 as a threshold.
    /// * `min_length` is the minimum length of a sentence. If a sentence is shorter than this, it is considered a short one.
    fn new(header_pctg: f64, threshold_pctg: f64, min_length: usize) -> Self {
        Self {
            header_pctg,
            threshold_pctg,
            min_length,
        }
    }

    /// counts the number of short lines at the beginning of a string iterator.
    #[inline]
    fn count_short_lines<'a>(&self, lines: impl Iterator<Item = &'a str>) -> u64 {
        // reset counter
        let mut short_lines_count = 0;

        for line in lines {
            if line.len() < self.min_length {
                short_lines_count += 1;
            }
        }

        short_lines_count
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use crate::{
        pipelines::oscardoc::types::{Document, Metadata},
        transformers::Annotate,
    };

    use super::Header;

    #[test]
    fn lengthy_enough() {
        let annotator = Header::new(0.30, 0.60, 30);
        let text = r"This is a lengthy enough sentence! Or at least I hope :)
        This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
short one but it's ok
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)";

        let mut doc = Document::new(text.to_string(), HashMap::new(), Metadata::default());
        annotator.annotate(&mut doc);
        assert_eq!(doc.metadata().annotation(), None);
    }

    #[test]
    fn test_header() {
        let annotator = Header::new(0.30, 0.60, 30);
        let text = r"This is a lengthy enough sentence! Or at least I hope :)
oop, tiny one here
oop, tiny one here
oop, tiny one here
oop, tiny one here
oop, tiny one here
oop, tiny one here
oop, tiny one here
oop, tiny one here
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
short one but it's ok
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)";

        let mut doc = Document::new(text.to_string(), HashMap::new(), Metadata::default());
        annotator.annotate(&mut doc);
        assert_eq!(
            doc.metadata().annotation(),
            Some(vec!["header".to_string()]).as_ref()
        );
    }

    #[test]
    fn test_footer() {
        let annotator = Header::new(0.30, 0.60, 30);
        let text = r"This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
short one but it's ok
short one but it's ok
short one but it's ok
short one but it's ok
This is a lengthy enough sentence! Or at least I hope :)";

        let mut doc = Document::new(text.to_string(), HashMap::new(), Metadata::default());
        annotator.annotate(&mut doc);
        assert_eq!(
            doc.metadata().annotation(),
            Some(vec!["footer".to_string()]).as_ref()
        );
    }

    #[test]
    fn test_both() {
        let annotator = Header::new(0.30, 0.60, 30);
        let text = r"This is a lengthy enough sentence! Or at least I hope :)
short one but it's ok
short one but it's ok
short one but it's ok
short one but it's ok
short one but it's ok
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
short one but it's ok
short one but it's ok
short one but it's ok
short one but it's ok
This is a lengthy enough sentence! Or at least I hope :)";

        let mut doc = Document::new(text.to_string(), HashMap::new(), Metadata::default());
        annotator.annotate(&mut doc);
        assert_eq!(
            doc.metadata().annotation(),
            Some(vec!["header".to_string(), "footer".to_string()]).as_ref()
        );
    }

    #[test]
    fn count_lines() {
        let text = r"This is a lengthy enough sentence! Or at least I hope :)
short one but it's ok
short one but it's ok
short one but it's ok
short one but it's ok
short one but it's ok
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
This is a lengthy enough sentence! Or at least I hope :)
short one but it's ok
short one but it's ok
short one but it's ok
This is a lengthy enough sentence! Or at least I hope :)";

        let h = Header::new(0.10, 0.50, 30);
        let short_count = h.count_short_lines(text.lines().take(10));
        assert_eq!(short_count, 5);
    }
    #[test]
    fn test_short_nblines_valid_doc() {
        let text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed a diam mollis, scelerisque arcu sed, bibendum ligula. Curabitur convallis urna auctor mi varius, 
at sagittis arcu vehicula. Maecenas ante velit, bibendum vel ligula quis, dapibus eleifend nibh. Vivamus dapibus nibh non eros feugiat accumsan. In ut risus vitae risus aliquet dictum blandit sed velit. Fusce semper sagittis egestas. Cras orci diam, tristique vel dictum a, mollis in velit. 
Suspendisse nisi tellus, fermentum eu cursus sit amet, dictum ornare nisi. Donec rhoncus nisi lacus, at malesuada nunc egestas nec. Nunc in elit nunc. Quisque id euismod lectus, id porttitor ante. Duis ac nibh tincidunt, vestibulum orci nec, sagittis tortor. 
Sed non ipsum et lacus mattis eleifend. Donec ultricies efficitur enim, non consectetur lorem efficitur et. Pellentesque non malesuada magna, vitae congue arcu. 
Aliquam tempus volutpat laoreet. Etiam facilisis nisl turpis, sed euismod justo euismod sit amet. Phasellus eget urna sodales, luctus dolor vel, malesuada arcu. Nullam tincidunt sem et nibh volutpat volutpat.
Etiam ornare ligula sollicitudin scelerisque finibus. Suspendisse orci odio, laoreet sed sapien ut, dignissim venenatis risus.";
        let h = Header::default();
        let mut d = Document::new(text.to_string(), HashMap::new(), Metadata::default());
        h.annotate(&mut d);
        assert_eq!(d.metadata().annotation(), None);
    }

    #[test]
    fn test_one_long_sentence() {
        let text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed a diam mollis, scelerisque arcu sed, bibendum ligula. Curabitur convallis urna auctor mi varius.";
        let h = Header::default();
        let mut d = Document::new(text.to_string(), HashMap::new(), Metadata::default());
        h.annotate(&mut d);
        assert_eq!(d.metadata().annotation(), None);
    }

    #[test]
    fn test_one_short_sentence() {
        let text = "Lorem ipsum dolor sit amet";
        let h = Header::default();
        let mut d = Document::new(text.to_string(), HashMap::new(), Metadata::default());
        h.annotate(&mut d);
        assert_eq!(d.metadata().annotation(), None);
    }
}
