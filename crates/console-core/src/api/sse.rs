//! Server-Sent-Events frame handling, kept pure for unit testing.

/// Drain complete SSE frames from `buf` (frames are separated by a blank
/// line), returning the `data: ` payload of each. Any partial trailing frame
/// stays in `buf` for the next network chunk.
///
/// Multi-line `data:` fields within one frame are joined with `\n` per the
/// SSE spec.
pub fn drain_data_payloads(buf: &mut String) -> Vec<String> {
    let mut payloads = Vec::new();
    while let Some(boundary) = buf.find("\n\n") {
        let frame: String = buf.drain(..boundary + 2).collect();
        let mut data_lines: Vec<&str> = Vec::new();
        for line in frame.lines() {
            if let Some(payload) = line.strip_prefix("data: ") {
                data_lines.push(payload);
            } else if let Some(payload) = line.strip_prefix("data:") {
                data_lines.push(payload);
            }
        }
        if !data_lines.is_empty() {
            payloads.push(data_lines.join("\n"));
        }
    }
    payloads
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drains_complete_frames_keeps_partial() {
        let mut buf = String::from("data: {\"a\":1}\n\ndata: {\"b\":");
        let got = drain_data_payloads(&mut buf);
        assert_eq!(got, vec!["{\"a\":1}".to_string()]);
        assert_eq!(buf, "data: {\"b\":");

        buf.push_str("2}\n\n");
        let got = drain_data_payloads(&mut buf);
        assert_eq!(got, vec!["{\"b\":2}".to_string()]);
        assert!(buf.is_empty());
    }

    #[test]
    fn ignores_non_data_lines_and_comments() {
        let mut buf = String::from(": keepalive\nevent: status\ndata: {\"x\":1}\n\n");
        let got = drain_data_payloads(&mut buf);
        assert_eq!(got, vec!["{\"x\":1}".to_string()]);
    }

    #[test]
    fn joins_multiline_data_fields() {
        let mut buf = String::from("data: line1\ndata: line2\n\n");
        let got = drain_data_payloads(&mut buf);
        assert_eq!(got, vec!["line1\nline2".to_string()]);
    }

    #[test]
    fn frame_split_across_many_chunks() {
        let full = "data: {\"n\":1}\n\ndata: {\"n\":2}\n\n";
        let mut buf = String::new();
        let mut collected = Vec::new();
        for chunk in full.as_bytes().chunks(3) {
            buf.push_str(std::str::from_utf8(chunk).unwrap());
            collected.extend(drain_data_payloads(&mut buf));
        }
        assert_eq!(collected, vec!["{\"n\":1}", "{\"n\":2}"]);
    }
}
