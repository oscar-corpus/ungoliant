/*! Document transformers.

Transformers can either (or both) [Annotate] content or [Transform] it:

- The [Annotate] trait only adds an annotation (see [crate::pipelines::oscardoc::types::Metadata]), without altering any content,
- The [Transform] trait can change the content (and shouldn't add any annotation?).
  It (for now) should only remove sentences without altering them.
!*/

mod annotate;
mod content_detector;
mod header;

mod lsh;
mod noisy;

#[cfg(feature = "kenlm")]
mod kenlm;

mod sentence_filter;
mod tiny;
mod transform;
pub use annotate::Annotate;
pub use annotate::Annotator;
pub use content_detector::ContentDetector;
pub use header::Header;
#[cfg(feature = "kenlm")]
pub use kenlm::AdultDetector;
#[cfg(feature = "kenlm")]
pub use kenlm::AdultDetectorBuilder;
#[cfg(feature = "kenlm")]
pub use kenlm::Models;
pub use lsh::LSH;
pub use noisy::Noisy;
pub use sentence_filter::Conv;
pub use sentence_filter::RemoveShortSentences;
pub use sentence_filter::ShortSentences;
pub use tiny::TinyDocument;
pub use transform::Transform;
