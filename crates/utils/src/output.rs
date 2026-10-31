//! UTF-8-safe bounded output shared by native utility surfaces.
pub const OUTPUT_LIMIT: usize = 64 * 1024;
#[derive(Default, Clone)]
pub struct OutputBuffer {
    pub text: String,
    pub truncated: bool,
}
impl OutputBuffer {
    pub fn append(&mut self, text: &str) {
        self.text.push_str(text);
        if self.text.len() > OUTPUT_LIMIT {
            let mut start = self.text.len() - OUTPUT_LIMIT;
            while !self.text.is_char_boundary(start) {
                start += 1;
            }
            self.text.drain(..start);
            self.truncated = true;
        }
    }
    pub fn reset(&mut self, text: &str) {
        self.text.clear();
        self.truncated = false;
        self.append(text);
    }
    pub fn fenced(&self) -> String {
        let mut run = 0;
        let mut longest = 0;
        for c in self.text.chars() {
            if c == '`' {
                run += 1;
                longest = longest.max(run);
            } else {
                run = 0;
            }
        }
        let fence = "`".repeat(3.max(longest + 1));
        format!(
            "{}{fence}\n{}\n{fence}",
            if self.truncated {
                "Earlier output truncated.\n\n"
            } else {
                ""
            },
            self.text
        )
    }
}
#[derive(Default)]
pub struct Decoder {
    pending: Vec<u8>,
}
impl Decoder {
    pub fn write(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut output = String::new();
        let mut offset = 0;
        while offset < self.pending.len() {
            match std::str::from_utf8(&self.pending[offset..]) {
                Ok(text) => {
                    output.push_str(text);
                    offset = self.pending.len();
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    output.push_str(
                        std::str::from_utf8(&self.pending[offset..offset + valid]).unwrap(),
                    );
                    offset += valid;
                    match error.error_len() {
                        Some(length) => {
                            output.push('�');
                            offset += length;
                        }
                        None => break,
                    }
                }
            }
        }
        self.pending.drain(..offset);
        output
    }
    pub fn finish(&mut self) -> String {
        let result = String::from_utf8_lossy(&self.pending).into_owned();
        self.pending.clear();
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_unicode_bounded_output_and_embedded_fences() {
        let mut decoder = Decoder::default();
        assert_eq!(decoder.write(&[0xe3]), "");
        assert_eq!(decoder.write(&[0x81, 0x82]), "あ");
        assert_eq!(decoder.write(&[0xff]), "�");
        let mut output = OutputBuffer::default();
        output.append(&"あ".repeat(30000));
        assert!(output.truncated);
        assert!(output.text.len() <= OUTPUT_LIMIT);
        output.reset("hello ```` world");
        assert!(output.fenced().starts_with("`````\n"));
    }
}
