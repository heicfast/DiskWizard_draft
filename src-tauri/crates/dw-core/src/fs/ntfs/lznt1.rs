//! LZNT1 decompression — the NTFS compressed-stream codec.
//!
//! Wire format: a sequence of chunks, each prefixed by a 2-byte
//! header. Header `0` = end of stream. Bit 15 set = the chunk is
//! flag-compressed; bits 0..11 = the chunk's size in the INPUT
//! (including its flag bytes). Decompressed chunks are ≤ 4096 bytes.
//!
//! Inside a compressed chunk: flag bytes gate 8 symbols each (LSB
//! first); symbol `0` = one literal byte; symbol `1` = a 2-byte
//! back-reference whose length/offset bit split depends on the
//! current decompressed position within the chunk:
//!
//! ```text
//! pos < 0x1000 → 4 length bits / 12 offset bits
//! pos < 0x2000 → 5 / 11 … (total always 16; offset shrinks)
//! pos < 0x4000 → 6 / 10
//! pos < 0x8000 → 7 / 9
//! otherwise    → 8 / 8
//! ```
//!
//! `length = (pair & mask) + 3`, `offset = (pair >> bits) + 1`
//! (offsets anchor backwards within the chunk's decompressed window).
//!
//! Salvage semantics: malformed input yields the decompressed prefix,
//! never a panic (partially readable compressed files still recover).

/// Decompress an LZNT1 stream (tolerant prefix semantics).
#[must_use]
pub fn lznt1_decompress(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len().saturating_mul(4));
    let mut pos = 0usize;
    while pos + 2 <= input.len() {
        let header = u16::from_le_bytes([input[pos], input[pos + 1]]);
        if header == 0 {
            break; // end-of-stream terminator
        }
        pos += 2;
        let compressed = header & 0x8000 != 0;
        let chunk_in_len = (usize::from(header & 0x0FFF) + 1).min(input.len() - pos + 1);
        if chunk_in_len == 0 || pos + chunk_in_len > input.len() {
            // Truncated final chunk — salvage what remains.
            let take = input.len() - pos;
            if compressed {
                decompress_chunk(&input[pos..pos + take], &mut out);
            } else {
                out.extend_from_slice(&input[pos..pos + take]);
            }
            break;
        }
        let chunk = &input[pos..pos + chunk_in_len];
        if compressed {
            decompress_chunk(chunk, &mut out);
        } else {
            out.extend_from_slice(chunk);
        }
        pos += chunk_in_len;
    }
    out
}

/// The length-bit split for a decompressed position (see module doc).
#[must_use]
fn split_for(position: usize) -> (u32, u16) {
    if position < 0x1000 {
        (4, 0x000F)
    } else if position < 0x2000 {
        (5, 0x001F)
    } else if position < 0x4000 {
        (6, 0x003F)
    } else if position < 0x8000 {
        (7, 0x007F)
    } else {
        (8, 0x00FF)
    }
}

fn decompress_chunk(chunk: &[u8], out: &mut Vec<u8>) {
    let chunk_start = out.len();
    let mut pos = 0usize;
    while pos < chunk.len() {
        let flags = chunk[pos];
        pos += 1;
        for bit in 0..8 {
            if pos >= chunk.len() {
                return; // chunk exhausted
            }
            if flags & (1 << bit) == 0 {
                out.push(chunk[pos]);
                pos += 1;
            } else {
                if pos + 2 > chunk.len() {
                    return; // malformed tail
                }
                let pair = u16::from_le_bytes([chunk[pos], chunk[pos + 1]]);
                pos += 2;
                let rel = out.len() - chunk_start;
                let (bits, mask) = split_for(rel);
                let length = usize::from(pair & mask) + 3;
                let offset = (usize::from(pair >> bits)).saturating_add(1);
                if offset > rel {
                    return; // reference escapes the chunk window
                }
                for _ in 0..length {
                    let src = out.len() - offset;
                    let b = out[src];
                    out.push(b);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_chunk(input: &mut Vec<u8>, compressed: bool, data: &[u8]) {
        assert!(data.len() <= 4096);
        let mut h = (data.len() - 1) as u16 & 0x0FFF;
        if compressed {
            h |= 0x8000;
        }
        input.extend_from_slice(&h.to_le_bytes());
        input.extend_from_slice(data);
    }

    #[test]
    fn uncompressed_chunk_round_trips() {
        let mut input = Vec::new();
        push_chunk(&mut input, false, b"plain data, no compression");
        let out = lznt1_decompress(&input);
        assert_eq!(out, b"plain data, no compression".to_vec());
    }

    #[test]
    fn multiple_chunks_concatenate() {
        let mut input = Vec::new();
        push_chunk(&mut input, false, b"first");
        push_chunk(&mut input, false, b"second");
        input.extend_from_slice(&0u16.to_le_bytes()); // terminator
        let out = lznt1_decompress(&input);
        assert_eq!(out, b"firstsecond".to_vec());
    }

    #[test]
    fn literal_only_compressed_chunk() {
        // flag byte 0x00 = 8 literals.
        let mut chunk = vec![0x00u8];
        chunk.extend_from_slice(b"ABCDEFGH");
        let mut input = Vec::new();
        push_chunk(&mut input, true, &chunk);
        let out = lznt1_decompress(&input);
        assert_eq!(out, b"ABCDEFGH".to_vec());
    }

    #[test]
    fn run_length_back_reference() {
        // Literals 'A','B' then a back-ref pair: flag bits = 0b100
        // (bit2 set → third symbol is a pair).
        let mut chunk = vec![0x04u8];
        chunk.push(b'A');
        chunk.push(b'B');
        // rel position when the pair resolves = 2 → split (4, 0x0F):
        // length = (pair & 0xF) + 3; offset = (pair >> 4) + 1.
        // Want offset 2, length 3+6=9: pair = (2-1)<<4 | 6 = 0x16.
        chunk.extend_from_slice(&[0x16, 0x00]);
        let mut input = Vec::new();
        push_chunk(&mut input, true, &chunk);
        let out = lznt1_decompress(&input);
        assert_eq!(out, b"ABABABABABA".to_vec());
    }

    #[test]
    fn malformed_inputs_never_panic() {
        let cases: Vec<Vec<u8>> = vec![
            Vec::new(),
            vec![0x00],
            vec![0x00, 0x10],
            vec![0xFF; 64],
            vec![0x01, 0x80, 0xFF, 0xFF, 0xFF],
        ];
        for c in cases {
            let _ = lznt1_decompress(&c);
        }
    }

    #[test]
    fn split_table_direction() {
        assert_eq!(split_for(0x0FFF), (4, 0x000F));
        assert_eq!(split_for(0x1000), (5, 0x001F));
        assert_eq!(split_for(0x2000), (6, 0x003F));
        assert_eq!(split_for(0x4000), (7, 0x007F));
        assert_eq!(split_for(0x8000), (8, 0x00FF));
    }
}
