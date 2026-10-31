use crate::{Card, Note, Point};
use serde::{Deserialize, Serialize};

pub const WIDTH: f64 = 2600.;
pub const HEIGHT: f64 = 1800.;
pub const MIN_WIDTH: f64 = 180.;
pub const MIN_HEIGHT: f64 = 120.;
pub const MAX_CONNECTIONS: usize = 2000;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}
impl Size {
    pub fn valid_at(self, x: f64, y: f64) -> bool {
        self.width.is_finite()
            && self.height.is_finite()
            && (MIN_WIDTH..=1000.).contains(&self.width)
            && (MIN_HEIGHT..=1200.).contains(&self.height)
            && x.is_finite()
            && y.is_finite()
            && x.abs() <= 100000.
            && y.abs() <= 100000.
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Connection {
    pub id: String,
    pub from: String,
    pub to: String,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub size: Size,
}
impl Card {
    pub fn resize(&mut self, width: f64, height: f64) {
        if !width.is_finite() || !height.is_finite() {
            return;
        }
        self.size = Some(Size {
            width: width.clamp(MIN_WIDTH, 1000.),
            height: height.clamp(MIN_HEIGHT, 1200.),
        });
    }
    pub fn move_to(&mut self, x: f64, y: f64, measured: Size) {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        let _ = measured;
        self.x = x.clamp(-100000., 100000.);
        self.y = y.clamp(-100000., 100000.);
    }
}
impl Note {
    pub fn remove_card(&mut self, id: &str) {
        self.cards.retain(|c| c.id != id);
        self.connections.retain(|c| c.from != id && c.to != id);
    }
}

/// Intersect each card's border with the ray to the other card's centre.
pub fn endpoints(a: Rect, b: Rect) -> (Point, Point) {
    fn edge(a: Rect, dx: f64, dy: f64) -> Point {
        let scale = ((dx.abs() / (a.size.width / 2.)).max(dy.abs() / (a.size.height / 2.))).recip();
        Point {
            x: a.x + a.size.width / 2. + dx * scale,
            y: a.y + a.size.height / 2. + dy * scale,
        }
    }
    let dx = b.x + b.size.width / 2. - a.x - a.size.width / 2.;
    let dy = b.y + b.size.height / 2. - a.y - a.size.height / 2.;
    if dx.abs() + dy.abs() < f64::EPSILON {
        return (
            Point {
                x: a.x + a.size.width,
                y: a.y + a.size.height / 2.,
            },
            Point {
                x: b.x,
                y: b.y + b.size.height / 2.,
            },
        );
    }
    (edge(a, dx, dy), edge(b, -dx, -dy))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Workspace;
    #[test]
    fn stored_legacy_data_upgrades_but_legacy_writes_are_rejected() {
        let mut old = crate::Workspace::default();
        old.schema = 1;
        old.revision = 8;
        assert!(old.validate().is_err());
        let upgraded = old.clone().upgrade().unwrap();
        assert_eq!(upgraded.schema, crate::SCHEMA);
        assert_eq!(upgraded.revision, 8);
        assert_eq!(upgraded.notes, old.notes);
        let mut future = old.clone();
        future.schema = 999;
        assert!(future.upgrade().is_err());
        old.notes[0].cards[0].x = f64::NAN;
        assert!(old.upgrade().is_err());
    }
    #[test]
    fn legacy_round_trip_and_geometry_validation() {
        let old = serde_json::to_string(&Workspace::default()).unwrap();
        assert!(!old.contains("\"size\""));
        assert!(!old.contains("\"connections\""));
        let mut w: Workspace = serde_json::from_str(&old).unwrap();
        w.notes[0].cards[0].resize(400., 300.);
        w.notes[0].connections.push(Connection {
            id: "edge".into(),
            from: "idea".into(),
            to: "sticky".into(),
        });
        w.validate().unwrap();
        assert_eq!(
            w,
            serde_json::from_str::<Workspace>(&serde_json::to_string(&w).unwrap()).unwrap()
        );
        for size in [
            Size {
                width: f64::NAN,
                height: 120.,
            },
            Size {
                width: 179.,
                height: 200.,
            },
            Size {
                width: 200.,
                height: f64::INFINITY,
            },
            Size {
                width: 1001.,
                height: 200.,
            },
        ] {
            w.notes[0].cards[0].size = Some(size);
            assert!(w.validate().is_err());
        }
        w.notes[0].cards[0].size = Some(Size {
            width: 1000.,
            height: 300.,
        });
        w.notes[0].cards[0].x = 100001.;
        assert!(w.validate().is_err());
    }
    #[test]
    fn connections_are_local_valid_and_removed_with_the_card() {
        let mut w = Workspace::default();
        let edge = Connection {
            id: "edge".into(),
            from: "idea".into(),
            to: "sticky".into(),
        };
        w.notes[0].connections.push(edge.clone());
        w.validate().unwrap();
        w.notes[0].connections[0].to = "absent".into();
        assert!(w.validate().is_err());
        w.notes[0].connections[0].to = "idea".into();
        assert!(w.validate().is_err());
        w.notes[0].connections = vec![edge.clone(), edge];
        assert!(w.validate().is_err());
        w.notes[0].connections.pop();
        w.notes[0].remove_card("idea");
        assert!(w.notes[0].connections.is_empty());
        w.validate().unwrap();
    }
    #[test]
    fn resizing_and_movement_support_an_open_board() {
        let mut c = Workspace::default().notes.remove(0).cards.remove(0);
        c.x = 2200.;
        c.y = 1400.;
        c.resize(5000., 5000.);
        assert_eq!(
            c.size,
            Some(Size {
                width: 1000.,
                height: 1200.
            })
        );
        c.move_to(-10., -10., c.size.unwrap());
        assert_eq!((c.x, c.y), (-10., -10.));
        c.resize(900., 1000.);
        c.move_to(2500., 1700., c.size.unwrap());
        assert_eq!((c.x, c.y), (2500., 1700.));
        assert!(c.size.unwrap().valid_at(c.x, c.y));
        c.resize(-1., -1.);
        assert_eq!(
            c.size,
            Some(Size {
                width: MIN_WIDTH,
                height: MIN_HEIGHT
            })
        );
        let before = c.clone();
        c.resize(f64::NAN, 200.);
        c.move_to(0., f64::INFINITY, c.size.unwrap());
        assert_eq!(c, before);
    }
    #[test]
    fn anchors_follow_movement_resize_and_diagonal_layouts() {
        let a = Rect {
            x: 0.,
            y: 0.,
            size: Size {
                width: 300.,
                height: 200.,
            },
        };
        let mut b = Rect {
            x: 500.,
            y: 0.,
            size: a.size,
        };
        assert_eq!(
            endpoints(a, b),
            (Point { x: 300., y: 100. }, Point { x: 500., y: 100. })
        );
        b.x = 50.;
        b.y = 600.;
        b.size = Size {
            width: 500.,
            height: 400.,
        };
        let (start, end) = endpoints(a, b);
        assert_eq!(start.y, 200.);
        assert_eq!(end.y, 600.);
        assert!(start.x >= a.x && start.x <= a.x + a.size.width);
        assert!(end.x >= b.x && end.x <= b.x + b.size.width);
        let (start, end) = endpoints(a, a);
        assert!(start.x.is_finite() && end.y.is_finite());
    }
}
