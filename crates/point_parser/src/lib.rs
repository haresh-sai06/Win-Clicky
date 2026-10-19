//! Pure POINT-tag parser (mirrors `src/point_parser.js`) for Rust-side tests.

use regex::Regex;
use once_cell::sync::Lazy;

#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    Say(String),
    Point { x: f64, y: f64, label: String, valid: bool },
}

static RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\[POINT:\s*(-?[0-9]*\.?[0-9]+)\s*,\s*(-?[0-9]*\.?[0-9]+)\s*:\s*([^\]]*)\]").unwrap()
});

pub fn parse_segments(text: &str) -> Vec<Segment> {
    let mut segs = Vec::new();
    let mut last = 0usize;
    for caps in RE.captures_iter(text) {
        let m = caps.get(0).unwrap();
        if m.start() > last {
            let say = text[last..m.start()].trim();
            if !say.is_empty() {
                segs.push(Segment::Say(say.to_string()));
            }
        }
        let x: f64 = caps[1].parse().unwrap_or(f64::NAN);
        let y: f64 = caps[2].parse().unwrap_or(f64::NAN);
        let label = caps[3].trim().to_string();
        let valid = x.is_finite() && y.is_finite() && (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y);
        segs.push(Segment::Point { x, y, label, valid });
        last = m.end();
    }
    if last < text.len() {
        let say = text[last..].trim();
        if !say.is_empty() {
            segs.push(Segment::Say(say.to_string()));
        }
    }
    segs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_points() {
        let segs = parse_segments("A [POINT:0.2,0.8:Submit] B [POINT:0.9,0.1:Close]");
        assert_eq!(segs.len(), 4);
        match &segs[1] {
            Segment::Point { x, y, label, valid } => {
                assert!((*x - 0.2).abs() < 1e-9);
                assert!((*y - 0.8).abs() < 1e-9);
                assert_eq!(label, "Submit");
                assert!(*valid);
            }
            _ => panic!("expected point"),
        }
    }

    #[test]
    fn marks_oob_invalid() {
        let segs = parse_segments("[POINT:1.5,0.5:x]");
        match &segs[0] {
            Segment::Point { valid, .. } => assert!(!*valid),
            _ => panic!("expected point"),
        }
    }
}
