//! Owned container bytes and opened legacy faces (root ADR 0100).

use alloc::{boxed::Box, vec::Vec};
use core::fmt;
use sofel_read::{
    cff::Cff1,
    sfnt::{FontFile, Sfnt},
};
use sofel_type1::type42::Type42Source;

use crate::{
    BareCffFace, BitmapFace, BitmapFormat, CidFace, Face, Font, PfrFace, Type1Face, Type1Source,
    type42::Type42Face,
};

const MAX_FONT_BYTES: usize = 0x7fff_ffff;

/// Why a common face cannot be opened or queried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaceError {
    /// Input or decoded font is malformed.
    Invalid,
    /// Logical face number is outside the file.
    IndexOutOfRange,
    /// Glyph number is outside the face.
    InvalidGlyph,
    /// Requested strike does not exist at the requested size.
    InvalidStrike,
    /// Size is zero or cannot be represented.
    InvalidSize,
    /// A metric or scale cannot be represented in 26.6 pixels.
    Overflow,
    /// The face has no such operation.
    Unsupported,
}

impl fmt::Display for FaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Invalid => "invalid font",
            Self::IndexOutOfRange => "face index out of range",
            Self::InvalidGlyph => "glyph index out of range",
            Self::InvalidStrike => "strike does not match size",
            Self::InvalidSize => "invalid pixel size",
            Self::Overflow => "metric overflow",
            Self::Unsupported => "unsupported face operation",
        })
    }
}
impl core::error::Error for FaceError {}

#[derive(Clone, Debug)]
enum Source {
    Bytes,
    Type1(Type1Source),
    Type42(Box<Type42Source>),
}

/// Owned bytes after container decompression, with owned PostScript source where needed.
#[derive(Clone, Debug)]
pub struct FaceData {
    bytes: Vec<u8>,
    source: Source,
}

impl FaceData {
    /// Copies an input font, expanding its outer compression and web-font container.
    pub fn new(bytes: &[u8]) -> Result<Self, FaceError> {
        if bytes.len() > MAX_FONT_BYTES {
            return Err(FaceError::Overflow);
        }
        let mut bytes = bytes.to_vec();
        bytes = if bytes.starts_with(&[31, 139]) {
            sofel_inflate::gzip(&bytes, MAX_FONT_BYTES).map_err(|_| FaceError::Invalid)?
        } else if bytes.starts_with(&[31, 157]) {
            sofel_lzw::decode(&bytes, MAX_FONT_BYTES).map_err(|_| FaceError::Invalid)?
        } else if bytes.starts_with(b"BZh") {
            sofel_bzip2::decode(&bytes, MAX_FONT_BYTES).map_err(|_| FaceError::Invalid)?
        } else {
            bytes
        };
        bytes =
            match bytes.get(..4) {
                Some(b"wOFF") => sofel_woff::decode_woff(&bytes, MAX_FONT_BYTES)
                    .map_err(|_| FaceError::Invalid)?,
                Some(b"wOF2") => sofel_woff::decode_woff2(&bytes, MAX_FONT_BYTES)
                    .map_err(|_| FaceError::Invalid)?,
                _ => bytes,
            };
        let source = if bytes.starts_with(b"%!PS-TrueTypeFont") {
            Source::Type42(Box::new(
                Type42Source::parse(&bytes, MAX_FONT_BYTES).map_err(|_| FaceError::Invalid)?,
            ))
        } else if Type1Source::has_text_header(&bytes) {
            Source::Type1(
                Type1Source::parse_pfa(&bytes, MAX_FONT_BYTES).map_err(|_| FaceError::Invalid)?,
            )
        } else if bytes.starts_with(&[0x80, 0x01]) {
            Source::Type1(
                Type1Source::parse_pfb(&bytes, MAX_FONT_BYTES).map_err(|_| FaceError::Invalid)?,
            )
        } else {
            Source::Bytes
        };
        Ok(Self { bytes, source })
    }

    /// Number of logical faces that can be selected in the file.
    pub fn face_count(&self) -> Result<u32, FaceError> {
        match &self.source {
            Source::Type1(_) => Ok(1),
            Source::Type42(source) => {
                match FontFile::new(source.sfnt()).map_err(|_| FaceError::Invalid)? {
                    FontFile::Sfnt(_) => Ok(1),
                    FontFile::Collection(c) => Ok(c.num_fonts()),
                    FontFile::Dfont(d) => Ok(d.num_fonts()),
                }
            }
            Source::Bytes => self.bytes_face_count(),
        }
    }

    fn bytes_face_count(&self) -> Result<u32, FaceError> {
        if let Some(format) = BitmapFormat::detect(&self.bytes) {
            return Ok(format
                .open(&self.bytes, 0)
                .map_err(|_| FaceError::Invalid)?
                .face_count());
        }
        if self.bytes.starts_with(b"PFR0") {
            return Ok(u32::from(
                Font::from_pfr(&self.bytes, 0)
                    .map_err(|_| FaceError::Invalid)?
                    .face_count(),
            ));
        }
        if self.bytes.starts_with(b"%!PS-Adobe-3.0 Resource-CIDFont") {
            return Ok(1);
        }
        if self.bytes.first() == Some(&1) {
            return Ok(Cff1::new(&self.bytes)
                .map_err(|_| FaceError::Invalid)?
                .names()
                .len());
        }
        match FontFile::new(&self.bytes).map_err(|_| FaceError::Invalid)? {
            FontFile::Sfnt(s) if sofel_read::cff::is_cef(&s) => Ok(1),
            FontFile::Sfnt(_) => Ok(1),
            FontFile::Collection(c) => Ok(c.num_fonts()),
            FontFile::Dfont(d) => Ok(d.num_fonts()),
        }
    }

    /// Opens one logical face while this data remains alive.
    pub fn open(&self, index: u32) -> Result<OpenedFace<'_>, FaceError> {
        if index >= self.face_count()? {
            return Err(FaceError::IndexOutOfRange);
        }
        match &self.source {
            Source::Type1(source) => Type1Face::new(source)
                .map(|f| OpenedFace::Type1(Box::new(f)))
                .map_err(|_| FaceError::Invalid),
            Source::Type42(source) => Font::from_type42(source, index)
                .map(|f| OpenedFace::Type42(Box::new(f)))
                .map_err(|_| FaceError::Invalid),
            Source::Bytes => self.open_bytes(index),
        }
    }

    fn open_bytes(&self, index: u32) -> Result<OpenedFace<'_>, FaceError> {
        if let Some(format) = BitmapFormat::detect(&self.bytes) {
            return format
                .open(&self.bytes, index)
                .map(|f| OpenedFace::Bitmap(Box::new(f)))
                .map_err(|_| FaceError::Invalid);
        }
        if self.bytes.starts_with(b"PFR0") {
            return Font::from_pfr(&self.bytes, index)
                .map(|f| OpenedFace::Pfr(Box::new(f)))
                .map_err(|_| FaceError::Invalid);
        }
        if self.bytes.starts_with(b"%!PS-Adobe-3.0 Resource-CIDFont") {
            return CidFace::parse(&self.bytes, MAX_FONT_BYTES)
                .map(|f| OpenedFace::Cid(Box::new(f)))
                .map_err(|_| FaceError::Invalid);
        }
        if self.bytes.first() == Some(&1)
            || Sfnt::new(&self.bytes).is_ok_and(|s| sofel_read::cff::is_cef(&s))
        {
            return BareCffFace::new(&self.bytes, index)
                .map(|f| OpenedFace::BareCff(Box::new(f)))
                .map_err(|_| FaceError::Invalid);
        }
        Font::new(&self.bytes, index)
            .map(|f| OpenedFace::Sfnt(Box::new(f)))
            .map_err(|_| FaceError::Invalid)
    }
}

/// A parsed face, borrowing only the separately owned [`FaceData`].
#[derive(Debug)]
pub enum OpenedFace<'a> {
    /// An sfnt face.
    Sfnt(Box<Font<'a>>),
    /// A bare CFF or CEF face.
    BareCff(Box<BareCffFace<'a>>),
    /// A PFA or PFB face.
    Type1(Box<Type1Face<'a>>),
    /// A CID-keyed Type 1 face.
    Cid(Box<CidFace>),
    /// A Type 42 face.
    Type42(Box<Type42Face<'a>>),
    /// A PFR face.
    Pfr(Box<PfrFace<'a>>),
    /// A fixed bitmap face.
    Bitmap(Box<BitmapFace>),
}

impl<'a> OpenedFace<'a> {
    /// Replaces Type 1 attachment with borrowed AFM metrics.
    pub fn attach_afm(&mut self, data: &'a [u8]) -> Result<(), FaceError> {
        match self {
            Self::Type1(face) => face.attach_afm(data).map_err(|_| FaceError::Invalid),
            _ => Err(FaceError::Unsupported),
        }
    }

    /// Replaces Type 1 attachment with parsed PFM metrics.
    pub fn attach_pfm(&mut self, data: &[u8]) -> Result<(), FaceError> {
        match self {
            Self::Type1(face) => face.attach_pfm(data).map_err(|_| FaceError::Invalid),
            _ => Err(FaceError::Unsupported),
        }
    }

    /// Borrows the format-independent face operations.
    pub fn as_face(&self) -> Face<'_> {
        match self {
            Self::Sfnt(f) => Face::Sfnt(f),
            Self::BareCff(f) => Face::BareCff(f),
            Self::Type1(f) => Face::Type1(f),
            Self::Cid(f) => Face::Cid(f),
            Self::Type42(f) => Face::Type42(f),
            Self::Pfr(f) => Face::Pfr(f),
            Self::Bitmap(f) => Face::Bitmap(f),
        }
    }
}
