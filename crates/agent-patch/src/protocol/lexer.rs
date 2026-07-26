//! Lightweight line lexer for the patch protocol.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawLine<'a> {
    pub line_no: usize,
    pub text: &'a str,
}

pub fn split_lines(input: &str) -> Vec<RawLine<'_>> {
    let mut out = Vec::new();
    let mut line_no = 1usize;
    let mut start = 0usize;
    let bytes = input.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            let end = if i > start && bytes[i - 1] == b'\r' {
                i - 1
            } else {
                i
            };
            out.push(RawLine {
                line_no,
                text: &input[start..end],
            });
            line_no += 1;
            i += 1;
            start = i;
        } else {
            i += 1;
        }
    }
    if start < input.len() || input.is_empty() {
        // Trailing content without newline, or empty input yields one empty line for empty.
        if start < input.len() {
            let end = if input[start..].ends_with('\r') {
                input.len() - 1
            } else {
                input.len()
            };
            out.push(RawLine {
                line_no,
                text: &input[start..end],
            });
        } else if input.is_empty() {
            out.push(RawLine {
                line_no: 1,
                text: "",
            });
        }
        // If input ends with newline, we already emitted the last line; no extra.
    }
    // File ending with newline: last empty segment after final \n should not add empty line
    // unless the content had trailing blank — standard: split_lines keeps lines without
    // synthesizing an empty trailing line after a terminating newline.
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_lf() {
        let lines = split_lines("a\nb\n");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "a");
        assert_eq!(lines[1].text, "b");
    }

    #[test]
    fn splits_crlf() {
        let lines = split_lines("a\r\nb\r\n");
        assert_eq!(lines[0].text, "a");
        assert_eq!(lines[1].text, "b");
    }
}
