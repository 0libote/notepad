use serde::{Deserialize, Serialize};

pub mod canvas;
mod email;
pub mod markdown;
pub mod shapes;
pub mod sync;
pub use email::{Email, MAX_EMAIL_BYTES, import_email};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AuthState {
    pub configured: bool,
    pub authenticated: bool,
}

pub const SCHEMA: u32 = 3;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Workspace {
    pub schema: u32,
    pub revision: u64,
    pub notes: Vec<Note>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub body: String,
    pub cards: Vec<Card>,
    pub strokes: Vec<Vec<Point>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connections: Vec<canvas::Connection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shapes: Vec<shapes::Shape>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ink: Vec<InkStroke>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct InkStroke {
    pub id: String,
    pub points: Vec<Point>,
    pub style: shapes::Style,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Card {
    pub id: String,
    pub kind: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<Email>,
    pub x: f64,
    pub y: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<canvas::Size>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

pub fn valid_identifier(id: &str) -> bool {
    !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control)
}

pub fn safe_link(s: &str) -> bool {
    (s.starts_with("https://") || s.starts_with("http://"))
        && !s.chars().any(char::is_whitespace)
        && s.split("://").nth(1).is_some_and(|host| !host.is_empty())
}
impl Workspace {
    /// Upgrade stored data and backups. Network writes must already use SCHEMA.
    pub fn upgrade(mut self) -> Result<Self, &'static str> {
        if self.schema == 1 || self.schema == 2 {
            self.schema = SCHEMA;
        }
        self.validate()?;
        Ok(self)
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema != SCHEMA {
            return Err("Unsupported workspace version. Reload or update the app.");
        }
        if self.notes.is_empty() || self.notes.len() > 1000 {
            return Err("Workspace needs 1–1000 notes");
        }
        let mut ids = std::collections::HashSet::new();
        for n in &self.notes {
            if !valid_identifier(&n.id) || !ids.insert(n.id.as_str()) {
                return Err("Duplicate or empty note identifier");
            }
            if n.title.len() > 512
                || n.body.len() > 1_000_000
                || n.cards.len() > 1000
                || n.strokes.len() > 10000
                || n.connections.len() > canvas::MAX_CONNECTIONS
                || n.shapes.len() > 5000
                || n.ink.len() > 10000
            {
                return Err("Note is too large");
            }
            let mut cards = std::collections::HashSet::new();
            for c in &n.cards {
                if !cards.insert(c.id.as_str()) || !valid_identifier(&c.id) {
                    return Err("Duplicate or empty card identifier");
                }
                if !c.x.is_finite()
                    || !c.y.is_finite()
                    || c.x.abs() > 100000.
                    || c.y.abs() > 100000.
                {
                    return Err("Card is outside canvas");
                }
                if c.size.is_some_and(|size| !size.valid_at(c.x, c.y)) {
                    return Err("Invalid card size");
                }
                if c.kind != "email" && c.email.is_some() {
                    return Err("Email metadata on unsupported card");
                }
                match c.kind.as_str() {
                    "email"
                        if c.text.len() <= 1_000_000
                            && c.email.as_ref().is_some_and(Email::validate) => {}
                    "text" | "sticky" if c.text.len() <= 100_000 => {}
                    "note" if valid_identifier(&c.text) => {}
                    "link" if safe_link(&c.text) && c.text.len() < 8192 => {}
                    "image"
                        if [
                            "data:image/png;base64,",
                            "data:image/jpeg;base64,",
                            "data:image/webp;base64,",
                            "data:image/gif;base64,",
                        ]
                        .iter()
                        .any(|p| c.text.starts_with(p))
                            && c.text.len() <= 6_000_000 => {}
                    _ => return Err("Unsupported card content"),
                }
            }
            let mut vector_ids = std::collections::HashSet::new();
            for shape in &n.shapes {
                shape.validate()?;
                if !vector_ids.insert(shape.id.as_str()) {
                    return Err("Duplicate shape identifier");
                }
            }
            for stroke in &n.ink {
                if !valid_identifier(&stroke.id)
                    || !vector_ids.insert(stroke.id.as_str())
                    || stroke.points.is_empty()
                    || stroke.points.len() > 50000
                {
                    return Err("Invalid ink stroke");
                }
                stroke.style.validate()?;
                if stroke.points.iter().any(|p| {
                    !p.x.is_finite()
                        || !p.y.is_finite()
                        || p.x.abs() > 100000.
                        || p.y.abs() > 100000.
                }) {
                    return Err("Invalid ink point");
                }
            }
            let mut connections = std::collections::HashSet::new();
            for c in &n.connections {
                if !valid_identifier(&c.id)
                    || !connections.insert(c.id.as_str())
                    || c.from == c.to
                    || !cards.contains(c.from.as_str())
                    || !cards.contains(c.to.as_str())
                {
                    return Err("Invalid card connection");
                }
            }
            if n.strokes.iter().any(|s| {
                s.len() > 50000
                    || s.iter().any(|p| {
                        !p.x.is_finite()
                            || !p.y.is_finite()
                            || p.x.abs() > 100000.
                            || p.y.abs() > 100000.
                    })
            }) {
                return Err("Invalid drawing");
            }
        }
        Ok(())
    }
}
impl Default for Workspace {
    fn default() -> Self {
        Self { schema: SCHEMA, revision: 0, notes: vec![Note {
            id: "welcome".into(), title: "A little room to think".into(),
            body: "# A little room to think\n\nSome thoughts need a page. Others need a little space.\n\nThis is your first note. Write here, or switch to Canvas to lay things out. Both views belong to the same note.\n\n## Make it yours\n\n- Collect a link or drop in an image\n- Add a thought and move it into place\n- Pick up the pen and draw a connection\n\nYour changes save on this device and sync automatically while signed in. Export a backup whenever you like.".into(),
            cards: vec![
                Card { size: None, email: None, id: "idea".into(), kind: "text".into(), x: 120., y: 100., text: "Room for the unfinished\n\nA place for ideas before they know what they want to be.\n\nTry moving this note by its top edge.".into() },
                Card { size: None, email: None, id: "sticky".into(), kind: "sticky".into(), x: 500., y: 180., text: "What if writing and thinking could happen in the same place?".into() },
                Card { size: None, email: None, id: "reference".into(), kind: "link".into(), x: 160., y: 440., text: "https://excalidraw.com".into() },
            ], shapes:vec![], ink:vec![], connections:vec![], strokes: vec![],
        }] }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip_and_validate() {
        let w = Workspace::default();
        assert!(w.validate().is_ok());
        assert_eq!(
            w,
            serde_json::from_str(&serde_json::to_string(&w).unwrap()).unwrap()
        );
    }
    #[test]
    fn rejects_unsafe_data() {
        let mut w = Workspace::default();
        w.notes[0].cards[0].kind = "link".into();
        w.notes[0].cards[0].text = "javascript:alert(1)".into();
        assert!(w.validate().is_err());
        w.notes[0].cards.clear();
        w.notes.push(w.notes[0].clone());
        assert!(w.validate().is_err());
    }
    #[test]
    fn accepts_legacy_cards_and_validates_email_metadata() {
        let legacy = serde_json::to_value(Workspace::default()).unwrap();
        assert!(legacy["notes"][0]["cards"][0].get("email").is_none());
        let mut w: Workspace = serde_json::from_value(legacy).unwrap();
        let (email, body) =
            import_email(b"Subject: Saved email\nFrom: a@example.com\n\nHello").unwrap();
        let c = &mut w.notes[0].cards[0];
        c.kind = "email".into();
        c.email = Some(email);
        c.text = body;
        assert!(w.validate().is_ok());
        let round_trip: Workspace =
            serde_json::from_str(&serde_json::to_string(&w).unwrap()).unwrap();
        assert_eq!(round_trip, w);
        w.notes[0].cards[0].email.as_mut().unwrap().original = "invalid base64!".into();
        assert!(w.validate().is_err());
        w.notes[0].cards[0].email = None;
        assert!(w.validate().is_err());
    }
    #[test]
    fn rejects_identifiers_that_cannot_round_trip_through_links() {
        let mut w = Workspace::default();
        w.notes[0].id = "bad\0id".into();
        assert!(w.validate().is_err());
        w.notes[0].id = "x".repeat(513);
        assert!(w.validate().is_err());
    }
    #[test]
    fn rejects_unknown_schema_and_bad_coordinates() {
        let mut w = Workspace::default();
        w.schema = 999;
        assert!(w.validate().is_err());
        w.schema = SCHEMA;
        w.notes[0].cards[0].x = f64::NAN;
        assert!(w.validate().is_err());
    }
}

#[cfg(test)]
mod drawing_roundtrip_tests {
    #[test]
    fn geometry_roundtrips_without_sync_drift() {
        let value = 375.20000000000005_f64;
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(
            serde_json::from_str::<f64>(&json).unwrap().to_bits(),
            value.to_bits()
        );
    }
}
