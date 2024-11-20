use std::convert::{TryFrom, TryInto};

use serde::{Deserialize, Serialize};

/// Incomplete location error type.
///
/// uses [LocationKind] to inform which field is missing.
#[derive(Debug, Clone)]
pub struct IncompleteLocation {
    missing: LocationKind,
}

/// enum of the mandatory [Location] fields.
///
// Not very elegant but works for now.
#[derive(Debug, Clone)]
pub enum LocationKind {
    ShardId,
    RecordID,
    LineStart,
    LineEnd,
    LocInShard,
}

/// A partial, still being filled location.
/// Each field shouldn't be filled more than once to
/// guarantee some integrity.
// TODO: Add methods to ensure that we add only once?
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct LocationBuilder {
    shard_id: Option<usize>,
    record_id: Option<String>,
    line_start: Option<usize>,
    line_end: Option<usize>,
    loc_in_shard: Option<usize>,
}

impl LocationBuilder {
    /// Set the partial location's shard id.
    pub fn set_shard_id(&mut self, shard_id: usize) {
        self.shard_id = Some(shard_id);
    }

    /// Set the partial location's record id.
    pub fn set_record_id(&mut self, record_id: String) {
        self.record_id = Some(record_id);
    }

    /// Set the partial location's line start.
    pub fn set_line_start(&mut self, line_start: usize) {
        self.line_start = Some(line_start);
    }

    /// Set the partial location's line end.
    pub fn set_line_end(&mut self, line_end: usize) {
        self.line_end = Some(line_end);
    }

    /// Set the partial location's loc in shard.
    pub fn set_loc_in_shard(&mut self, loc_in_shard: usize) {
        self.loc_in_shard = Some(loc_in_shard);
    }

    /// Builds the location.
    ///
    /// Errors if a field is missing
    pub fn build(self) -> Result<Location, IncompleteLocation> {
        self.try_into()
    }
}

impl TryFrom<LocationBuilder> for Location {
    type Error = IncompleteLocation;

    fn try_from(value: LocationBuilder) -> Result<Self, Self::Error> {
        let shard_id = value.shard_id.ok_or(IncompleteLocation {
            missing: LocationKind::ShardId,
        })?;

        let record_id = value.record_id.ok_or(IncompleteLocation {
            missing: LocationKind::RecordID,
        })?;

        let line_start = value.line_start.ok_or(IncompleteLocation {
            missing: LocationKind::LineStart,
        })?;
        let line_end = value.line_end.ok_or(IncompleteLocation {
            missing: LocationKind::LineEnd,
        })?;
        let loc_in_shard = value.loc_in_shard.ok_or(IncompleteLocation {
            missing: LocationKind::LocInShard,
        })?;

        Ok(Location {
            shard_id,
            record_id,
            line_start,
            line_end,
            loc_in_shard,
        })
    }
}
/// Links a record id to a set location in a shard:
/// - shard_id is the shard number (ex. 12345.txt.gz)
/// - record_id is the record id :)
/// - line_start/line_end are the boundaries of kept text (inclusive)
/// - loc_in_shard is the record index _in_ shard.
///
/// # Example
/// If we're working on the 10th record of a shard that is shard 100,
/// that the record has 10 lines and we only keep the first 5,
/// We'd get `line_start=0, line_end=4, loc_in_shard=99`.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct Location {
    shard_id: usize,
    record_id: String,
    line_start: usize,
    line_end: usize,
    loc_in_shard: usize,
}

impl Location {
    /// Create a new [Location].
    ///
    /// Depending on usage, [LocationBuilder] can be more convinient.
    pub fn new(
        shard_id: usize,
        record_id: String,
        line_start: usize,
        line_end: usize,
        loc_in_shard: usize,
    ) -> Self {
        Self {
            shard_id,
            record_id,
            line_start,
            line_end,
            loc_in_shard,
        }
    }

    /// Get a reference to the location's shard id.
    pub fn shard_id(&self) -> usize {
        self.shard_id
    }

    /// Get a reference to the location's record id.
    pub fn record_id(&self) -> &str {
        self.record_id.as_ref()
    }

    /// Get a reference to the location's line start.
    pub fn line_start(&self) -> usize {
        self.line_start
    }

    /// Get a reference to the location's line end.
    pub fn line_end(&self) -> usize {
        self.line_end
    }

    /// Get a reference to the location's loc in shard.
    pub fn loc_in_shard(&self) -> usize {
        self.loc_in_shard
    }
}

#[cfg(test)]
mod tests {

    use super::Location;
    use super::LocationBuilder;

    #[test]
    fn location_build_incomplete() {
        let lb = LocationBuilder::default();
        assert!(lb.build().is_err());
    }

    #[test]
    fn location_build_complete() {
        let mut lb = LocationBuilder::default();
        let (rid, ls, le, lis, si) = ("record_id", 0, 10, 1, 4);
        lb.set_record_id(rid.to_string());
        lb.set_line_start(ls);
        lb.set_line_end(le);
        lb.set_loc_in_shard(lis);
        lb.set_shard_id(si);
        let loc_built = lb.build();
        assert!(loc_built.is_ok());
        let loc_built = loc_built.unwrap();

        let location = Location::new(si, rid.to_string(), ls, le, lis);

        assert_eq!(location, loc_built);
    }
}
