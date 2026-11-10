//! Segment properties (ADR 0020, 区間の性質): the direction, script and
//! language of a run of text, as given and as guessed
//! (`hb_segment_properties_t`, `hb_buffer_guess_segment_properties`).
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-buffer.cc` and `src/hb-common.cc`
//! (`hb_script_get_horizontal_direction`), MIT; see
//! THIRD_PARTY_NOTICES.md.

use sofel_unicode::Script;

use crate::buffer::UnicodeBuffer;
use crate::direction::Direction;
use crate::language::Language;

/// The properties of a run of text as the caller knows them; `None`
/// fields are guessed by [`SegmentProperties::guess`].
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct SegmentProperties {
    /// The direction.
    pub direction: Option<Direction>,
    /// The script.
    pub script: Option<Script>,
    /// The language.
    pub language: Option<Language>,
}

/// The properties a plan is made for: a direction always, a script and
/// a language when known.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ResolvedProperties {
    /// The direction.
    pub direction: Direction,
    /// The script; `None` is HarfBuzz's `HB_SCRIPT_INVALID`.
    pub script: Option<Script>,
    /// The language; `None` is HarfBuzz's `HB_LANGUAGE_INVALID`.
    pub language: Option<Language>,
}

impl SegmentProperties {
    /// Fills in what is missing as `hb_buffer_guess_segment_properties`
    /// does, without reading the process locale: the script from the
    /// first character whose script is not Common, Inherited or
    /// Unknown; the direction from the script
    /// ([`script_horizontal_direction`], left to right when that gives
    /// none); the language from `default_language` (HarfBuzz reads the
    /// locale, `hb_language_get_default`; ADR 0020 leaves that to the
    /// caller). Fields already set are kept.
    pub fn guess(
        self,
        buffer: &UnicodeBuffer,
        default_language: Option<Language>,
    ) -> ResolvedProperties {
        let script = self.script.or_else(|| {
            buffer.codepoints().map(sofel_unicode::script).find(|&s| {
                s != Script::COMMON && s != Script::INHERITED && s != Script::UNKNOWN
            })
        });
        let direction = self
            .direction
            .unwrap_or_else(|| script_horizontal_direction(script).unwrap_or(Direction::Ltr));
        ResolvedProperties {
            direction,
            script,
            language: self.language.or(default_language),
        }
    }
}

/// The direction of a script set horizontally
/// (`hb_script_get_horizontal_direction`): right to left for the
/// right-to-left scripts, `None` for Old Hungarian, Old Italic, Runic and
/// Tifinagh, which are written both ways, and left to right for every
/// other script, unknown ones and `None` (`HB_SCRIPT_INVALID`) included.
pub fn script_horizontal_direction(script: Option<Script>) -> Option<Direction> {
    let Some(script) = script else {
        return Some(Direction::Ltr);
    };
    match &script.tag() {
        b"Arab" | b"Hebr" | b"Syrc" | b"Thaa" | b"Cprt" | b"Khar" | b"Phnx" | b"Nkoo"
        | b"Lydi" | b"Avst" | b"Armi" | b"Phli" | b"Prti" | b"Sarb" | b"Orkh" | b"Samr"
        | b"Mand" | b"Merc" | b"Mero" | b"Mani" | b"Mend" | b"Nbat" | b"Narb" | b"Palm"
        | b"Phlp" | b"Hatr" | b"Adlm" | b"Rohg" | b"Sogo" | b"Sogd" | b"Elym" | b"Chrs"
        | b"Yezi" | b"Ougr" | b"Gara" | b"Sidt" => Some(Direction::Rtl),
        b"Hung" | b"Ital" | b"Runr" | b"Tfng" => None,
        _ => Some(Direction::Ltr),
    }
}
