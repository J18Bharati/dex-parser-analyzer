// Modified by Jatin Bharati (2026-11-01): new test helper (fork M1b); review shapes (2026-11-02). See NOTICE.
//! Builds Android binary XML (AXML) documents for tests, written from the chunk layout in
//! AOSP's ResourceTypes.h. There is no AXML fixture in the repo and third-party APKs are not
//! committed, so tests build the documents they need, in both string-pool encodings.
#![allow(dead_code)]

pub const ANDROID_NS: &str = "http://schemas.android.com/apk/res/android";
const NO_INDEX: u32 = 0xffff_ffff;

#[derive(Clone)]
pub enum V {
    Str(String),
    Int(i32),
    Hex(u32),
    Bool(bool),
    Ref(u32),
    /// A string whose raw index and typed index name different pool strings (raw, typed).
    /// Android reads string attributes through the raw index.
    StrSplit(String, String),
}

#[derive(Clone)]
pub struct A {
    pub android: bool,
    pub name: &'static str,
    pub id: Option<u32>,
    pub value: V,
}

#[derive(Clone)]
pub struct El {
    pub name: &'static str,
    pub attrs: Vec<A>,
    pub children: Vec<El>,
}

pub fn el(name: &'static str, attrs: Vec<A>, children: Vec<El>) -> El {
    El {
        name,
        attrs,
        children,
    }
}

/// An `android:` attribute with its framework resource id.
pub fn aa(name: &'static str, id: u32, value: V) -> A {
    A {
        android: true,
        name,
        id: Some(id),
        value,
    }
}

/// A plain (no namespace, no resource id) attribute, like `package`.
pub fn pa(name: &'static str, value: V) -> A {
    A {
        android: false,
        name,
        id: None,
        value,
    }
}

pub fn s(v: &str) -> V {
    V::Str(v.to_string())
}

#[derive(Clone, Default)]
pub struct Opts {
    /// UTF-8 string pool (aapt2's default) instead of UTF-16.
    pub utf8: bool,
    /// Empty the pool strings of attribute names that have a resource id, as resource
    /// shrinkers and obfuscators do. The parser must still resolve them by id.
    pub strip_names: bool,
    /// Insert a chunk of an unknown type after the resource map.
    pub unknown_chunk: bool,
    /// Emit the root element a second time after the first one closes.
    pub second_root: bool,
    /// Put the string pool after the element chunks instead of before them.
    pub pool_after_body: bool,
}

struct Pool {
    strings: Vec<String>,
}

impl Pool {
    fn index(&mut self, s: &str) -> u32 {
        if let Some(i) = self.strings.iter().position(|x| x == s) {
            return i as u32;
        }
        self.strings.push(s.to_string());
        (self.strings.len() - 1) as u32
    }
}

fn walk<'a>(e: &'a El, out: &mut Vec<&'a A>) {
    out.extend(e.attrs.iter());
    for c in &e.children {
        walk(c, out);
    }
}

pub fn build(root: &El, opts: &Opts) -> Vec<u8> {
    // Attribute names with resource ids come first, so the resource map covers exactly them.
    let mut all = Vec::new();
    walk(root, &mut all);
    let mut id_names: Vec<(&str, u32)> = Vec::new();
    for a in &all {
        if let Some(id) = a.id
            && !id_names.iter().any(|(n, _)| *n == a.name)
        {
            id_names.push((a.name, id));
        }
    }
    let mut pool = Pool {
        strings: id_names.iter().map(|(n, _)| n.to_string()).collect(),
    };
    let ns_uri = pool.index(ANDROID_NS);
    let ns_prefix = pool.index("android");

    let mut body = Vec::new();
    // start namespace
    node(&mut body, 0x0100, &[ns_prefix, ns_uri]);
    emit(root, &mut pool, &id_names, ns_uri, &mut body);
    if opts.second_root {
        emit(root, &mut pool, &id_names, ns_uri, &mut body);
    }
    node(&mut body, 0x0101, &[ns_prefix, ns_uri]);

    let mut strings = pool.strings.clone();
    if opts.strip_names {
        for s in strings.iter_mut().take(id_names.len()) {
            s.clear();
        }
    }
    let mut out = Vec::new();
    if !opts.pool_after_body {
        out.extend(string_pool(&strings, opts.utf8));
    }
    let mut map = Vec::new();
    for (_, id) in &id_names {
        map.extend(id.to_le_bytes());
    }
    out.extend(chunk(0x0180, 8, &map));
    if opts.unknown_chunk {
        out.extend(chunk(0x0777, 8, &[1, 2, 3, 4, 5, 6, 7, 8]));
    }
    out.extend(body);
    if opts.pool_after_body {
        out.extend(string_pool(&strings, opts.utf8));
    }
    let mut doc = Vec::new();
    doc.extend(0x0003u16.to_le_bytes());
    doc.extend(8u16.to_le_bytes());
    doc.extend(((out.len() + 8) as u32).to_le_bytes());
    doc.extend(out);
    doc
}

fn emit(e: &El, pool: &mut Pool, id_names: &[(&str, u32)], ns_uri: u32, out: &mut Vec<u8>) {
    let name = pool.index(e.name);
    let mut ext = Vec::new();
    for w in [NO_INDEX, name] {
        ext.extend(w.to_le_bytes());
    }
    for h in [20u16, 20, e.attrs.len() as u16, 0, 0, 0] {
        ext.extend(h.to_le_bytes());
    }
    for a in &e.attrs {
        let aname = match id_names.iter().position(|(n, _)| *n == a.name) {
            Some(i) if a.id.is_some() => i as u32,
            _ => pool.index(a.name),
        };
        let (raw, ty, data) = match &a.value {
            V::Str(v) => {
                let i = pool.index(v);
                (i, 0x03u8, i)
            }
            V::StrSplit(raw, typed) => (pool.index(raw), 0x03u8, pool.index(typed)),
            V::Int(v) => (NO_INDEX, 0x10, *v as u32),
            V::Hex(v) => (NO_INDEX, 0x11, *v),
            V::Bool(v) => (NO_INDEX, 0x12, if *v { 0xffff_ffff } else { 0 }),
            V::Ref(v) => (NO_INDEX, 0x01, *v),
        };
        let ns = if a.android { ns_uri } else { NO_INDEX };
        for w in [ns, aname, raw] {
            ext.extend(w.to_le_bytes());
        }
        ext.extend(8u16.to_le_bytes());
        ext.push(0);
        ext.push(ty);
        ext.extend(data.to_le_bytes());
    }
    let mut start = Vec::new();
    start.extend(1u32.to_le_bytes());
    start.extend(NO_INDEX.to_le_bytes());
    start.extend(ext);
    out.extend(chunk(0x0102, 16, &start));
    for c in &e.children {
        emit(c, pool, id_names, ns_uri, out);
    }
    node(out, 0x0103, &[NO_INDEX, name]);
}

/// A tree node chunk: line number, comment, then the given extension words.
fn node(out: &mut Vec<u8>, ty: u16, ext: &[u32]) {
    let mut body = Vec::new();
    body.extend(1u32.to_le_bytes());
    body.extend(NO_INDEX.to_le_bytes());
    for w in ext {
        body.extend(w.to_le_bytes());
    }
    out.extend(chunk(ty, 16, &body));
}

/// A chunk whose header is `header_size` bytes: type, header size, total size, then the rest
/// of the header and the body, both in `rest`.
fn chunk(ty: u16, header_size: u16, rest: &[u8]) -> Vec<u8> {
    let mut c = Vec::new();
    c.extend(ty.to_le_bytes());
    c.extend(header_size.to_le_bytes());
    c.extend(((rest.len() + 8) as u32).to_le_bytes());
    c.extend(rest);
    c
}

fn len8(n: usize, out: &mut Vec<u8>) {
    assert!(n < 0x8000, "UTF-8 pool lengths are at most 0x7fff");
    if n < 0x80 {
        out.push(n as u8);
    } else {
        out.push(0x80 | (n >> 8) as u8);
        out.push(n as u8);
    }
}

fn len16(n: usize, out: &mut Vec<u8>) {
    if n < 0x8000 {
        out.extend((n as u16).to_le_bytes());
    } else {
        out.extend((0x8000 | (n >> 16) as u16).to_le_bytes());
        out.extend((n as u16).to_le_bytes());
    }
}

fn string_pool(strings: &[String], utf8: bool) -> Vec<u8> {
    let mut data = Vec::new();
    let mut offsets = Vec::new();
    for s in strings {
        offsets.push(data.len() as u32);
        if utf8 {
            len8(s.encode_utf16().count(), &mut data);
            len8(s.len(), &mut data);
            data.extend(s.as_bytes());
            data.push(0);
        } else {
            let units: Vec<u16> = s.encode_utf16().collect();
            len16(units.len(), &mut data);
            for u in units {
                data.extend(u.to_le_bytes());
            }
            data.extend(0u16.to_le_bytes());
        }
    }
    while data.len() % 4 != 0 {
        data.push(0);
    }
    let count = strings.len() as u32;
    let strings_start = 28 + 4 * count;
    let mut rest = Vec::new();
    for w in [count, 0, if utf8 { 0x100 } else { 0 }, strings_start, 0] {
        rest.extend(w.to_le_bytes());
    }
    for o in offsets {
        rest.extend(o.to_le_bytes());
    }
    rest.extend(data);
    chunk(0x0001, 28, &rest)
}
