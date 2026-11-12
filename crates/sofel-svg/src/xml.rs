//! Index of SVG elements for glyph selection and local fragment references.
use super::{ParseError, attr, attributes};
use alloc::{collections::BTreeMap, vec::Vec};

pub(crate) struct Node<'a> {
    pub name: &'a str,
    pub attrs: Vec<(&'a str, &'a str)>,
    pub parent: Option<usize>,
    end: usize,
}

impl<'a> Node<'a> {
    pub fn attr(&self, name: &str) -> Option<&'a str> {
        attr(&self.attrs, name)
    }

    /// An inline style declaration takes precedence over its presentation attribute.
    pub fn property(&self, name: &str) -> Option<&'a str> {
        self.attr("style")
            .into_iter()
            .flat_map(|style| style.split(';'))
            .rev()
            .filter_map(|entry| entry.split_once(':'))
            .find(|(key, _)| key.trim() == name)
            .map(|(_, value)| value.trim())
            .or_else(|| self.attr(name))
    }
}

pub(crate) struct Document<'a> {
    pub nodes: Vec<Node<'a>>,
    pub ids: BTreeMap<&'a str, usize>,
}

impl<'a> Document<'a> {
    #[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // All slices terminate at ASCII delimiters.
    pub fn parse(data: &'a [u8]) -> Result<Self, ParseError> {
        let mut text = core::str::from_utf8(data).map_err(|_| ParseError)?;
        let mut nodes: Vec<Node<'a>> = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        let mut ids = BTreeMap::new();
        while let Some((_, rest)) = text.split_once('<') {
            if let Some(body) = rest.strip_prefix("!--") {
                text = body.split_once("-->").ok_or(ParseError)?.1;
                continue;
            }
            if let Some(body) = rest.strip_prefix("![CDATA[") {
                text = body.split_once("]]>").ok_or(ParseError)?.1;
                continue;
            }
            if let Some(body) = rest.strip_prefix('?') {
                text = body.split_once("?>").ok_or(ParseError)?.1;
                continue;
            }
            // DTDs and external entities are outside the OT-SVG input subset.
            if rest.starts_with('!') {
                return Err(ParseError);
            }
            let mut quote = None;
            let end = rest
                .bytes()
                .position(|b| {
                    if let Some(q) = quote {
                        if q == b {
                            quote = None;
                        }
                    } else if b == b'\'' || b == b'"' {
                        quote = Some(b);
                    } else if b == b'>' {
                        return true;
                    }
                    false
                })
                .ok_or(ParseError)?;
            let tag = rest[..end].trim();
            text = &rest[end + 1..];
            if let Some(close) = tag.strip_prefix('/') {
                let i = stack.pop().ok_or(ParseError)?;
                if nodes[i].name != close.trim() {
                    return Err(ParseError);
                }
                nodes[i].end = nodes.len();
                continue;
            }
            let parent = stack.last().copied();
            if stack.len() >= 64 || nodes.len() >= 65_536 || (parent.is_none() && !nodes.is_empty())
            {
                return Err(ParseError);
            }
            let content = tag.trim_end_matches('/').trim_end();
            let name = content.split_ascii_whitespace().next().ok_or(ParseError)?;
            let attrs = attributes(content);
            let i = nodes.len();
            if let Some(id) = attr(&attrs, "id")
                && ids.insert(id, i).is_some()
            {
                return Err(ParseError);
            }
            nodes.push(Node {
                name,
                attrs,
                parent,
                end: i + 1,
            });
            if !tag.ends_with('/') {
                stack.push(i);
            }
        }
        if !stack.is_empty() || nodes.first().is_none_or(|n| n.name != "svg") {
            return Err(ParseError);
        }
        Ok(Self { nodes, ids })
    }

    #[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // Indices and subtree ends come from the parser.
    pub fn children(&self, i: usize) -> impl Iterator<Item = usize> + '_ {
        let end = self.nodes[i].end;
        core::iter::successors((i + 1 < end).then_some(i + 1), move |&n| {
            let next = self.nodes[n].end;
            (next < end).then_some(next)
        })
    }
}
