/// The storage kind of one PFB section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionKind {
    /// PostScript text.
    Ascii,
    /// Binary encrypted data.
    Binary,
}

/// One PFB section borrowing its payload from the input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Section<'a> {
    /// The section kind.
    pub kind: SectionKind,
    /// The unframed payload.
    pub data: &'a [u8],
}

/// Read the PFB framing and pass each section to `visit` in file order.
///
/// Fails with the offset of the invalid byte or truncated section. The
/// payloads together may not exceed `max_output` bytes.
pub fn sections<'a>(
    data: &'a [u8],
    max_output: usize,
    visit: &mut dyn FnMut(Section<'a>),
) -> Result<(), usize> {
    let mut at = 0usize;
    let mut total = 0usize;
    let mut count = 0usize;
    loop {
        let header = data
            .get(at..at.checked_add(2).ok_or(data.len())?)
            .ok_or(data.len())?;
        if header.first() != Some(&0x80) {
            return Err(at);
        }
        let kind = *header.get(1).ok_or(data.len())?;
        at = at.checked_add(2).ok_or(data.len())?;
        if kind == 3 {
            return if count == 0 || at != data.len() {
                Err(at)
            } else {
                Ok(())
            };
        }
        let kind = match kind {
            1 => SectionKind::Ascii,
            2 if count != 0 => SectionKind::Binary,
            _ => return Err(at.saturating_sub(1)),
        };
        let size = data
            .get(at..at.checked_add(4).ok_or(data.len())?)
            .ok_or(data.len())?;
        let length = usize::try_from(u32::from_le_bytes(size.try_into().map_err(|_| at)?))
            .map_err(|_| at)?;
        at = at.checked_add(4).ok_or(data.len())?;
        let end = at.checked_add(length).ok_or(data.len())?;
        let payload = data.get(at..end).ok_or(data.len())?;
        total = total.checked_add(length).ok_or(data.len())?;
        if total > max_output {
            return Err(at);
        }
        visit(Section {
            kind,
            data: payload,
        });
        count = count.saturating_add(1);
        at = end;
    }
}

/// The sections of a PFB whose framing [`sections`] accepted, one by one.
#[derive(Clone)]
pub(crate) struct Sections<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Sections<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }
}

impl<'a> Iterator for Sections<'a> {
    type Item = Section<'a>;

    fn next(&mut self) -> Option<Section<'a>> {
        let header = self.data.get(self.at..self.at.checked_add(6)?)?;
        let kind = match header.get(..2)? {
            [0x80, 1] => SectionKind::Ascii,
            [0x80, 2] => SectionKind::Binary,
            _ => return None,
        };
        let length =
            usize::try_from(u32::from_le_bytes(header.get(2..6)?.try_into().ok()?)).ok()?;
        let start = self.at.checked_add(6)?;
        let end = start.checked_add(length)?;
        self.at = end;
        Some(Section {
            kind,
            data: self.data.get(start..end)?,
        })
    }
}
