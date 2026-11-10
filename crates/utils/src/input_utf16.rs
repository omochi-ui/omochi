//! Incremental UTF-16 conversion for Windows console reads.
#[derive(Default)]
pub(crate) struct Decoder {
    high_surrogate: Option<u16>,
}

impl Decoder {
    pub(crate) fn write(&mut self, units: &[u16]) -> Vec<u8> {
        let mut units: Vec<_> = self
            .high_surrogate
            .take()
            .into_iter()
            .chain(units.iter().copied())
            .collect();
        if units
            .last()
            .is_some_and(|unit| (0xd800..=0xdbff).contains(unit))
        {
            self.high_surrogate = units.pop();
        }
        String::from_utf16_lossy(&units).into_bytes()
    }

    pub(crate) fn finish(&mut self) -> Vec<u8> {
        if self.high_surrogate.take().is_some() {
            "�".as_bytes().to_vec()
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_unicode_at_every_read_boundary() {
        let text = "éあ🦀終";
        let units: Vec<_> = text.encode_utf16().collect();
        for boundary in 0..=units.len() {
            let mut decoder = Decoder::default();
            let mut bytes = decoder.write(&units[..boundary]);
            bytes.extend(decoder.write(&[]));
            bytes.extend(decoder.write(&units[boundary..]));
            bytes.extend(decoder.finish());
            assert_eq!(bytes, text.as_bytes());
        }
    }

    #[test]
    fn replaces_unpaired_surrogates_and_flushes_only_at_eof() {
        let mut decoder = Decoder::default();
        assert!(decoder.write(&[0xd800]).is_empty());
        assert_eq!(decoder.write(&[b'x' as u16, 0xdc00]), "�x�".as_bytes());
        assert!(decoder.write(&[0xd800]).is_empty());
        assert_eq!(decoder.finish(), "�".as_bytes());
        assert!(decoder.finish().is_empty());
    }
}
