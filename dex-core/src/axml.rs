// Modified by Zero (2026-11-01): new module (fork M1b); raw string index first (2026-11-02, /review). See NOTICE.
//! Android binary XML (AXML), as stored in an APK's `AndroidManifest.xml`.

use serde::Serialize;

use crate::error::{DexError, DexResult};

/// A typed attribute value. References stay references: resolving them needs `resources.arsc`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum XmlValue {
    /// No value.
    Null,
    /// A string.
    String(String),
    /// A decimal integer.
    Int(i32),
    /// A hexadecimal integer.
    Hex(u32),
    /// A boolean.
    Bool(bool),
    /// A resource reference (`@...`), unresolved.
    Reference(u32),
    /// A theme attribute reference (`?...`), unresolved.
    AttributeRef(u32),
    /// Any other `Res_value` type (float, dimension, colour, ...), raw.
    Other {
        /// `Res_value::dataType`.
        data_type: u8,
        /// `Res_value::data`.
        data: u32,
    },
}

/// One attribute of an element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct XmlAttribute {
    /// Namespace URI, if any.
    pub namespace: Option<String>,
    /// Attribute name from the string pool (may be empty after resource shrinking).
    pub name: String,
    /// Framework or app resource id of the attribute name, from the resource map.
    pub resource_id: Option<u32>,
    /// The value.
    pub value: XmlValue,
}

/// One element and its children.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct XmlElement {
    /// Namespace URI, if any.
    pub namespace: Option<String>,
    /// Element name.
    pub name: String,
    /// Attributes in document order.
    pub attributes: Vec<XmlAttribute>,
    /// Child elements in document order.
    pub children: Vec<XmlElement>,
}

/// Elements may nest at most this deep. Real manifests nest about five levels.
pub const MAX_DEPTH: usize = 256;

/// At most this many bytes of strings are copied into the tree. String-pool offsets may all
/// point at one long string, so without a budget a small file could decode to gigabytes.
pub const MAX_DECODED_STRING_BYTES: usize = 32 * 1024 * 1024;

const RES_XML_TYPE: u16 = 0x0003;
const RES_STRING_POOL_TYPE: u16 = 0x0001;
const RES_XML_RESOURCE_MAP_TYPE: u16 = 0x0180;
const RES_XML_START_ELEMENT_TYPE: u16 = 0x0102;
const RES_XML_END_ELEMENT_TYPE: u16 = 0x0103;
const UTF8_FLAG: u32 = 1 << 8;
const NO_INDEX: u32 = 0xffff_ffff;
const CTX: &str = "binary XML";

fn malformed(message: &'static str) -> DexError {
    DexError::Malformed {
        context: CTX,
        message,
    }
}

fn u16_at(b: &[u8], off: usize) -> DexResult<u16> {
    b.get(
        off..off
            .checked_add(2)
            .ok_or_else(|| malformed("offset overflow"))?,
    )
    .map(|s| u16::from_le_bytes([s[0], s[1]]))
    .ok_or_else(|| malformed("truncated"))
}

fn u32_at(b: &[u8], off: usize) -> DexResult<u32> {
    b.get(
        off..off
            .checked_add(4)
            .ok_or_else(|| malformed("offset overflow"))?,
    )
    .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    .ok_or_else(|| malformed("truncated"))
}

/// A string pool, decoded lazily: only strings the tree uses are decoded.
struct StringPool<'a> {
    /// The pool chunk's bytes.
    chunk: &'a [u8],
    offsets: Vec<u32>,
    strings_start: usize,
    utf8: bool,
    cache: Vec<Option<Option<String>>>,
}

impl<'a> StringPool<'a> {
    fn parse(chunk: &'a [u8], header_size: usize) -> DexResult<Self> {
        let count = u32_at(chunk, 8)? as usize;
        let flags = u32_at(chunk, 16)?;
        let strings_start = u32_at(chunk, 20)? as usize;
        let table_end = count
            .checked_mul(4)
            .and_then(|n| n.checked_add(header_size))
            .ok_or_else(|| malformed("string count overflow"))?;
        if table_end > chunk.len() {
            return Err(malformed("string offsets exceed the pool"));
        }
        let mut offsets = Vec::with_capacity(count);
        for i in 0..count {
            offsets.push(u32_at(chunk, header_size + 4 * i)?);
        }
        Ok(Self {
            chunk,
            offsets,
            strings_start,
            utf8: flags & UTF8_FLAG != 0,
            cache: vec![None; count],
        })
    }

    /// The string at `index`, or `None` if absent or undecodable.
    fn get(&mut self, index: u32) -> Option<String> {
        let i = index as usize;
        if index == NO_INDEX || i >= self.offsets.len() {
            return None;
        }
        if let Some(cached) = &self.cache[i] {
            return cached.clone();
        }
        let decoded = self.decode(self.strings_start.checked_add(self.offsets[i] as usize)?);
        self.cache[i] = Some(decoded.clone());
        decoded
    }

    fn decode(&self, at: usize) -> Option<String> {
        let b = self.chunk;
        if self.utf8 {
            // UTF-16 length (skipped), then UTF-8 byte length; each 1 or 2 bytes.
            let (_, n1) = len8(b, at)?;
            let (len, n2) = len8(b, at + n1)?;
            let start = at + n1 + n2;
            let bytes = b.get(start..start.checked_add(len)?)?;
            Some(String::from_utf8_lossy(bytes).into_owned())
        } else {
            let first = u16_at(b, at).ok()? as usize;
            let (len, header) = if first & 0x8000 != 0 {
                (
                    ((first & 0x7fff) << 16) | u16_at(b, at + 2).ok()? as usize,
                    4,
                )
            } else {
                (first, 2)
            };
            let start = at + header;
            let bytes = b.get(start..start.checked_add(len.checked_mul(2)?)?)?;
            let units: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c))
                .collect();
            Some(String::from_utf16_lossy(&units))
        }
    }
}

fn len8(b: &[u8], at: usize) -> Option<(usize, usize)> {
    let first = *b.get(at)? as usize;
    if first & 0x80 != 0 {
        Some((((first & 0x7f) << 8) | *b.get(at + 1)? as usize, 2))
    } else {
        Some((first, 1))
    }
}

struct Budget(usize);

impl Budget {
    fn take(&mut self, s: Option<String>) -> DexResult<Option<String>> {
        if let Some(v) = &s {
            self.0 = self
                .0
                .checked_sub(v.len())
                .ok_or_else(|| malformed("decoded strings exceed the budget"))?;
        }
        Ok(s)
    }
}

/// Parses an AXML document and returns its root element.
///
/// Unknown chunk types are skipped. Attribute names are resolved through the resource map
/// where it has an entry, so a name emptied by resource shrinking still carries its id.
/// Malformed input returns an error; it never panics.
pub fn parse_axml(bytes: &[u8]) -> DexResult<XmlElement> {
    if u16_at(bytes, 0)? != RES_XML_TYPE {
        return Err(malformed("not a binary XML document"));
    }
    let header_size = u16_at(bytes, 2)? as usize;
    let end = (u32_at(bytes, 4)? as usize).min(bytes.len());
    if header_size < 8 || header_size > end {
        return Err(malformed("bad document header"));
    }

    let mut pool: Option<StringPool<'_>> = None;
    let mut resource_map: Vec<u32> = Vec::new();
    let mut stack: Vec<XmlElement> = Vec::new();
    let mut root: Option<XmlElement> = None;
    let mut budget = Budget(MAX_DECODED_STRING_BYTES);

    let mut off = header_size;
    while off + 8 <= end {
        let ty = u16_at(bytes, off)?;
        let hs = u16_at(bytes, off + 2)? as usize;
        let size = u32_at(bytes, off + 4)? as usize;
        let chunk_end = off
            .checked_add(size)
            .ok_or_else(|| malformed("chunk size overflow"))?;
        if size < 8 || hs < 8 || hs > size || chunk_end > end {
            return Err(malformed("bad chunk header"));
        }
        let chunk = &bytes[off..chunk_end];
        match ty {
            RES_STRING_POOL_TYPE if pool.is_none() => {
                if hs < 28 {
                    return Err(malformed("short string pool header"));
                }
                pool = Some(StringPool::parse(chunk, hs)?);
            }
            RES_XML_RESOURCE_MAP_TYPE => {
                resource_map = chunk[hs..]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|c| u32::from_le_bytes(*c))
                    .collect();
            }
            RES_XML_START_ELEMENT_TYPE => {
                let pool = pool
                    .as_mut()
                    .ok_or_else(|| malformed("element before the string pool"))?;
                if stack.len() >= MAX_DEPTH {
                    return Err(malformed("elements nest too deeply"));
                }
                if root.is_some() && stack.is_empty() {
                    return Err(malformed("more than one root element"));
                }
                stack.push(start_element(chunk, hs, pool, &resource_map, &mut budget)?);
            }
            RES_XML_END_ELEMENT_TYPE => {
                let done = stack
                    .pop()
                    .ok_or_else(|| malformed("end without a start"))?;
                match stack.last_mut() {
                    Some(parent) => parent.children.push(done),
                    None => root = Some(done),
                }
            }
            _ => {} // namespaces, CDATA and unknown chunks carry nothing the model needs
        }
        off = chunk_end;
    }
    if !stack.is_empty() {
        return Err(malformed("unclosed element"));
    }
    root.ok_or_else(|| malformed("no root element"))
}

fn start_element(
    chunk: &[u8],
    hs: usize,
    pool: &mut StringPool<'_>,
    resource_map: &[u32],
    budget: &mut Budget,
) -> DexResult<XmlElement> {
    let ext = hs;
    let namespace = budget.take(pool.get(u32_at(chunk, ext)?))?;
    let name = budget
        .take(pool.get(u32_at(chunk, ext + 4)?))?
        .ok_or_else(|| malformed("element without a name"))?;
    let attr_start = u16_at(chunk, ext + 8)? as usize;
    let attr_size = u16_at(chunk, ext + 10)? as usize;
    let attr_count = u16_at(chunk, ext + 12)? as usize;
    if attr_count > 0 && attr_size < 20 {
        return Err(malformed("attribute record too small"));
    }
    let base = ext + attr_start;
    let last = attr_count
        .checked_mul(attr_size)
        .and_then(|n| n.checked_add(base));
    if last.is_none_or(|l| l > chunk.len()) {
        return Err(malformed("attributes exceed the element chunk"));
    }
    let mut attributes = Vec::with_capacity(attr_count);
    for i in 0..attr_count {
        let a = base + i * attr_size;
        let name_index = u32_at(chunk, a + 4)?;
        let raw = u32_at(chunk, a + 8)?;
        let data_type = *chunk
            .get(a + 15)
            .ok_or_else(|| malformed("truncated attribute"))?;
        let data = u32_at(chunk, a + 16)?;
        let value = match data_type {
            0x00 => XmlValue::Null,
            0x01 => XmlValue::Reference(data),
            0x02 => XmlValue::AttributeRef(data),
            // Android reads string attributes through the raw index
            // (`ResXMLParser::getAttributeStringValue`); the typed index is the fallback.
            0x03 => match budget.take(pool.get(raw).or_else(|| pool.get(data)))? {
                Some(v) => XmlValue::String(v),
                None => XmlValue::Other { data_type, data },
            },
            0x10 => XmlValue::Int(data as i32),
            0x11 => XmlValue::Hex(data),
            0x12 => XmlValue::Bool(data != 0),
            _ => XmlValue::Other { data_type, data },
        };
        attributes.push(XmlAttribute {
            namespace: budget.take(pool.get(u32_at(chunk, a)?))?,
            name: budget.take(pool.get(name_index))?.unwrap_or_default(),
            resource_id: resource_map.get(name_index as usize).copied(),
            value,
        });
    }
    Ok(XmlElement {
        namespace,
        name,
        attributes,
        children: Vec::new(),
    })
}
