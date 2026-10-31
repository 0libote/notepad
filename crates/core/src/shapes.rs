use crate::{
    Point,
    canvas::{Rect, Size},
};
use serde::{Deserialize, Serialize};

const COORD_LIMIT: f64 = 100_000.0;
const DIM_LIMIT: f64 = 200_000.0;
const MIN_SIDE: f64 = 1.0;

/// Excalidraw-like vector shape kinds.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    Rectangle,
    Diamond,
    Ellipse,
    Line,
    Arrow,
    Text,
}

impl ShapeKind {
    fn is_line_like(self) -> bool {
        matches!(self, ShapeKind::Line | ShapeKind::Arrow)
    }
}

/// Stroke/fill styling for a shape.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Style {
    pub stroke: String,
    pub fill: String,
    pub stroke_width: f64,
    pub opacity: u8,
    pub dashed: bool,
    pub rough: bool,
    pub font_size: f64,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            stroke: "#1e1e1e".to_string(),
            fill: "transparent".to_string(),
            stroke_width: 2.0,
            opacity: 100,
            dashed: false,
            rough: true,
            font_size: 24.0,
        }
    }
}

fn is_valid_color(s: &str) -> bool {
    if s == "transparent" {
        return true;
    }
    let bytes = s.as_bytes();
    if bytes.len() != 7 || bytes[0] != b'#' {
        return false;
    }
    bytes[1..].iter().all(|b| b.is_ascii_hexdigit())
}

impl Style {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !is_valid_color(&self.stroke) {
            return Err("Invalid stroke color");
        }
        if !is_valid_color(&self.fill) {
            return Err("Invalid fill color");
        }
        if !self.stroke_width.is_finite() || !(1.0..=8.0).contains(&self.stroke_width) {
            return Err("Invalid stroke width");
        }
        if !self.font_size.is_finite() || !(12.0..=72.0).contains(&self.font_size) {
            return Err("Invalid font size");
        }
        if self.opacity > 100 {
            return Err("Invalid opacity");
        }
        Ok(())
    }
}

/// A vector shape on the canvas.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub id: String,
    pub kind: ShapeKind,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub style: Style,
    pub text: String,
}

impl Shape {
    /// Build a shape from a drag gesture.
    ///
    /// Closed shapes (rectangle/diamond/ellipse/text) normalize reverse drags
    /// so `width`/`height` are non-negative. Lines/arrows keep signed
    /// `width`/`height` so the drag direction survives.
    pub fn from_drag(id: String, kind: ShapeKind, start: Point, end: Point, style: Style) -> Self {
        let (x, y, width, height) = if kind.is_line_like() {
            (start.x, start.y, end.x - start.x, end.y - start.y)
        } else {
            (
                start.x.min(end.x),
                start.y.min(end.y),
                (end.x - start.x).abs(),
                (end.y - start.y).abs(),
            )
        };
        Self {
            id,
            kind,
            x,
            y,
            width,
            height,
            style,
            text: String::new(),
        }
    }

    /// Normalized bounding rectangle with `width`/`height >= 1`.
    pub fn bounds(&self) -> Rect {
        let x = self.x.min(self.x + self.width);
        let y = self.y.min(self.y + self.height);
        let width = self.width.abs().max(MIN_SIDE);
        let height = self.height.abs().max(MIN_SIDE);
        // `f64::max` returns the non-NaN side when one side is NaN, so an
        // invalid shape still yields a finite 1x1 rect here; `validate`
        // rejects it separately.
        Rect {
            x,
            y,
            size: Size { width, height },
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if !crate::valid_identifier(&self.id) {
            return Err("Invalid shape identifier");
        }
        self.style.validate()?;
        if self.text.len() > 100_000 {
            return Err("Shape text is too large");
        }
        if !self.x.is_finite()
            || !self.y.is_finite()
            || !self.width.is_finite()
            || !self.height.is_finite()
        {
            return Err("Invalid shape coordinates");
        }
        if self.width.abs() > DIM_LIMIT || self.height.abs() > DIM_LIMIT {
            return Err("Invalid shape dimensions");
        }
        let x1 = self.x + self.width;
        let y1 = self.y + self.height;
        if !x1.is_finite() || !y1.is_finite() {
            return Err("Invalid shape coordinates");
        }
        for v in [self.x, self.y, x1, y1] {
            if !(-COORD_LIMIT..=COORD_LIMIT).contains(&v) {
                return Err("Shape is outside canvas");
            }
        }
        if !self.kind.is_line_like() && (self.width < MIN_SIDE || self.height < MIN_SIDE) {
            return Err("Invalid shape dimensions");
        }
        Ok(())
    }

    /// SVG path for the shape body. Arrowheads are rendered by the parent,
    /// so arrows return just the shaft. Text returns an empty path.
    pub fn path(&self) -> String {
        if !self.x.is_finite()
            || !self.y.is_finite()
            || !self.width.is_finite()
            || !self.height.is_finite()
        {
            return String::new();
        }
        match self.kind {
            ShapeKind::Text => String::new(),
            ShapeKind::Line | ShapeKind::Arrow => {
                let x2 = self.x + self.width;
                let y2 = self.y + self.height;
                if !self.style.rough {
                    format!("M {} {} L {} {}", n(self.x), n(self.y), n(x2), n(y2))
                } else {
                    let h = hash_str(&self.id);
                    let mx = (self.x + x2) / 2.0 + jitter(h, 1, 1.5);
                    let my = (self.y + y2) / 2.0 + jitter(h, 2, 1.5);
                    format!(
                        "M {} {} L {} {} L {} {}",
                        n(self.x),
                        n(self.y),
                        n(mx),
                        n(my),
                        n(x2),
                        n(y2)
                    )
                }
            }
            ShapeKind::Rectangle => {
                let b = self.bounds();
                let x2 = b.x + b.size.width;
                let y2 = b.y + b.size.height;
                if !self.style.rough {
                    format!(
                        "M {} {} H {} V {} H {} Z",
                        n(b.x),
                        n(b.y),
                        n(x2),
                        n(y2),
                        n(b.x)
                    )
                } else {
                    let h = hash_str(&self.id);
                    let p = [
                        (b.x + jitter(h, 1, 1.5), b.y + jitter(h, 2, 1.5)),
                        (x2 + jitter(h, 3, 1.5), b.y + jitter(h, 4, 1.5)),
                        (x2 + jitter(h, 5, 1.5), y2 + jitter(h, 6, 1.5)),
                        (b.x + jitter(h, 7, 1.5), y2 + jitter(h, 8, 1.5)),
                    ];
                    format!(
                        "M {} {} L {} {} L {} {} L {} {} Z",
                        n(p[0].0),
                        n(p[0].1),
                        n(p[1].0),
                        n(p[1].1),
                        n(p[2].0),
                        n(p[2].1),
                        n(p[3].0),
                        n(p[3].1)
                    )
                }
            }
            ShapeKind::Diamond => {
                let b = self.bounds();
                let cx = b.x + b.size.width / 2.0;
                let cy = b.y + b.size.height / 2.0;
                let x2 = b.x + b.size.width;
                let y2 = b.y + b.size.height;
                if !self.style.rough {
                    format!(
                        "M {} {} L {} {} L {} {} L {} {} Z",
                        n(cx),
                        n(b.y),
                        n(x2),
                        n(cy),
                        n(cx),
                        n(y2),
                        n(b.x),
                        n(cy)
                    )
                } else {
                    let h = hash_str(&self.id);
                    let p = [
                        (cx + jitter(h, 1, 1.5), b.y + jitter(h, 2, 1.5)),
                        (x2 + jitter(h, 3, 1.5), cy + jitter(h, 4, 1.5)),
                        (cx + jitter(h, 5, 1.5), y2 + jitter(h, 6, 1.5)),
                        (b.x + jitter(h, 7, 1.5), cy + jitter(h, 8, 1.5)),
                    ];
                    format!(
                        "M {} {} L {} {} L {} {} L {} {} Z",
                        n(p[0].0),
                        n(p[0].1),
                        n(p[1].0),
                        n(p[1].1),
                        n(p[2].0),
                        n(p[2].1),
                        n(p[3].0),
                        n(p[3].1)
                    )
                }
            }
            ShapeKind::Ellipse => {
                let b = self.bounds();
                let cy = b.y + b.size.height / 2.0;
                let (rx, ry) = if !self.style.rough {
                    (b.size.width / 2.0, b.size.height / 2.0)
                } else {
                    let h = hash_str(&self.id);
                    (
                        (b.size.width / 2.0 + jitter(h, 1, 1.5)).max(0.5),
                        (b.size.height / 2.0 + jitter(h, 2, 1.5)).max(0.5),
                    )
                };
                format!(
                    "M {} {} A {} {} 0 1 0 {} {} A {} {} 0 1 0 {} {}",
                    n(b.x),
                    n(cy),
                    n(rx),
                    n(ry),
                    n(b.x + b.size.width),
                    n(cy),
                    n(rx),
                    n(ry),
                    n(b.x),
                    n(cy)
                )
            }
        }
    }
}

fn n(v: f64) -> String {
    // Normalize -0.0 to 0.0 so output is stable.
    format!("{}", v + 0.0)
}

fn hash_str(s: &str) -> u64 {
    let mut h: u64 = 14_695_981_039_346_656_037;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(1_099_511_628_211);
    }
    h
}

/// Deterministic pseudo-random offset in `[-scale, scale]`.
fn jitter(hash: u64, salt: u64, scale: f64) -> f64 {
    let h = hash.wrapping_add(salt.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let v = ((h >> 17) % 1000) as f64 / 1000.0;
    (v - 0.5) * 2.0 * scale
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    #[test]
    fn reverse_drag_normalizes_closed_shapes() {
        let s = Shape::from_drag(
            "a".into(),
            ShapeKind::Rectangle,
            pt(100.0, 100.0),
            pt(10.0, 20.0),
            Style::default(),
        );
        assert_eq!((s.x, s.y), (10.0, 20.0));
        assert_eq!((s.width, s.height), (90.0, 80.0));
        let b = s.bounds();
        assert_eq!((b.x, b.y), (10.0, 20.0));
        assert_eq!((b.size.width, b.size.height), (90.0, 80.0));

        for kind in [ShapeKind::Diamond, ShapeKind::Ellipse] {
            let s = Shape::from_drag(
                "a".into(),
                kind,
                pt(5.0, 5.0),
                pt(-5.0, -5.0),
                Style::default(),
            );
            assert_eq!((s.x, s.y), (-5.0, -5.0));
            assert_eq!((s.width, s.height), (10.0, 10.0));
        }
    }

    #[test]
    fn line_and_arrow_keep_drag_direction() {
        let line = Shape::from_drag(
            "l".into(),
            ShapeKind::Line,
            pt(100.0, 100.0),
            pt(10.0, 20.0),
            Style::default(),
        );
        assert_eq!((line.x, line.y), (100.0, 100.0));
        assert_eq!((line.width, line.height), (-90.0, -80.0));
        // Bounds still normalize for hit-testing.
        let b = line.bounds();
        assert_eq!((b.x, b.y), (10.0, 20.0));
        assert_eq!((b.size.width, b.size.height), (90.0, 80.0));

        let arrow = Shape::from_drag(
            "a".into(),
            ShapeKind::Arrow,
            pt(0.0, 0.0),
            pt(-30.0, 5.0),
            Style::default(),
        );
        assert_eq!((arrow.width, arrow.height), (-30.0, 5.0));
    }

    #[test]
    fn bounds_clamp_degenerate_sides_to_one() {
        let mut s = Shape::from_drag(
            "z".into(),
            ShapeKind::Line,
            pt(3.0, 4.0),
            pt(3.0, 4.0),
            Style::default(),
        );
        // Zero-length line keeps 0 size but bounds report >= 1 for hit-testing.
        assert_eq!((s.width, s.height), (0.0, 0.0));
        let b = s.bounds();
        assert_eq!((b.size.width, b.size.height), (1.0, 1.0));
        s.kind = ShapeKind::Rectangle;
        assert!(s.validate().is_err());
    }

    #[test]
    fn rejects_invalid_shape_data() {
        let good = Shape::from_drag(
            "ok".into(),
            ShapeKind::Rectangle,
            pt(0.0, 0.0),
            pt(10.0, 10.0),
            Style::default(),
        );
        assert!(good.validate().is_ok());

        let mut bad = good.clone();
        bad.id = "bad\0id".into();
        assert!(bad.validate().is_err());
        let mut bad = good.clone();
        bad.id = String::new();
        assert!(bad.validate().is_err());

        let mut bad = good.clone();
        bad.x = f64::NAN;
        assert!(bad.validate().is_err());
        let mut bad = good.clone();
        bad.x = 200_000.0;
        assert!(bad.validate().is_err());
        let mut bad = good.clone();
        bad.width = 300_000.0;
        assert!(bad.validate().is_err());
        let mut bad = good.clone();
        bad.width = 0.5;
        assert!(bad.validate().is_err());
        let mut bad = good.clone();
        bad.text = "x".repeat(100_001);
        assert!(bad.validate().is_err());

        // Endpoints must stay within +/-100000.
        let mut bad = good.clone();
        bad.x = 99_999.0;
        bad.width = 2.0;
        assert!(bad.validate().is_err());

        // Serde round-trip.
        let json = serde_json::to_string(&good).unwrap();
        assert_eq!(good, serde_json::from_str::<Shape>(&json).unwrap());
        // snake_case kind.
        assert!(json.contains("\"rectangle\""));
    }

    #[test]
    fn style_validation_rejects_bad_values() {
        let st = Style::default();
        assert!(st.validate().is_ok());
        assert_eq!(st.stroke_width, 2.0);
        assert_eq!(st.opacity, 100);
        assert_eq!(st.font_size, 24.0);

        for (st, ok) in [
            (
                Style {
                    stroke: "red".into(),
                    ..Style::default()
                },
                false,
            ),
            (
                Style {
                    stroke: "#gggggg".into(),
                    ..Style::default()
                },
                false,
            ),
            (
                Style {
                    stroke: "#FFF".into(),
                    ..Style::default()
                },
                false,
            ),
            (
                Style {
                    stroke: "#ffffff".into(),
                    ..Style::default()
                },
                true,
            ),
            (
                Style {
                    stroke: "#FFFFFF".into(),
                    ..Style::default()
                },
                true,
            ),
            (
                Style {
                    fill: "nope".into(),
                    ..Style::default()
                },
                false,
            ),
            (
                Style {
                    fill: "transparent".into(),
                    ..Style::default()
                },
                true,
            ),
            (
                Style {
                    stroke_width: 0.5,
                    ..Style::default()
                },
                false,
            ),
            (
                Style {
                    stroke_width: 9.0,
                    ..Style::default()
                },
                false,
            ),
            (
                Style {
                    stroke_width: f64::NAN,
                    ..Style::default()
                },
                false,
            ),
            (
                Style {
                    font_size: 11.0,
                    ..Style::default()
                },
                false,
            ),
            (
                Style {
                    font_size: 73.0,
                    ..Style::default()
                },
                false,
            ),
            (
                Style {
                    opacity: 101,
                    ..Style::default()
                },
                false,
            ),
        ] {
            // opacity u8 boundary: 100 ok, 101 err (constructed above).
            assert_eq!(st.validate().is_ok(), ok, "style={st:?}");
            // Shape validation surfaces style errors too.
            let s = Shape {
                id: "s".into(),
                kind: ShapeKind::Rectangle,
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
                style: st.clone(),
                text: String::new(),
            };
            assert_eq!(s.validate().is_ok(), ok);
        }
    }

    #[test]
    fn path_output_is_deterministic_and_covers_kinds() {
        let mk = |kind, rough| Shape {
            id: "det".into(),
            kind,
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 60.0,
            style: Style {
                rough,
                ..Style::default()
            },
            text: String::new(),
        };
        for kind in [
            ShapeKind::Rectangle,
            ShapeKind::Diamond,
            ShapeKind::Ellipse,
            ShapeKind::Line,
            ShapeKind::Arrow,
        ] {
            let clean = mk(kind, false);
            let rough = mk(kind, true);
            assert_eq!(clean.path(), clean.path());
            assert_eq!(rough.path(), rough.path());
            assert_ne!(clean.path(), rough.path(), "kind={kind:?}");
            assert!(!clean.path().is_empty());
        }
        // Text has no path.
        assert_eq!(mk(ShapeKind::Text, false).path(), "");
        // Clean rectangle exact output stays stable.
        assert_eq!(
            mk(ShapeKind::Rectangle, false).path(),
            "M 10 20 H 110 V 80 H 10 Z"
        );
        // Clean line shaft.
        assert_eq!(mk(ShapeKind::Line, false).path(), "M 10 20 L 110 80");
        // Rough line adds a midpoint segment but keeps endpoints.
        let rough_line = mk(ShapeKind::Line, true).path();
        assert!(rough_line.starts_with("M 10 20 L "));
        assert!(rough_line.ends_with(" L 110 80"));
        // Ellipse uses arcs.
        assert!(mk(ShapeKind::Ellipse, false).path().contains('A'));
    }
}
