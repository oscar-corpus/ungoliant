//! Conversion utilities or fasttext tags to standardized BCP47.
use std::{borrow::Cow, collections::HashMap, convert::TryFrom};

use lazy_static::lazy_static;
use oxilangtag::{LanguageTag, LanguageTagParseError};

lazy_static! {
    pub static ref NEW_TAG_REPLACE: HashMap<&'static str, &'static str> = [
        ("abk", "ab"),
        ("ace_Arab", "ace-Arab"),
        ("ace_Latn", "ace-Latn"),
        ("afr", "af"),
        ("aka", "ak"),
        ("als", "gsw"), //TODO: remove when not using lid.176.bin
        ("amh", "am"),
        ("ara_Arab", "ar-Arab"),
        ("ara_Latn", "ar-Latn"),
        ("asm", "as"),
        ("bak", "ba"),
        ("bam", "bm"),
        ("bel", "be"),
        ("ben", "bn"),
        ("bis", "bi"),
        ("bjn_Arab", "bjn-Arab"),
        ("bjn_Latn", "bjn-Latn"),
        ("bod", "bo"),
        ("bos", "bs"),
        ("bul", "bg"),
        ("cat", "ca"),
        ("ces", "cs"),
        ("che", "ce"),
        ("chv", "cv"),
        ("crh_Latn", "crh-Latn"),
        ("cym", "cy"),
        ("dan", "da"),
        ("deu", "de"),
        ("dzo", "dz"),
        ("ell", "el"),
        ("eml", "x-eml"), // Quality at a Glance table 10
        ("eng", "en"),
        ("epo", "eo"),
        ("est", "et"),
        ("eus", "eu"),
        ("ewe", "ee"),
        ("fao", "fo"),
        ("fas", "fa"),
        ("fij", "fj"),
        ("fin", "fi"),
        ("fra", "fr"),
        ("gla", "gd"),
        ("gle", "ga"),
        ("glg", "gl"),
        ("grn", "gn"),
        ("guj", "gu"),
        ("hat", "ht"),
        ("hau", "ha"),
        ("heb", "he"),
        ("hin", "hi"),
        ("hrv", "hr"),
        ("hun", "hu"),
        ("hye", "hy"),
        ("ibo", "ig"),
        ("ind", "id"),
        ("isl", "is"),
        ("ita", "it"),
        ("jav", "jv"),
        ("jpn", "ja"),
        ("kal", "kl"),
        ("kan", "kn"),
        ("kas_Arab", "ks-Arab"),
        ("kas_Deva", "ks-Deva"),
        ("kat", "ka"),
        ("kau_Arab", "kr-Arab"),
        ("kau_Latn", "kr-Latn"),
        ("kaz", "kk"),
        ("khm", "km"),
        ("kik", "ki"),
        ("kin", "rw"),
        ("kir", "ky"),
        ("kon", "kg"),
        ("kor", "ko"),
        ("kur", "ku"),
        ("lao", "lo"),
        ("lav", "lv"),
        ("lim", "li"),
        ("lin", "ln"),
        ("lit", "lt"),
        ("ltz", "lb"),
        ("lug", "lg"),
        ("mal", "ml"),
        ("mar", "mr"),
        ("min_Latn", "min-Latn"),
        ("mkd", "mk"),
        ("mlg", "mg"),
        ("mlt", "mt"),
        ("mni_Mtei", "mni-Mtei"),
        ("mon", "mn"),
        ("mri", "mi"),
        ("msa", "ms"),
        ("mya", "my"),
        ("nav", "nv"),
        ("nld", "nl"),
        ("nno", "nn"),
        ("nob", "nb"),
        ("nya", "ny"),
        ("oci", "oc"),
        ("orm", "om"),
        ("oss", "os"),
        ("pan", "pa"),
        ("pol", "pl"),
        ("por", "pt"),
        ("prs", "fa-AF"),
        ("pus", "ps"),
        ("que", "qu"),
        ("roh", "rm"),
        ("ron", "ro"),
        ("run", "rn"),
        ("rus", "ru"),
        ("sag", "sg"),
        ("san", "sa"),
        ("sin", "si"),
        ("slk", "sk"),
        ("slv", "sl"),
        ("smo", "sm"),
        ("sna", "sn"),
        ("snd", "sd"),
        ("som", "so"),
        ("sot", "st"),
        ("spa", "es"),
        ("sqi", "sq"),
        ("srd", "sc"),
        ("srp_Cyrl", "sr-Cyrl"),
        ("ssw", "ss"),
        ("sun", "su"),
        ("swe", "sv"),
        ("tah", "ty"),
        ("tam", "ta"),
        ("tat_Cyrl", "tt-Cyrl"),
        ("tel", "te"),
        ("tgk", "tg"),
        ("tgl", "fil"),
        ("tha", "th"),
        ("tir", "ti"),
        ("tmh_Latn", "tmh-Latn"),
        ("tmh_Tfng", "tmh-Tfng"),
        ("ton", "to"),
        ("tsn", "tn"),
        ("tso", "ts"),
        ("tuk", "tk"),
        ("tur", "tr"),
        ("twi", "tw"),
        ("uig", "ug"),
        ("ukr", "uk"),
        ("urd", "ur"),
        ("uzb", "uz"),
        ("vie", "vi"),
        ("wol", "wo"),
        ("xho", "xh"),
        ("yid", "yi"),
        ("yor", "yo"),
        ("zho_Hans", "zh-Hans"),
        ("zho_Hant", "zh-Hant"),
        ("zul", "zu"),
    ]
    .into_iter()
    .collect();
}

pub struct Tag<'a> {
    inner: Cow<'a, str>,
}

impl<'a> Tag<'a> {
    pub fn new(tag: &'a str) -> Self {
        Self {
            // attempt to remove first nine chars or pass the whole thing.
            inner: Tag::fix(tag.get(9..).unwrap_or(tag)),
        }
    }

    #[inline]
    fn fix(tag: &'a str) -> Cow<'a, str> {
        // go from __label__foo_bar to foo_bar
        let tag = match NEW_TAG_REPLACE.get(&tag) {
            None => Cow::from(tag),
            Some(x) => Cow::from(x.to_string()),
        };

        // go from foo_bar to foo-bar
        if tag.contains('_') {
            Cow::from(tag.replace('_', "-"))
        } else {
            tag
        }
    }

    pub fn inner(&self) -> &Cow<'a, str> {
        &self.inner
    }
}

impl<'a> TryFrom<Tag<'a>> for LanguageTag<String> {
    type Error = LanguageTagParseError;
    //TODO: remove cloning, use generics to provide a ref
    // if applicable
    fn try_from(tag: Tag<'a>) -> Result<Self, Self::Error> {
        LanguageTag::parse(tag.inner.into_owned())
    }
}
#[cfg(test)]
mod tests {

    use oxilangtag::LanguageTag;

    use crate::identifiers::tag_convert::Tag;

    // use super::{NewTag, OldTag};

    #[test]
    fn test_en() {
        let old_style = Tag::new("__label__en");
        let new_style = Tag::new("__label__eng");
        let old_style: LanguageTag<String> = old_style.try_into().unwrap();
        let new_style: LanguageTag<String> = new_style.try_into().unwrap();

        assert_eq!(old_style, new_style);
    }

    #[test]
    fn test_langcode_script() {
        let langcode = "__label__fra_Latn";
        let parsed: LanguageTag<String> = Tag::new(langcode).try_into().unwrap();
        let expected = "fra-Latn";
        assert_eq!(parsed, expected);
    }
    #[test]
    fn quality_at_a_glance_table10() {
        // table 10: Miscellaneous errors in language codes.
        // We don't check for sh -> hbs since hbs doesn't seem to be valid bcp47
        let table_10: [(
            Result<LanguageTag<String>, _>,
            Result<LanguageTag<String>, _>,
        ); 2] = [("eml", "x-eml"), ("als", "gsw") /*("sh", "hbs")*/]
            .map(|(old, new)| (format!("__label__{old}"), format!("__label__{new}")))
            .map(|(old, new)| (Tag::new(&old).try_into(), Tag::new(&new).try_into()));

        for (erroneous, correct) in table_10 {
            let (erroneous, correct) = (erroneous.unwrap(), correct.unwrap());
            println!("{erroneous:?} {correct:?}");
            assert_eq!(erroneous, correct);
        }
    }
}
