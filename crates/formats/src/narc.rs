//! NARC archives.
//!
//! Layout: a 16-byte header ("NARC", 0xFFFE, 0x0100, file size, header
//! size 16, 3 sections), then BTAF (member start and end offsets), BTNF
//! (names, kept as they are), GMIF (member bytes, each member aligned to 4
//! with 0xFF). Packing a real archive's own members with its own BTNF gives
//! the archive back byte for byte.

use std::fmt;

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

fn u16_at(d: &[u8], p: usize) -> Result<u16, Error> {
    d.get(p..p + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or_else(|| Error(format!("read past the end at {p:#x}")))
}

fn u32_at(d: &[u8], p: usize) -> Result<u32, Error> {
    d.get(p..p + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| Error(format!("read past the end at {p:#x}")))
}

#[derive(Clone, Debug)]
pub struct Narc {
    pub files: Vec<Vec<u8>>,
    /// The BTNF section as stored (names), so a repack keeps it.
    pub btnf: Vec<u8>,
}

impl Narc {
    pub fn parse(d: &[u8]) -> Result<Narc, Error> {
        if d.get(0..4) != Some(b"NARC") {
            return Err(Error("not a NARC".into()));
        }
        let mut off = u16_at(d, 0x0C)? as usize;
        if d.get(off..off + 4) != Some(b"BTAF") {
            return Err(Error("BTAF missing".into()));
        }
        let btaf_size = u32_at(d, off + 4)? as usize;
        let count = u16_at(d, off + 8)? as usize;
        let mut spans = Vec::with_capacity(count);
        for i in 0..count {
            let s = u32_at(d, off + 12 + 8 * i)? as usize;
            let e = u32_at(d, off + 16 + 8 * i)? as usize;
            spans.push((s, e));
        }
        off += btaf_size;
        if d.get(off..off + 4) != Some(b"BTNF") {
            return Err(Error("BTNF missing".into()));
        }
        let btnf_size = u32_at(d, off + 4)? as usize;
        let btnf = d
            .get(off..off + btnf_size)
            .ok_or_else(|| Error("BTNF past the end".into()))?
            .to_vec();
        off += btnf_size;
        if d.get(off..off + 4) != Some(b"GMIF") {
            return Err(Error("GMIF missing".into()));
        }
        let base = off + 8;
        let mut files = Vec::with_capacity(count);
        for (s, e) in spans {
            let m = d
                .get(base + s..base + e)
                .ok_or_else(|| Error(format!("member {s:#x}..{e:#x} past the end")))?;
            files.push(m.to_vec());
        }
        Ok(Narc { files, btnf })
    }

    pub fn pack(&self) -> Vec<u8> {
        pack(&self.files, Some(&self.btnf))
    }
}

/// Pack members into a NARC. Without a BTNF, an empty one (one root entry).
pub fn pack(files: &[Vec<u8>], btnf: Option<&[u8]>) -> Vec<u8> {
    let default_btnf: Vec<u8> = {
        let mut b = b"BTNF".to_vec();
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&4u32.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b
    };
    let btnf = btnf.unwrap_or(&default_btnf);
    let mut gmif = Vec::new();
    let mut spans = Vec::with_capacity(files.len());
    for f in files {
        let s = gmif.len();
        gmif.extend_from_slice(f);
        spans.push((s as u32, gmif.len() as u32));
        while gmif.len() % 4 != 0 {
            gmif.push(0xFF);
        }
    }
    let mut body = Vec::new();
    body.extend_from_slice(b"BTAF");
    body.extend_from_slice(&((12 + 8 * files.len()) as u32).to_le_bytes());
    body.extend_from_slice(&(files.len() as u16).to_le_bytes());
    body.extend_from_slice(&0u16.to_le_bytes());
    for (s, e) in spans {
        body.extend_from_slice(&s.to_le_bytes());
        body.extend_from_slice(&e.to_le_bytes());
    }
    body.extend_from_slice(btnf);
    body.extend_from_slice(b"GMIF");
    body.extend_from_slice(&((8 + gmif.len()) as u32).to_le_bytes());
    body.extend_from_slice(&gmif);
    let mut out = Vec::with_capacity(16 + body.len());
    out.extend_from_slice(b"NARC");
    out.extend_from_slice(&0xFFFEu16.to_le_bytes());
    out.extend_from_slice(&0x0100u16.to_le_bytes());
    out.extend_from_slice(&((16 + body.len()) as u32).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes());
    out.extend_from_slice(&body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_parse_roundtrip() {
        let files = vec![vec![1, 2, 3], vec![], vec![9; 8]];
        let n = Narc::parse(&pack(&files, None)).unwrap();
        assert_eq!(n.files, files);
        assert_eq!(Narc::parse(&n.pack()).unwrap().pack(), n.pack());
    }
}
