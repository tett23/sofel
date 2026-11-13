//! The validators of `CFF ` and `CFF2` (ADR 0021).

use super::charstring::{self, CharStringErrorKind, Context, NullSink, Work};
use super::dict::{
    self, After, BlendWeights, DictError, DictErrorKind, DictKind, DictToken, DictTokens, FontDict,
    NoBlend, PrivateDict, TopDict, op,
};
use super::{Charset, Encoding, FdSelect, Header, Index, TAG, TAG2, Version};
use crate::be::{self, to_u32, to_usize};
use crate::item_variation::{self, ItemVariationStore};
use crate::{Diagnostic, Kind, ReadError, Tag};

struct Reporter<'r> {
    tag: Tag,
    report: &'r mut dyn FnMut(Diagnostic),
    errors: bool,
}

impl Reporter<'_> {
    fn emit(&mut self, at: usize, kind: Kind) {
        if kind.severity() == crate::Severity::Error {
            self.errors = true;
        }
        (self.report)(Diagnostic::new(self.tag, to_u32(at), kind));
    }

    fn read_error(&mut self, at: usize, e: ReadError, available: usize) {
        let kind = match e {
            ReadError::Truncated { needed, available } => Kind::Truncated {
                needed: to_u32(needed),
                available: to_u32(available),
            },
            ReadError::UnknownVersion(found) => Kind::UnknownVersion { found },
            ReadError::UnknownFormat(f) => Kind::UnknownFormat {
                found: u32::from(f),
            },
            ReadError::OutOfBounds | ReadError::InvalidValue => Kind::OffsetOutOfBounds {
                offset: to_u32(at),
                length: 0,
                available: to_u32(available),
            },
        };
        self.emit(at, kind);
    }
}

/// Checks the INDEX at `at`; returns it when it can be read.
fn index<'a>(
    r: &mut Reporter<'_>,
    data: &'a [u8],
    at: usize,
    version: Version,
) -> Option<Index<'a>> {
    let Some(tail) = data.get(at..) else {
        r.emit(
            at,
            Kind::OffsetOutOfBounds {
                offset: to_u32(at),
                length: to_u32(version.count_len()),
                available: to_u32(data.len()),
            },
        );
        return None;
    };
    let idx = match Index::new(tail, version) {
        Ok(i) => i,
        Err(ReadError::InvalidValue) => {
            let found = be::u8_at(tail, version.count_len()).map_or(0, u32::from);
            r.emit(
                at.saturating_add(version.count_len()),
                Kind::InvalidOffSize { found },
            );
            return None;
        }
        Err(e) => {
            r.read_error(at, e, data.len());
            return None;
        }
    };
    if idx.is_empty() {
        return Some(idx);
    }
    let first = idx.offset(0).unwrap_or(0);
    if first != 1 {
        r.emit(at, Kind::FirstOffsetNotOne { found: first });
        return None;
    }
    let mut previous = first;
    for i in 1..=idx.len() {
        let o = idx.offset(i).unwrap_or(0);
        if o < previous {
            r.emit(at, Kind::OffsetsNotSorted { index: i });
            return None;
        }
        previous = o;
    }
    let end = idx.end().unwrap_or(0);
    if end > tail.len() {
        r.emit(
            at,
            Kind::OffsetOutOfBounds {
                offset: to_u32(at.saturating_add(idx.data_offset())),
                length: previous.saturating_sub(1),
                available: to_u32(data.len()),
            },
        );
        return None;
    }
    Some(idx)
}

/// Whether the spec or FreeType knows `o` in a DICT of `kind`.
fn spec_known(kind: DictKind, o: u16) -> bool {
    if dict::field(kind, o).is_some() {
        return true;
    }
    let e = |b: u8| dict::escape(b);
    match kind {
        DictKind::Top => matches!(o, op::XUID) || o == e(22) || o == e(23),
        DictKind::Private => false,
        DictKind::Top2 | DictKind::Font2 | DictKind::Private2 => false,
    }
}

fn other_version(kind: DictKind) -> DictKind {
    match kind {
        DictKind::Top => DictKind::Top2,
        DictKind::Top2 => DictKind::Top,
        DictKind::Private => DictKind::Private2,
        DictKind::Private2 => DictKind::Private,
        DictKind::Font2 => DictKind::Top,
    }
}

fn dict_error(r: &mut Reporter<'_>, base: usize, e: DictError) {
    let at = base.saturating_add(e.offset);
    let kind = match e.kind {
        DictErrorKind::StackOverflow => Kind::DictStackOverflow,
        DictErrorKind::StackUnderflow { operator } => Kind::DictMissingOperands {
            operator: u32::from(operator),
        },
        DictErrorKind::TruncatedOperator => Kind::DictTruncatedOperator,
        DictErrorKind::InvalidValue { operator } => Kind::DictInvalidValue {
            operator: u32::from(operator),
        },
        DictErrorKind::Blend => Kind::DictBlend,
    };
    r.emit(at, kind);
}

fn valid_real(nibbles: &[u8]) -> bool {
    let Some(last) = nibbles.last() else {
        return false;
    };
    if last >> 4 != 15 && last & 0xF != 15 {
        return false;
    }
    nibbles
        .iter()
        .flat_map(|&b| [b >> 4, b & 0xF])
        .take_while(|&n| n != 15)
        .all(|n| n != 0xD)
}

/// The warnings and errors of the DICT `data` at `base`, without the
/// value checks of the parsers.
fn dict_checks(r: &mut Reporter<'_>, data: &[u8], base: usize, kind: DictKind) {
    for (at, t) in DictTokens::new(data).flatten() {
        if let DictToken::Real(n) = t
            && !valid_real(n)
        {
            r.emit(base.saturating_add(at), Kind::InvalidReal);
        }
    }
    let mut seen = [0u64; 8];
    let result = dict::run(data, kind.stack_size(), &mut |o, at, s| {
        let at = base.saturating_add(at);
        let operator = u32::from(o);
        if !spec_known(kind, o) {
            if spec_known(other_version(kind), o) {
                r.emit(at, Kind::OperatorNotAllowed { operator });
            } else {
                r.emit(at, Kind::UnknownOperator { operator });
            }
        }
        let bit = usize::from(if o >= 0x0C00 {
            256 | (o & 0xFF)
        } else {
            o & 0xFF
        });
        if let Some(word) = seen.get_mut(bit / 64) {
            let mask = 1u64 << (bit % 64);
            if *word & mask != 0 && o != op::BLEND {
                r.emit(at, Kind::DuplicateOperator { operator });
            }
            *word |= mask;
        }
        let _ = s;
        Ok(if o == op::BLEND && kind == DictKind::Private2 {
            After::Keep
        } else {
            After::Clear
        })
    });
    // Value errors come from the real parsers; only the lexing and stack
    // errors are reported here.
    if let Err(e) = result
        && matches!(
            e.kind,
            DictErrorKind::StackOverflow | DictErrorKind::TruncatedOperator
        )
    {
        dict_error(r, base, e);
    }
}

fn top_dict(r: &mut Reporter<'_>, data: &[u8], base: usize, version: Version) -> Option<TopDict> {
    let kind = match version {
        Version::Cff => DictKind::Top,
        Version::Cff2 => DictKind::Top2,
    };
    dict_checks(r, data, base, kind);
    match TopDict::parse(data, version) {
        Ok(t) => Some(t),
        Err(e) => {
            if !matches!(
                e.kind,
                DictErrorKind::StackOverflow | DictErrorKind::TruncatedOperator
            ) {
                dict_error(r, base, e);
            }
            None
        }
    }
}

/// What the charstrings of one Font DICT need from its Private DICT.
#[derive(Clone, Copy)]
struct Sub<'a> {
    subrs: Index<'a>,
    sorted: bool,
    default_width_x: i64,
    nominal_width_x: i64,
    vsindex: u32,
    seed: u32,
}

impl<'a> Sub<'a> {
    fn new(dict: &PrivateDict, subrs: Index<'a>) -> Sub<'a> {
        Sub {
            subrs,
            sorted: subrs.ft_offsets_sorted(),
            default_width_x: dict.default_width_x,
            nominal_width_x: dict.nominal_width_x,
            vsindex: dict.vsindex,
            seed: u32::try_from(dict.initial_random_seed & 0xFFFF_FFFF).unwrap_or(0),
        }
    }
}

fn private<'a>(
    r: &mut Reporter<'_>,
    data: &'a [u8],
    size: u64,
    offset: u64,
    version: Version,
    blend: &dyn BlendWeights,
    fd: Option<u32>,
) -> Option<Sub<'a>> {
    if size == 0 || offset == 0 {
        if let Some(fd) = fd
            && size == 0
            && offset == 0
        {
            r.emit(0, Kind::MissingPrivate { fd });
        }
        return Some(Sub::new(&PrivateDict::default(), Index::empty(version)));
    }
    let at = usize::try_from(offset).unwrap_or(usize::MAX);
    let len = usize::try_from(size).unwrap_or(usize::MAX);
    let Some(bytes) = be::slice_at(data, at, len) else {
        r.emit(
            at,
            Kind::OffsetOutOfBounds {
                offset: to_u32(at),
                length: to_u32(len),
                available: to_u32(data.len()),
            },
        );
        return None;
    };
    let kind = match version {
        Version::Cff => DictKind::Private,
        Version::Cff2 => DictKind::Private2,
    };
    dict_checks(r, bytes, at, kind);
    let dict = match PrivateDict::parse(bytes, version, blend) {
        Ok(d) => d,
        Err(e) => {
            if !matches!(
                e.kind,
                DictErrorKind::StackOverflow | DictErrorKind::TruncatedOperator
            ) {
                dict_error(r, at, e);
            }
            return None;
        }
    };
    let subrs = if dict.subrs != 0 {
        let s = at.saturating_add(usize::try_from(dict.subrs).unwrap_or(usize::MAX));
        index(r, data, s, version)?
    } else {
        Index::empty(version)
    };
    Some(Sub::new(&dict, subrs))
}

fn fd_select<'a>(
    r: &mut Reporter<'_>,
    data: &'a [u8],
    at: usize,
    num_glyphs: u32,
    fds: u32,
) -> Option<FdSelect<'a>> {
    let s = match FdSelect::new(data, at, num_glyphs) {
        Ok(s) => s,
        Err(ReadError::InvalidValue) => {
            r.emit(at, Kind::EmptyRanges);
            return None;
        }
        Err(e) => {
            r.read_error(at, e, data.len());
            return None;
        }
    };
    let mut ok = true;
    match s {
        FdSelect::Format0(d) => {
            for (i, &fd) in d.iter().enumerate() {
                if u32::from(fd) >= fds {
                    r.emit(
                        at.saturating_add(1).saturating_add(i),
                        Kind::FdOutOfRange {
                            fd: u32::from(fd),
                            count: fds,
                        },
                    );
                    ok = false;
                    break;
                }
            }
        }
        FdSelect::Format3(_) | FdSelect::Format4(_) => {
            if s.format() == 4 {
                r.emit(at, Kind::FdSelectFormat4);
            }
            let n = s.len();
            let (first, _) = s.range(0).unwrap_or((0, 0));
            if first != 0 {
                r.emit(at, Kind::FirstRangeNotZero { found: first });
                ok = false;
            }
            let mut previous: Option<u32> = None;
            for i in 0..n {
                let (start, fd) = s.range(i).unwrap_or((0, 0));
                if previous.is_some_and(|p| start <= p) {
                    r.emit(at, Kind::RangesNotSorted { index: to_u32(i) });
                    ok = false;
                    break;
                }
                previous = Some(start);
                if u32::from(fd) >= fds {
                    r.emit(
                        at,
                        Kind::FdOutOfRange {
                            fd: u32::from(fd),
                            count: fds,
                        },
                    );
                    ok = false;
                    break;
                }
            }
            let (sentinel, _) = s.range(n).unwrap_or((0, 0));
            if sentinel != num_glyphs {
                r.emit(
                    at,
                    Kind::SentinelMismatch {
                        found: sentinel,
                        expected: num_glyphs,
                    },
                );
                ok = false;
            }
            if previous.is_some_and(|p| sentinel <= p) {
                r.emit(at, Kind::RangesNotSorted { index: to_u32(n) });
                ok = false;
            }
        }
    }
    ok.then_some(s)
}

fn char_string_error(glyph: u32, position: usize, kind: CharStringErrorKind) -> Kind {
    let position = to_u32(position);
    use CharStringErrorKind as E;
    match kind {
        E::StackOverflow => Kind::CharStringStackOverflow { glyph, position },
        E::StackUnderflow => Kind::CharStringStackUnderflow { glyph, position },
        E::SubroutineOutOfRange => Kind::SubroutineOutOfRange { glyph, position },
        E::SubroutineTooDeep => Kind::SubroutineTooDeep { glyph, position },
        E::Truncated => Kind::CharStringTruncated { glyph, position },
        E::VsindexOutOfRange => Kind::VsindexOutOfRange { glyph, position },
        E::BlendWithoutStore => Kind::BlendWithoutStore { glyph, position },
        E::TypeMismatch
        | E::ReturnOutsideSubroutine
        | E::TooManyHints
        | E::NestedSeac
        | E::SeacComponentMissing
        | E::VsindexAfterBlend
        | E::InstructionLimit => Kind::InvalidCharString { glyph, position },
    }
}

/// What the charstring check needs of one font.
struct Glyphs<'a, 'b> {
    version: Version,
    char_strings: Index<'a>,
    char_strings_at: usize,
    charset: Option<Charset<'a>>,
    global_subrs: Index<'a>,
    top: &'b TopDict,
    privates: &'b [Option<Sub<'a>>],
    fd_select: Option<FdSelect<'a>>,
    blend: &'b dyn BlendWeights,
    has_variations: bool,
}

fn char_strings(r: &mut Reporter<'_>, g: &Glyphs<'_, '_>) {
    let mut work = Work::default();
    let global_sorted = g.global_subrs.ft_offsets_sorted();
    for glyph in 0..g.char_strings.len() {
        let fd = g.fd_select.map_or(0, |s| s.fd(glyph));
        let Some(Some(p)) = g.privates.get(usize::from(fd)) else {
            continue;
        };
        let ctx = Context {
            version: g.version,
            char_strings: g.char_strings,
            charset: g.charset,
            global_subrs: g.global_subrs,
            global_subrs_sorted: global_sorted,
            local_subrs: p.subrs,
            local_subrs_sorted: p.sorted,
            charstring_type: g.top.charstring_type,
            default_width_x: p.default_width_x,
            nominal_width_x: p.nominal_width_x,
            max_stack: g.top.maxstack,
            has_variations: g.has_variations,
            vsindex: p.vsindex,
            blend: g.blend,
            random_seed: p.seed,
            scale: 0x400,
        };
        let data = g.char_strings.get(glyph).unwrap_or_default();
        let base = g
            .char_strings_at
            .saturating_add(g.char_strings.data_offset())
            .saturating_add(to_usize(
                g.char_strings.offset(glyph).unwrap_or(1).saturating_sub(1),
            ));
        match charstring::decode(data, &ctx, &mut work, &mut NullSink) {
            Ok(_) => {
                if work.ended_without_endchar() {
                    r.emit(
                        base.saturating_add(data.len()),
                        Kind::MissingEndchar { glyph },
                    );
                }
            }
            Err(e) => {
                let at = if e.depth == 0 {
                    base.saturating_add(e.position)
                } else {
                    base
                };
                r.emit(at, char_string_error(glyph, e.position, e.kind));
            }
        }
        if let Some((position, operator)) = work.unknown_operator() {
            r.emit(
                base,
                Kind::UnknownCharStringOperator {
                    glyph,
                    position: to_u32(position),
                    operator: u32::from(operator),
                },
            );
        }
    }
}

fn glyph_count(r: &mut Reporter<'_>, at: usize, cs: &Index<'_>, num_glyphs: Option<u32>) -> bool {
    if cs.is_empty() {
        r.emit(at, Kind::NoGlyphs);
        return false;
    }
    if let Some(n) = num_glyphs
        && n != cs.len()
    {
        r.emit(
            at,
            Kind::GlyphCountMismatch {
                found: cs.len(),
                expected: n,
            },
        );
    }
    true
}

fn offset(v: u64) -> usize {
    usize::try_from(v).unwrap_or(usize::MAX)
}

/// Validates a `CFF ` table (version 1) of a sfnt font with `num_glyphs`
/// glyphs in `maxp` (ADR 0021): the header, the INDEXes, the DICTs, the
/// charset, the encoding, the FDArray and FDSelect, the subroutines and
/// every charstring (run through the decoder without drawing). A table
/// without errors can be opened by FreeType, unless its FDSelect has
/// format 4 (a warning).
pub fn validate(data: &[u8], num_glyphs: Option<u32>, report: &mut dyn FnMut(Diagnostic)) {
    validate_with_mode(data, num_glyphs, false, report);
}

/// Validates every face in a bare CFF 1 font set without an sfnt `maxp`.
pub fn validate_bare(data: &[u8], report: &mut dyn FnMut(Diagnostic)) {
    validate_with_mode(data, None, true, report);
}

fn validate_with_mode(
    data: &[u8],
    num_glyphs: Option<u32>,
    bare: bool,
    report: &mut dyn FnMut(Diagnostic),
) {
    let mut r = Reporter {
        tag: TAG,
        report,
        errors: false,
    };
    let header = match Header::new(data) {
        Ok(h) => h,
        Err(e) => return r.read_error(0, e, data.len()),
    };
    if header.major != 1 {
        return r.emit(
            0,
            Kind::UnknownVersion {
                found: u32::from(header.major),
            },
        );
    }
    if header.header_size < 4 {
        return r.emit(
            2,
            Kind::HeaderSizeTooSmall {
                found: u32::from(header.header_size),
                minimum: 4,
            },
        );
    }
    if !(1..=4).contains(&header.off_size) {
        r.emit(
            3,
            Kind::InvalidOffSize {
                found: u32::from(header.off_size),
            },
        );
    }
    let v = Version::Cff;
    let names_at = usize::from(header.header_size);
    let Some(names) = index(&mut r, data, names_at, v) else {
        return;
    };
    let top_at = names_at.saturating_add(names.end().unwrap_or(0));
    let Some(tops) = index(&mut r, data, top_at, v) else {
        return;
    };
    if !bare && names.len() > 1 {
        r.emit(names_at, Kind::MultipleNames { count: names.len() });
    }
    if tops.len() < names.len().max(1) {
        r.emit(
            top_at,
            Kind::TooFewTopDicts {
                found: tops.len(),
                expected: names.len().max(1),
            },
        );
    }
    let strings_at = top_at.saturating_add(tops.end().unwrap_or(0));
    let Some(strings) = index(&mut r, data, strings_at, v) else {
        return;
    };
    let gsubrs_at = strings_at.saturating_add(strings.end().unwrap_or(0));
    let Some(global_subrs) = index(&mut r, data, gsubrs_at, v) else {
        return;
    };
    let context = BareFaceValidation {
        data,
        version: v,
        tops,
        top_at,
        strings,
        global_subrs,
        num_glyphs,
    };
    let count = if bare { names.len() } else { 1 };
    for face_index in 0..count {
        validate_one(&mut r, &context, face_index);
    }
}

#[derive(Clone, Copy)]
struct BareFaceValidation<'a> {
    data: &'a [u8],
    version: Version,
    tops: Index<'a>,
    top_at: usize,
    strings: Index<'a>,
    global_subrs: Index<'a>,
    num_glyphs: Option<u32>,
}

fn validate_one(r: &mut Reporter<'_>, context: &BareFaceValidation<'_>, face_index: u32) {
    let BareFaceValidation {
        data,
        version: v,
        tops,
        top_at,
        strings,
        global_subrs,
        num_glyphs,
    } = *context;
    let Some(top_data) = tops.get(face_index) else {
        return;
    };
    let top_base = top_at
        .saturating_add(tops.data_offset())
        .saturating_add(to_usize(
            tops.offset(face_index).unwrap_or(1).saturating_sub(1),
        ));
    let Some(top) = top_dict(r, top_data, top_base, v) else {
        return;
    };
    if top.charstring_type != 2 {
        r.emit(
            top_base,
            Kind::UnsupportedCharstringType {
                found: top.charstring_type.cast_unsigned(),
            },
        );
    }
    if top.char_strings == 0 {
        return r.emit(top_base, Kind::MissingCharStrings);
    }
    let cs_at = offset(top.char_strings);
    let Some(cs) = index(r, data, cs_at, v) else {
        return;
    };
    if !glyph_count(r, cs_at, &cs, num_glyphs) {
        return;
    }
    let n = cs.len();
    let charset = match Charset::new(data, top.charset, n) {
        Ok(c) => Some(c),
        Err(ReadError::InvalidValue) => {
            let max = [229u32, 166, 87]
                .get(offset(top.charset))
                .copied()
                .unwrap_or(0);
            r.emit(top_base, Kind::PredefinedCharsetTooShort { glyphs: n, max });
            None
        }
        Err(e) => {
            r.read_error(offset(top.charset), e, data.len());
            None
        }
    };
    if !top.is_cid()
        && let Some(c) = charset
    {
        let limit = super::NUM_STANDARD_STRINGS.saturating_add(strings.len());
        if let Some((glyph, sid)) = (0u32..).zip(c.sids()).find(|&(_, s)| u32::from(s) >= limit) {
            r.emit(
                offset(top.charset),
                Kind::SidOutOfRange {
                    glyph,
                    sid: u32::from(sid),
                },
            );
        }
        if let Err(e) = Encoding::new(data, top.encoding, n) {
            r.read_error(offset(top.encoding), e, data.len());
        }
    }
    let mut privates: [Option<Sub<'_>>; 256] = [None; 256];
    let mut fd_select_view = None;
    if top.is_cid() {
        if top.fd_array == 0 {
            return r.emit(top_base, Kind::MissingFdArray);
        }
        let fda_at = offset(top.fd_array);
        let Some(fda) = index(r, data, fda_at, v) else {
            return;
        };
        if fda.len() > 256 {
            return r.emit(fda_at, Kind::TooManyFontDicts { found: fda.len() });
        }
        let fds = fda.len();
        for (i, slot) in (0..fda.len()).zip(privates.iter_mut()) {
            let d = fda.get(i).unwrap_or_default();
            let base = fda_at
                .saturating_add(fda.data_offset())
                .saturating_add(to_usize(fda.offset(i).unwrap_or(1).saturating_sub(1)));
            dict_checks(r, d, base, DictKind::Top);
            match FontDict::parse(d, v) {
                Ok(f) if f.cid_registry == dict::NO_SID => {
                    *slot = private(
                        r,
                        data,
                        f.private_size,
                        f.private_offset,
                        v,
                        &NoBlend,
                        Some(i),
                    );
                }
                Ok(_) => *slot = Some(Sub::new(&PrivateDict::default(), Index::empty(v))),
                Err(e) => dict_error(r, base, e),
            }
        }
        fd_select_view = fd_select(r, data, offset(top.fd_select), n, fds);
        if fd_select_view.is_none() {
            return;
        }
    } else if let Some(slot) = privates.get_mut(0) {
        *slot = private(
            r,
            data,
            top.private_size,
            top.private_offset,
            v,
            &NoBlend,
            None,
        );
    }
    char_strings(
        r,
        &Glyphs {
            version: v,
            char_strings: cs,
            char_strings_at: cs_at,
            charset,
            global_subrs,
            top: &top,
            privates: &privates,
            fd_select: fd_select_view,
            blend: &NoBlend,
            has_variations: false,
        },
    );
}

/// Validates a `CFF2` table of a sfnt font with `num_glyphs` glyphs in
/// `maxp` and `fvar_axes` axes in `fvar` (ADR 0021): as [`validate`],
/// plus the Variation Store, whose regions must have the axes of `fvar`.
pub fn validate2(
    data: &[u8],
    num_glyphs: Option<u32>,
    fvar_axes: Option<u16>,
    report: &mut dyn FnMut(Diagnostic),
) {
    let mut r = Reporter {
        tag: TAG2,
        report,
        errors: false,
    };
    let header = match Header::new(data) {
        Ok(h) => h,
        Err(e) => return r.read_error(0, e, data.len()),
    };
    if header.major != 2 {
        return r.emit(
            0,
            Kind::UnknownVersion {
                found: u32::from(header.major),
            },
        );
    }
    if header.header_size < 5 {
        return r.emit(
            2,
            Kind::HeaderSizeTooSmall {
                found: u32::from(header.header_size),
                minimum: 5,
            },
        );
    }
    let v = Version::Cff2;
    let top_at = usize::from(header.header_size);
    let len = usize::from(header.top_dict_length);
    let Some(top_data) = be::slice_at(data, top_at, len) else {
        return r.emit(
            3,
            Kind::OffsetOutOfBounds {
                offset: to_u32(top_at),
                length: to_u32(len),
                available: to_u32(data.len()),
            },
        );
    };
    let gsubrs_at = top_at.saturating_add(len);
    let Some(global_subrs) = index(&mut r, data, gsubrs_at, v) else {
        return;
    };
    let Some(top) = top_dict(&mut r, top_data, top_at, v) else {
        return;
    };
    if top.char_strings == 0 {
        return r.emit(top_at, Kind::MissingCharStrings);
    }
    let cs_at = offset(top.char_strings);
    let Some(cs) = index(&mut r, data, cs_at, v) else {
        return;
    };
    if !glyph_count(&mut r, cs_at, &cs, num_glyphs) {
        return;
    }
    let n = cs.len();
    let mut store = None;
    if top.vstore != 0 {
        let at = offset(top.vstore).saturating_add(2);
        match data.get(at..) {
            Some(d) => {
                let before = r.errors;
                r.errors = false;
                item_variation::validate(d, TAG2, to_u32(at), fvar_axes, &mut |d| {
                    r.errors |= d.severity == crate::Severity::Error;
                    (r.report)(d);
                });
                if !r.errors {
                    store = ItemVariationStore::new(d).ok();
                }
                r.errors |= before;
            }
            None => r.emit(
                at,
                Kind::OffsetOutOfBounds {
                    offset: to_u32(at),
                    length: 8,
                    available: to_u32(data.len()),
                },
            ),
        }
    }
    let blend = super::FtBlend::new(store, &[]);
    if top.fd_array == 0 {
        return r.emit(top_at, Kind::MissingFdArray);
    }
    let fda_at = offset(top.fd_array);
    let Some(fda) = index(&mut r, data, fda_at, v) else {
        return;
    };
    if fda.len() > 256 {
        return r.emit(fda_at, Kind::TooManyFontDicts { found: fda.len() });
    }
    let mut privates: [Option<Sub<'_>>; 256] = [None; 256];
    for (i, slot) in (0..fda.len()).zip(privates.iter_mut()) {
        let d = fda.get(i).unwrap_or_default();
        let base = fda_at
            .saturating_add(fda.data_offset())
            .saturating_add(to_usize(fda.offset(i).unwrap_or(1).saturating_sub(1)));
        dict_checks(&mut r, d, base, DictKind::Font2);
        match FontDict::parse(d, v) {
            Ok(f) => {
                *slot = private(
                    &mut r,
                    data,
                    f.private_size,
                    f.private_offset,
                    v,
                    &blend,
                    Some(i),
                );
            }
            Err(e) => dict_error(&mut r, base, e),
        }
    }
    let mut fd_select_view = None;
    if fda.len() > 1 {
        fd_select_view = fd_select(&mut r, data, offset(top.fd_select), n, fda.len());
        if fd_select_view.is_none() {
            return;
        }
    }
    let has_variations = store.is_some_and(|s| s.data_count() != 0);
    char_strings(
        &mut r,
        &Glyphs {
            version: v,
            char_strings: cs,
            char_strings_at: cs_at,
            charset: None,
            global_subrs,
            top: &top,
            privates: &privates,
            fd_select: fd_select_view,
            blend: &blend,
            has_variations,
        },
    );
}
