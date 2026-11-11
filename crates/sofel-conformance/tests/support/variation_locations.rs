use sofel_font::{
    Font,
    var::{Location, Variation},
};

pub struct Position {
    pub id: String,
    named: Option<u16>,
    variations: Vec<Variation>,
}

impl Position {
    pub fn location<'a>(&self, font: &Font<'a>) -> Location<'a> {
        Location::new(font, self.named, &self.variations)
    }

    pub fn args(&self) -> Vec<String> {
        let mut args = Vec::new();
        if let Some(index) = self.named {
            args.push(format!("--named-instance={index}"));
        }
        if !self.variations.is_empty() {
            let values: Vec<_> = self
                .variations
                .iter()
                .map(|v| format!("{}={}", v.tag, v.value))
                .collect();
            args.push(format!("--variations={}", values.join(",")));
        }
        args
    }
}

pub fn positions(font: &Font<'_>) -> Vec<Position> {
    let mut positions = vec![("default".to_string(), None, Vec::new())];
    for i in 0..font
        .ft_mm_var()
        .map_or(0, |mm| mm.named_styles().count())
        .max(font.named_instances().count())
    {
        positions.push((format!("named:{i}"), Some(u16::try_from(i).unwrap()), Vec::new()));
    }
    for axis in font.var_axes() {
        for (edge, value) in [("min", axis.min), ("max", axis.max)] {
            positions.push((
                format!("{}:{edge}", axis.tag),
                None,
                vec![Variation {
                    tag: axis.tag,
                    value,
                }],
            ));
        }
    }
    let mut state = 1u32;
    for sample in 0..8 {
        let settings = font
            .var_axes()
            .map(|axis| {
                state = state.wrapping_mul(1103515245).wrapping_add(12345);
                let t = ((state >> 16) & 0x7fff) as f32 / 32767.0;
                Variation {
                    tag: axis.tag,
                    value: axis.min + (axis.max - axis.min) * t,
                }
            })
            .collect();
        positions.push((format!("rand:{sample}"), None, settings));
    }
    positions
        .into_iter()
        .map(|(id, named, variations)| Position {
            id,
            named,
            variations,
        })
        .collect()
}
