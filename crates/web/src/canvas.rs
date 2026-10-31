use crate::{Editor, Icon, point, read_capture, uid};
use leptos::prelude::*;
use notepad_core::{
    Card, InkStroke, Point, Workspace,
    canvas::{self, Connection, Rect, Size},
    markdown::{self, NoteLink},
    shapes::{Shape, ShapeKind, Style},
};
use std::collections::HashMap;
use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{KeyboardEvent, PointerEvent};

#[derive(Clone, PartialEq)]
enum Selection {
    Card(String),
    Connection(String),
    Stroke(usize),
    Ink(String),
    Shapes(Vec<String>),
}
#[derive(Clone)]
enum Gesture {
    InkMove {
        pointer: i32,
        id: String,
        start: Point,
        points: Vec<Point>,
    },
    Erase {
        pointer: i32,
    },
    Pan {
        pointer: i32,
        x: f64,
        y: f64,
        origin: Point,
    },
    Shape {
        pointer: i32,
        id: String,
        start: Point,
        kind: ShapeKind,
    },
    VectorMove {
        pointer: i32,
        start: Point,
        originals: Vec<(String, f64, f64)>,
    },
    VectorResize {
        pointer: i32,
        id: String,
        start: Point,
        shape: Shape,
    },
    Marquee {
        pointer: i32,
        start: Point,
        end: Point,
    },
    Move {
        pointer: i32,
        id: String,
        dx: f64,
        dy: f64,
    },
    Resize {
        pointer: i32,
        id: String,
        x: f64,
        y: f64,
        size: Size,
    },
    Draw {
        pointer: i32,
    },
}
impl Gesture {
    fn pointer(&self) -> i32 {
        match self {
            Self::Move { pointer, .. }
            | Self::Resize { pointer, .. }
            | Self::Draw { pointer }
            | Self::Pan { pointer, .. }
            | Self::Shape { pointer, .. }
            | Self::VectorMove { pointer, .. }
            | Self::VectorResize { pointer, .. }
            | Self::Marquee { pointer, .. }
            | Self::Erase { pointer }
            | Self::InkMove { pointer, .. } => *pointer,
        }
    }
}
#[derive(Clone, Copy)]
struct Board {
    e: Editor,
    tool: RwSignal<&'static str>,
    zoom: RwSignal<f64>,
    selected: RwSignal<Option<Selection>>,
    gesture: RwSignal<Option<Gesture>>,
    from: RwSignal<Option<String>>,
    sizes: RwSignal<HashMap<String, Size>>,
    pan: RwSignal<Point>,
    space: RwSignal<bool>,
    style: RwSignal<Style>,
    text_edit: RwSignal<Option<String>>,
    locked: RwSignal<bool>,
    gesture_saved: RwSignal<bool>,
}
impl Board {
    fn size(self, card: &Card) -> Size {
        card.size
            .or_else(|| self.sizes.with(|sizes| sizes.get(&card.id).copied()))
            .unwrap_or(Size {
                width: if card.kind == "sticky" {
                    260.
                } else if ["email", "note"].contains(&card.kind.as_str()) {
                    360.
                } else {
                    300.
                },
                height: 260.,
            })
    }
    fn rect(self, card: &Card) -> Rect {
        Rect {
            x: card.x,
            y: card.y,
            size: self.size(card),
        }
    }
    fn select(self, selection: Selection) {
        self.selected.set(Some(selection));
    }
    fn focus(self) {
        if let Some(world) = document()
            .get_element_by_id("world")
            .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let options = web_sys::FocusOptions::new();
            options.set_prevent_scroll(true);
            world.focus_with_options(&options).ok();
        }
    }
    fn remove(self) {
        if !self.e.ready.get_untracked() {
            return;
        }
        let Some(selection) = self.selected.get_untracked() else {
            return;
        };
        self.e.snapshot();
        self.e.change(|n| match selection {
            Selection::Card(id) => n.remove_card(&id),
            Selection::Connection(id) => n.connections.retain(|c| c.id != id),
            Selection::Ink(id) => n.ink.retain(|s| s.id != id),
            Selection::Shapes(ids) => n.shapes.retain(|s| !ids.contains(&s.id)),
            Selection::Stroke(i) => {
                if i < n.strokes.len() {
                    n.strokes.remove(i);
                }
            }
        });
        self.selected.set(None);
        self.from.set(None);
        self.focus();
    }
    fn duplicate(self) {
        if let Some(Selection::Ink(id)) = self.selected.get_untracked() {
            let copy = self.e.workspace.with_untracked(|w| {
                w.notes[self.e.active.get_untracked()]
                    .ink
                    .iter()
                    .find(|s| s.id == id)
                    .cloned()
            });
            if let Some(mut copy) = copy {
                self.e.snapshot();
                copy.id = uid();
                let id = copy.id.clone();
                for p in &mut copy.points {
                    p.x = (p.x + 24.).clamp(-100000., 100000.);
                    p.y = (p.y + 24.).clamp(-100000., 100000.);
                }
                self.e.change(|n| n.ink.push(copy));
                self.select(Selection::Ink(id));
            }
            return;
        }
        if let Some(Selection::Shapes(ids)) = self.selected.get_untracked() {
            let shapes = self.e.workspace.with_untracked(|w| {
                w.notes[self.e.active.get_untracked()]
                    .shapes
                    .iter()
                    .filter(|s| ids.contains(&s.id))
                    .cloned()
                    .collect::<Vec<_>>()
            });
            if self.e.workspace.with_untracked(|w| {
                w.notes[self.e.active.get_untracked()].shapes.len() + shapes.len() > 5000
            }) {
                return;
            }
            self.e.snapshot();
            let mut copies = vec![];
            self.e.change(|n| {
                for mut shape in shapes {
                    shape.id = uid();
                    shape.x += 24.;
                    shape.y += 24.;
                    copies.push(shape.id.clone());
                    n.shapes.push(shape);
                }
            });
            self.select(Selection::Shapes(copies));
            return;
        }
        if self
            .e
            .workspace
            .with_untracked(|w| w.notes[self.e.active.get_untracked()].cards.len() >= 1000)
        {
            self.e.status.set("Card limit reached on this page.".into());
            return;
        }
        let Some(Selection::Card(id)) = self.selected.get_untracked() else {
            return;
        };
        let card = self.e.workspace.with_untracked(|w| {
            w.notes[self.e.active.get_untracked()]
                .cards
                .iter()
                .find(|c| c.id == id)
                .cloned()
        });
        if let Some(mut card) = card {
            let size = self.size(&card);
            card.id = uid();
            let id = card.id.clone();
            card.move_to(card.x + 24., card.y + 24., size);
            let note = self
                .e
                .workspace
                .with_untracked(|w| w.notes[self.e.active.get_untracked()].id.clone());
            self.e.insert(note, card);
            self.select(Selection::Card(id));
            self.focus();
        }
    }
    fn connect(self, id: String) {
        let Some(from) = self.from.get_untracked() else {
            self.from.set(Some(id.clone()));
            self.select(Selection::Card(id));
            return;
        };
        if from == id {
            return;
        }
        let available = self.e.workspace.with_untracked(|w| {
            let n = &w.notes[self.e.active.get_untracked()];
            n.connections.len() < canvas::MAX_CONNECTIONS
                && n.cards.iter().any(|c| c.id == from)
                && !n.connections.iter().any(|c| c.from == from && c.to == id)
        });
        if !available {
            self.e.status.set(
                "Those cards are already connected, or the connection limit was reached.".into(),
            );
            return;
        }
        let edge = uid();
        self.e.snapshot();
        self.e.change(|n| {
            n.connections.push(Connection {
                id: edge.clone(),
                from,
                to: id,
            })
        });
        self.from.set(None);
        self.tool.set("select");
        self.select(Selection::Connection(edge));
        self.focus();
    }
    fn begin_move(self, id: String, ev: PointerEvent) {
        if self.tool.get_untracked() != "select"
            || ev.button() != 0
            || self.gesture.with_untracked(|g| g.is_some())
            || !self.e.ready.get_untracked()
        {
            return;
        }
        ev.prevent_default();
        ev.stop_propagation();
        self.select(Selection::Card(id.clone()));
        let p = point(&ev, self.zoom.get_untracked());
        let position = self.e.workspace.with_untracked(|w| {
            w.notes[self.e.active.get_untracked()]
                .cards
                .iter()
                .find(|c| c.id == id)
                .map(|c| (c.x, c.y))
        });
        if let Some((x, y)) = position {
            self.e.snapshot();
            self.gesture.set(Some(Gesture::Move {
                pointer: ev.pointer_id(),
                id,
                dx: p.x - x,
                dy: p.y - y,
            }));
            capture(&ev);
        }
        self.focus();
    }
    fn begin_resize(self, id: String, ev: PointerEvent) {
        if ev.button() != 0
            || self.gesture.with_untracked(|g| g.is_some())
            || !self.e.ready.get_untracked()
        {
            return;
        }
        ev.prevent_default();
        ev.stop_propagation();
        let size = self.e.workspace.with_untracked(|w| {
            w.notes[self.e.active.get_untracked()]
                .cards
                .iter()
                .find(|c| c.id == id)
                .map(|c| self.size(c))
        });
        if let Some(size) = size {
            let p = point(&ev, self.zoom.get_untracked());
            self.e.snapshot();
            self.gesture.set(Some(Gesture::Resize {
                pointer: ev.pointer_id(),
                id,
                x: p.x,
                y: p.y,
                size,
            }));
            capture(&ev);
        }
    }
    fn resize_key(self, id: &str, ev: KeyboardEvent) {
        let Some((dx, dy)) = arrow(&ev) else {
            return;
        };
        ev.prevent_default();
        ev.stop_propagation();
        if !ev.repeat() {
            self.e.snapshot();
        }
        self.e.change(|n| {
            if let Some(c) = n.cards.iter_mut().find(|c| c.id == id) {
                let size = self.size(c);
                c.resize(size.width + dx, size.height + dy);
            }
        });
    }
    fn key(self, ev: KeyboardEvent) {
        let editing = ev
            .target()
            .and_then(|e| e.dyn_into::<web_sys::Element>().ok())
            .is_some_and(|e| {
                e.closest("input,textarea,select,[contenteditable='true']")
                    .ok()
                    .flatten()
                    .is_some()
            });
        if editing || !self.e.ready.get_untracked() {
            return;
        }
        if ev.key() == " "
            && self.tool.get_untracked() != "connect"
            && !ev
                .target()
                .and_then(|e| e.dyn_into::<web_sys::Element>().ok())
                .is_some_and(|e| e.tag_name() == "BUTTON")
        {
            ev.prevent_default();
            self.space.set(true);
            return;
        }
        if ev.key() == "Escape" {
            ev.prevent_default();
            self.gesture.set(None);
            self.from.set(None);
            self.selected.set(None);
            self.tool.set("select");
            return;
        }
        if (ev.ctrl_key() || ev.meta_key()) && !ev.alt_key() {
            match ev.key().to_lowercase().as_str() {
                "z" if ev.shift_key() => {
                    ev.prevent_default();
                    self.e.redo();
                }
                "y" => {
                    ev.prevent_default();
                    self.e.redo();
                }
                "a" => {
                    ev.prevent_default();
                    let ids = self.e.workspace.with_untracked(|w| {
                        w.notes[self.e.active.get_untracked()]
                            .shapes
                            .iter()
                            .map(|s| s.id.clone())
                            .collect()
                    });
                    self.select(Selection::Shapes(ids));
                }
                "z" if !ev.shift_key() => {
                    ev.prevent_default();
                    self.e.undo();
                }
                "d" => {
                    ev.prevent_default();
                    self.duplicate();
                }
                _ => {}
            }
            return;
        }
        if ev.alt_key() {
            return;
        }
        if ["Delete", "Backspace"].contains(&ev.key().as_str())
            && self.selected.get_untracked().is_some()
        {
            ev.prevent_default();
            self.remove();
            return;
        }
        if let Some(Selection::Ink(id)) = self.selected.get_untracked()
            && let Some((dx, dy)) = arrow(&ev)
        {
            ev.prevent_default();
            if !ev.repeat() {
                self.e.snapshot();
            }
            self.e.change(|n| {
                if let Some(stroke) = n.ink.iter_mut().find(|s| s.id == id) {
                    for p in &mut stroke.points {
                        p.x = (p.x + dx).clamp(-100000., 100000.);
                        p.y = (p.y + dy).clamp(-100000., 100000.);
                    }
                }
            });
            return;
        }
        if let Some(Selection::Shapes(ids)) = self.selected.get_untracked() {
            if let Some((dx, dy)) = arrow(&ev) {
                ev.prevent_default();
                if !ev.repeat() {
                    self.e.snapshot();
                }
                self.e.change(|n| {
                    for shape in &mut n.shapes {
                        if ids.contains(&shape.id) {
                            shape.x = (shape.x + dx).clamp(-100000., 100000.);
                            shape.y = (shape.y + dy).clamp(-100000., 100000.);
                        }
                    }
                });
                return;
            }
            if ev.key() == "Enter" && ids.len() == 1 {
                self.edit_text(ids[0].clone());
                return;
            }
        }
        if let Some(Selection::Card(id)) = self.selected.get_untracked() {
            if self.tool.get_untracked() == "connect" && ["Enter", " "].contains(&ev.key().as_str())
            {
                ev.prevent_default();
                self.connect(id);
                return;
            }
            if let Some((dx, dy)) = arrow(&ev) {
                ev.prevent_default();
                if !ev.repeat() {
                    self.e.snapshot();
                }
                self.e.change(|n| {
                    if let Some(c) = n.cards.iter_mut().find(|c| c.id == id) {
                        c.move_to(c.x + dx, c.y + dy, self.size(c));
                    }
                });
                return;
            }
        }
        let tool = match ev.key().as_str() {
            "v" | "V" | "1" => "select",
            "h" | "H" => "hand",
            "r" | "R" | "2" => "rectangle",
            "d" | "D" | "3" => "diamond",
            "o" | "O" | "4" => "ellipse",
            "a" | "A" | "5" => "arrow",
            "l" | "L" | "6" => "line",
            "p" | "P" | "7" => "pen",
            "t" | "T" | "8" => "text",
            "e" | "E" => "eraser",
            "c" | "C" => "connect",
            _ => return,
        };
        self.tool.set(tool);
        self.from.set(None);
    }
}
fn capture(ev: &PointerEvent) {
    if let Some(el) = ev
        .current_target()
        .and_then(|el| el.dyn_into::<web_sys::Element>().ok())
    {
        el.set_pointer_capture(ev.pointer_id()).ok();
    }
}
fn arrow(ev: &KeyboardEvent) -> Option<(f64, f64)> {
    let step = if ev.shift_key() { 40. } else { 10. };
    match ev.key().as_str() {
        "ArrowLeft" => Some((-step, 0.)),
        "ArrowRight" => Some((step, 0.)),
        "ArrowUp" => Some((0., -step)),
        "ArrowDown" => Some((0., step)),
        _ => None,
    }
}
fn description(card: &Card, w: &Workspace) -> String {
    match card.kind.as_str() {
        "image" => "image".into(),
        "email" => card
            .email
            .as_ref()
            .map(|e| e.subject.clone())
            .unwrap_or_else(|| "email".into()),
        "note" => w
            .notes
            .iter()
            .find(|n| n.id == card.text)
            .map(|n| n.title.clone())
            .unwrap_or_else(|| "linked note".into()),
        _ => card
            .text
            .lines()
            .next()
            .unwrap_or("card")
            .chars()
            .take(50)
            .collect(),
    }
}
type CardObserver =
    StoredValue<(web_sys::ResizeObserver, Closure<dyn FnMut(js_sys::Array)>), LocalStorage>;

#[component]
pub fn Canvas(
    e: Editor,
    picker: RwSignal<Option<&'static str>>,
    picked: RwSignal<String>,
    note_catalog: Memo<Vec<NoteLink>>,
) -> impl IntoView {
    let b = Board {
        e,
        tool: RwSignal::new("select"),
        zoom: RwSignal::new(1.),
        selected: RwSignal::new(None),
        gesture: RwSignal::new(None),
        from: RwSignal::new(None),
        sizes: RwSignal::new(HashMap::new()),
        pan: RwSignal::new(Point { x: 0., y: 0. }),
        space: RwSignal::new(false),
        style: RwSignal::new(Style::default()),
        text_edit: RwSignal::new(None),
        locked: RwSignal::new(false),
        gesture_saved: RwSignal::new(false),
    };
    let link_open = RwSignal::new(false);
    let link = RwSignal::new(String::new());
    let insert_open = RwSignal::new(false);
    let properties_open = RwSignal::new(false);
    Effect::new(move |_| {
        let _ = e.active.get();
        let _ = e.workspace_epoch.get();
        let _ = e.ready.get();
        b.gesture.set(None);
        b.text_edit.set(None);
        b.selected.set(None);
        b.from.set(None);
        b.sizes.set(HashMap::new());
    });
    let callback = Closure::<dyn FnMut(js_sys::Array)>::new(move |entries: js_sys::Array| {
        let mut changed = Vec::new();
        for entry in entries.iter() {
            let entry: web_sys::ResizeObserverEntry = entry.unchecked_into();
            let target = entry.target();
            if !target.is_connected() {
                continue;
            }
            if let Some(id) = target.get_attribute("data-card-id") {
                let rect = target.get_bounding_client_rect();
                let zoom = b.zoom.get_untracked();
                let size = Size {
                    width: rect.width() / zoom,
                    height: rect.height() / zoom,
                };
                if size.width <= 0. || size.height <= 0. {
                    continue;
                }
                if b.sizes.with_untracked(|sizes| {
                    sizes.get(&id).is_none_or(|s| {
                        (s.width - size.width).abs() > 0.1 || (s.height - size.height).abs() > 0.1
                    })
                }) {
                    changed.push((id, size));
                }
            }
        }
        if !changed.is_empty() {
            b.sizes.update(|sizes| sizes.extend(changed));
        }
    });
    let observer = web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref())
        .expect("ResizeObserver is required by current browsers");
    let observer = StoredValue::new_local((observer, callback));
    on_cleanup(move || observer.with_value(|(observer, _)| observer.disconnect()));
    view! {
        <div class="canvas-wrap drawing-surface" class:properties-open=move ||properties_open.get() on:keydown=move |ev|b.key(ev) on:keyup=move |ev|{if ev.key()==" "{b.space.set(false);}}>
            <div class="drawing-inserts"><button aria-label="Insert into canvas" on:click=move |_|insert_open.update(|v|*v = !*v)><Icon name="plus"/>"Insert"</button>
                <Show when=move ||insert_open.get()><div class="drawing-insert-menu">
                    <button on:click=move |_|{picked.set(String::new());picker.set(Some("canvas"));insert_open.set(false);}>"Linked page"</button>
                    <button aria-label="Add text card" on:click=move |_|{let p=b.centre();e.add("text","A new thought…".into(),p.x,p.y);insert_open.set(false);}>"Note card"</button>
                    <button aria-label="Add sticky note" on:click=move |_|{let p=b.centre();e.add("sticky","Something worth keeping…".into(),p.x,p.y);insert_open.set(false);}>"Sticky note"</button>
                    <button aria-label="Add website" on:click=move |_|{link_open.set(true);insert_open.set(false);}>"Website link"</button>
                    <button aria-label="Connect cards" on:click=move |_|{b.tool.set("connect");b.from.set(None);insert_open.set(false);}>"Connect note cards"</button>
                    <label class="image-picker" title="Add email">"Email file"<input type="file" accept=".eml,message/rfc822" aria-label="Add email" on:change=move |ev|{let input:web_sys::HtmlInputElement=event_target(&ev);if let Some(f)=input.files().and_then(|fs|fs.get(0)){let p=b.centre();read_capture(f,e,p.x,p.y);}input.set_value("");insert_open.set(false);}/></label>
                </div></Show>
            </div>
            <div class="drawing-toolbar" role="toolbar" aria-label="Drawing tools">
                {[("hand","hand","Pan","H"),("select","cursor","Select and move","V"),("rectangle","rectangle","Rectangle","R"),("diamond","diamond","Diamond","D"),("ellipse","ellipse","Ellipse","O"),("arrow","arrow","Arrow","A"),("line","line","Line","L"),("pen","pen","Draw","P"),("text","text","Text","T"),("eraser","eraser","Eraser","E")].into_iter().map(|(tool,icon,label,key)|view!{<button class:chosen=move ||b.tool.get()==tool aria-label=label title=format!("{label} ({key})") on:click=move |_|{b.tool.set(tool);b.from.set(None);b.text_edit.set(None);insert_open.set(false);if tool!="select"{b.selected.set(None);}b.focus();}><Icon name=icon/><span class="tool-key">{key}</span></button>}).collect_view()}
                <label class="image-picker" title="Insert image"><Icon name="image"/><input type="file" accept="image/png,image/jpeg,image/webp,image/gif" aria-label="Add image" on:change=move |ev|{let input:web_sys::HtmlInputElement=event_target(&ev);if let Some(f)=input.files().and_then(|fs|fs.get(0)){let p=b.centre();read_capture(f,e,p.x,p.y);}input.set_value("");}/></label>
                <button class:chosen=move ||b.locked.get() aria-label="Keep drawing tool active" title="Keep tool active" aria-pressed=move ||b.locked.get() on:click=move |_|b.locked.update(|v|*v = !*v)><Icon name="lock"/></button>
            </div>
            <button class="properties-toggle" aria-label="Toggle drawing properties" on:click=move |_|properties_open.update(|v|*v = !*v)>"Style"</button>
            <DrawingProperties b=b/>
            <Show when=move ||link_open.get()><form class="link-form" on:submit=move |ev|{ev.prevent_default();let url=link.get_untracked();if !notepad_core::safe_link(&url){e.status.set("Enter a full http:// or https:// website address.".into());return;}let p=b.centre();e.add("link",url,p.x,p.y);link.set(String::new());link_open.set(false);} ><label for="website-url">"Website address"</label><input id="website-url" type="url" required placeholder="https://…" prop:value=move ||link.get() on:input=move |ev|link.set(event_target_value(&ev))/><button class="primary" type="submit">"Add link"</button><button type="button" on:click=move |_|link_open.set(false)>"Cancel"</button></form></Show>
            <div id="drawing-viewport" class="drawing-viewport" class:hand-mode=move ||b.tool.get()=="hand"||b.space.get() class:pen-mode=move ||b.tool.get()=="pen" class:eraser-mode=move ||b.tool.get()=="eraser"
                on:pointerdown=move |ev|b.start(ev) on:pointermove=move |ev|b.motion(ev) on:pointerup=move |ev|b.finish(ev)
                on:pointercancel=move |_|{b.gesture.set(None);b.space.set(false);} on:lostpointercapture=move |_|b.gesture.set(None)
                on:wheel=move |ev|{ev.prevent_default();if ev.ctrl_key()||ev.meta_key(){let factor=(-ev.delta_y()*0.005).exp();b.zoom_at(b.zoom.get_untracked()*factor,ev.client_x() as f64,ev.client_y() as f64);}else{b.pan.update(|p|{p.x-=ev.delta_x();p.y-=ev.delta_y();});}}
                on:dragover=move |ev|ev.prevent_default() on:drop=move |ev|{ev.prevent_default();let p=b.position(ev.client_x() as f64,ev.client_y() as f64);if let Some(dt)=ev.data_transfer(){if let Some(files)=dt.files()&&files.length()>0{for i in 0..files.length().min(8){if let Some(f)=files.get(i){read_capture(f,e,p.x+i as f64*24.,p.y+i as f64*24.);}}return;}let url=dt.get_data("text/uri-list").unwrap_or_default().lines().find(|s|!s.starts_with('#')).unwrap_or("").to_string();let text=if url.is_empty(){dt.get_data("text/plain").unwrap_or_default()}else{url};if !text.trim().is_empty(){e.add(if notepad_core::safe_link(text.trim()){"link"}else{"text"},text.trim().chars().take(100000).collect(),p.x,p.y);}}}>
                <div id="world" class="world drawing-world" tabindex="0" aria-label="Drawing canvas. V select, R rectangle, O ellipse, A arrow, T text, space to pan." class:pen-mode=move ||b.tool.get()=="pen" class:connect-mode=move ||b.tool.get()=="connect" style=move ||{let p=b.pan.get();format!("width:100%;height:100%;background:none;transform:translate({}px,{}px) scale({})",p.x,p.y,b.zoom.get())}>
                    <svg class="ink drawing-layer" width="100%" height="100%" style="overflow:visible" aria-label="Drawing elements">
                        <defs><marker id="canvas-arrow" markerWidth="8" markerHeight="8" refX="10" refY="5" orient="auto" viewBox="0 0 10 10"><path class="arrow-head" d="M0 0L10 5L0 10Z"/></marker></defs>
                        <For each=move ||e.workspace.with(|w|w.notes[e.active.get()].connections.iter().map(|c|c.id.clone()).collect::<Vec<_>>()) key=move |id|(e.workspace_epoch.get(),e.active.get(),id.clone()) children=move |id|view!{<ConnectionLine b=b id=id/>}/>
                        <For each=move ||e.workspace.with(|w|(0..w.notes[e.active.get()].strokes.len()).collect::<Vec<_>>()) key=move |i|(e.workspace_epoch.get(),e.active.get(),*i) children=move |i|view!{<DrawingStroke b=b index=i/>}/>
                        <For each=move ||e.workspace.with(|w|w.notes[e.active.get()].shapes.iter().map(|s|s.id.clone()).collect::<Vec<_>>()) key=move |id|(e.workspace_epoch.get(),e.active.get(),id.clone()) children=move |id|view!{<VectorShape b=b id=id/>}/>
                        <For each=move ||e.workspace.with(|w|w.notes[e.active.get()].ink.iter().map(|s|s.id.clone()).collect::<Vec<_>>()) key=move |id|(e.workspace_epoch.get(),e.active.get(),id.clone()) children=move |id|view!{<StyledInk b=b id=id/>}/>
                        {move ||match b.gesture.get(){Some(Gesture::Marquee{start,end,..})=>Some(view!{<rect class="marquee" x=start.x.min(end.x) y=start.y.min(end.y) width=(end.x-start.x).abs() height=(end.y-start.y).abs() style="fill:var(--color-accent-soft);stroke:var(--color-accent);opacity:0.5;pointer-events:none"/>}),_=>None}}
                    </svg>
                    <For each=move ||e.workspace.with(|w|w.notes[e.active.get()].cards.iter().map(|c|c.id.clone()).collect::<Vec<_>>()) key=move |id|(e.workspace_epoch.get(),e.active.get(),id.clone()) children=move |id|{let card=e.workspace.with_untracked(|w|w.notes[e.active.get_untracked()].cards.iter().find(|c|c.id==id).unwrap().clone());view!{<BoardCard b=b card=card observer=observer note_catalog=note_catalog/>}}/>
                    <Show when=move ||b.text_edit.get().is_some()><DrawingTextEditor b=b/></Show>
                </div>
                <Show when=move ||e.workspace.with(|w|{let n=&w.notes[e.active.get()];n.cards.is_empty()&&n.shapes.is_empty()&&n.ink.is_empty()&&n.strokes.is_empty()})><div class="drawing-empty-hint">"Pick a tool and draw. Or drop an image, link or note."</div></Show>
            </div>
            <div class="drawing-footer"><div class="drawing-navigation"><button aria-label="Zoom out" on:click=move |_|b.zoom_centre(b.zoom.get_untracked()/1.2)>"−"</button><button aria-label="Reset zoom" on:click=move |_|b.zoom_centre(1.)>{move ||format!("{}%",(b.zoom.get()*100.).round() as i32)}</button><button aria-label="Zoom in" on:click=move |_|b.zoom_centre(b.zoom.get_untracked()*1.2)>"+"</button><button aria-label="Fit drawing" title="Fit drawing" on:click=move |_|b.fit()>"Fit"</button><span class="tool-divider"></span><button aria-label="Undo" title="Undo (Ctrl/Cmd+Z)" disabled=move ||e.history.with(|h|h.is_empty()) on:click=move |_|e.undo()><Icon name="undo"/></button><button aria-label="Redo" title="Redo (Ctrl/Cmd+Shift+Z)" disabled=move ||e.future.with(|h|h.is_empty()) on:click=move |_|e.redo()><Icon name="redo"/></button></div>
                <Show when=move ||b.selected.get().is_some()><div class="drawing-selection-actions"><button on:click=move |_|b.duplicate()>"Duplicate"</button><button on:click=move |_|b.remove()>"Delete"</button></div></Show><span class="drawing-help">{move ||if b.tool.get()=="connect"{"Choose two note cards to connect"}else{"Space to pan · Ctrl/⌘ + wheel to zoom"}}</span>
            </div>
        </div>
    }
}

#[component]
fn BoardCard(
    b: Board,
    card: Card,
    observer: CardObserver,
    note_catalog: Memo<Vec<NoteLink>>,
) -> impl IntoView {
    let e = b.e;
    let id = StoredValue::new(card.id.clone());
    let node = NodeRef::<leptos::html::Article>::new();
    let observed = StoredValue::new_local(None::<web_sys::Element>);
    Effect::new(move |_| {
        if let Some(node) = node.get() {
            let el: web_sys::Element = node.unchecked_into();
            observer.with_value(|(o, _)| o.observe(&el));
            observed.set_value(Some(el));
        }
    });
    on_cleanup(move || {
        if let Some(el) = observed.get_value() {
            observer.try_with_value(|(o, _)| o.unobserve(&el));
        }
    });
    let kind = card.kind.clone();
    let value_kind = kind.clone();
    let edit_id = card.id.clone();
    let focus_id = card.id.clone();
    let value_id = card.id.clone();
    view! {
        <article node_ref=node id=format!("canvas-card-{}",card.id) data-card-id=card.id.clone() class=format!("canvas-card {}",kind)
            class:selected=move ||b.selected.get()==Some(Selection::Card(id.get_value()))
            class:sized=move ||e.workspace.with(|w|w.notes[e.active.get()].cards.iter().find(|c|c.id==id.get_value()).is_some_and(|c|c.size.is_some()))
            style=move ||e.workspace.with(|w|w.notes[e.active.get()].cards.iter().find(|c|c.id==id.get_value()).map(|c|format!("transform:translate({}px,{}px);{}",c.x,c.y,c.size.map(|s|format!("width:{}px;height:{}px",s.width,s.height)).unwrap_or_default())).unwrap_or_default())
            on:pointerdown=move |ev|{if b.tool.get_untracked()=="connect"{ev.prevent_default();ev.stop_propagation();}}
            on:click=move |ev|{if b.tool.get_untracked()=="connect"{ev.prevent_default();ev.stop_propagation();b.connect(id.get_value());}else if b.tool.get_untracked()=="select"{b.select(Selection::Card(id.get_value()));}}>
            <div class="card-handle" tabindex="0" role="group" aria-label="Move card. Use arrow keys to move, Shift for larger steps. Delete removes it." on:focus=move |_|b.select(Selection::Card(id.get_value())) on:pointerdown=move |ev|b.begin_move(id.get_value(),ev)>
                <span>{match kind.as_str(){"sticky"=>"Keep in mind","image"=>"Image","email"=>"Email","note"=>"Linked note","link"=>"From the web",_=>"A thought"}}</span>
                <button aria-label="Remove card" title="Remove card (can undo)" on:pointerdown=move |ev|ev.stop_propagation() on:click=move |ev|{ev.stop_propagation();b.select(Selection::Card(id.get_value()));b.remove();}><Icon name="close"/></button>
            </div>
                                    {if kind=="note" {
                                        let target=card.text.clone();let title_target=target.clone();let content_target=target.clone();let open_target=target.clone();
                                        let linked_title=Memo::new(move |_|e.workspace.with(|w|w.notes.iter().find(|n|n.id==title_target).map(|n|n.title.clone())));
                                        let linked_body=Memo::new(move |_|e.workspace.with(|w|w.notes.iter().find(|n|n.id==content_target).map(|n|n.body.chars().take(2000).collect::<String>()).unwrap_or_default()));
                                        let preview=Memo::new(move |_|markdown::render_markdown(&linked_body.get(),&note_catalog.get()));
                                        view!{<div class="note-card-content"><h3>{move ||linked_title.get().unwrap_or_else(||"Note not found".into())}</h3><div class="markdown-body note-card-preview" inner_html=move ||preview.get()></div><a class="open-note" data-note-id=open_target.clone() href=markdown::note_href(&open_target,"read")>"Open note"</a></div>}.into_any()
                                    } else if kind=="email" {
                                        let email=card.email.clone().expect("validated email card");
                                        view!{<section class="email-content"><h3>{email.subject}</h3><dl><dt>"From"</dt><dd>{if email.sender.is_empty() {"Sender not provided".into()} else {email.sender}}</dd><dt>"To"</dt><dd>{if email.recipient.is_empty() {"Recipient not provided".into()} else {email.recipient}}</dd></dl><p class="email-date">{email.date}</p><div class="email-body" tabindex="0" aria-label="Email text">{card.text.clone()}</div><div class="email-footer"><a href=format!("data:message/rfc822;base64,{}",email.original) download="notepad-email.eml"><Icon name="download"/>"Original .eml"</a>{(email.attachments>0).then(||view!{<span>{format!("{} attachment(s) in original",email.attachments)}</span>})}</div></section>}.into_any()
                                    } else if kind=="image" {view!{<img class="attachment" src=card.text.clone() alt="Dropped image" draggable="false"/>}.into_any()} else if kind=="link" {let url=card.text.clone();view!{<a class="web-link" href=url.clone() target="_blank" rel="noopener noreferrer"><span class="link-symbol"><Icon name="link"/></span><strong>{url.trim_start_matches("https://").trim_start_matches("http://").split('/').next().unwrap_or("").to_string()}</strong><span>{url.clone()}</span><small>"Open website"</small></a>}.into_any()} else {view!{<textarea maxlength="100000" aria-label=if value_kind=="sticky" {"Sticky note content"} else {"Card content"} prop:value=move || e.workspace.get().notes[e.active.get()].cards.iter().find(|c| c.id==value_id).map(|c|c.text.clone()).unwrap_or_default() on:focus=move |_| {b.select(Selection::Card(focus_id.clone()));e.snapshot();} on:input=move |ev| {let text=event_target_value(&ev);e.change(|n| {if let Some(c)=n.cards.iter_mut().find(|c| c.id==edit_id) {c.text=text;}});} />}.into_any()}}
            <Show when=move ||b.selected.get()==Some(Selection::Card(id.get_value()))&&b.tool.get()=="select">
                <button class="resize-handle" aria-label="Resize card. Arrow keys change width and height, Shift for larger steps." title="Resize card" on:pointerdown=move |ev|b.begin_resize(id.get_value(),ev) on:keydown=move |ev|b.resize_key(&id.get_value(),ev) on:click=move |ev|ev.stop_propagation()><Icon name="resize"/></button>
            </Show>
        </article>
    }
}

#[component]
fn ConnectionLine(b: Board, id: String) -> impl IntoView {
    let id = StoredValue::new(id);
    let details = Memo::new(move |_| {
        b.e.workspace.with(|w| {
            let n = &w.notes[b.e.active.get()];
            let edge = n.connections.iter().find(|c| c.id == id.get_value())?;
            let from = n.cards.iter().find(|c| c.id == edge.from)?;
            let to = n.cards.iter().find(|c| c.id == edge.to)?;
            let (start, end) = canvas::endpoints(b.rect(from), b.rect(to));
            Some((
                format!("M{} {}L{} {}", start.x, start.y, end.x, end.y),
                format!(
                    "Connection from {} to {}. Delete removes it.",
                    description(from, w),
                    description(to, w)
                ),
            ))
        })
    });
    view! {<g class="connection" data-connection-id=move ||id.get_value() class:selected=move ||b.selected.get()==Some(Selection::Connection(id.get_value())) tabindex="0" role="img" aria-label=move ||details.with(|d|d.as_ref().map(|(_,label)|label.clone()).unwrap_or_default()) on:focus=move |_|b.select(Selection::Connection(id.get_value())) on:pointerdown=move |ev|{if b.tool.get_untracked()=="select"&&ev.button()==0{ev.prevent_default();ev.stop_propagation();b.select(Selection::Connection(id.get_value()));b.focus();}}>
        <path class="connection-line" d=move ||details.with(|d|d.as_ref().map(|(path,_)|path.clone()).unwrap_or_default()) marker-end="url(#canvas-arrow)"/>
        <path class="connection-hit" d=move ||details.with(|d|d.as_ref().map(|(path,_)|path.clone()).unwrap_or_default())/>
    </g>}
}
#[component]
fn DrawingStroke(b: Board, index: usize) -> impl IntoView {
    let path = Memo::new(move |_| {
        b.e.workspace.with(|w| {
            let Some(s) = w.notes[b.e.active.get()].strokes.get(index) else {
                return String::new();
            };
            let mut d = s
                .iter()
                .enumerate()
                .map(|(j, p)| format!("{}{} {}", if j == 0 { "M" } else { "L" }, p.x, p.y))
                .collect::<Vec<_>>()
                .join(" ");
            if s.len() == 1 {
                d.push_str("l0.01 0");
            }
            d
        })
    });
    view! {<g class="drawing-stroke" data-stroke=index class:selected=move ||b.selected.get()==Some(Selection::Stroke(index)) tabindex="0" role="img" aria-label=format!("Drawing {}. Delete removes it.",index+1) on:focus=move |_|b.select(Selection::Stroke(index)) on:pointerdown=move |ev|{if b.tool.get_untracked()=="select"&&ev.button()==0{ev.prevent_default();ev.stop_propagation();b.select(Selection::Stroke(index));b.focus();}}><path class="stroke-line" d=move ||path.get()/><path class="stroke-hit" d=move ||path.get()/></g>}
}

fn shape_kind(tool: &str) -> Option<ShapeKind> {
    match tool {
        "rectangle" => Some(ShapeKind::Rectangle),
        "diamond" => Some(ShapeKind::Diamond),
        "ellipse" => Some(ShapeKind::Ellipse),
        "arrow" => Some(ShapeKind::Arrow),
        "line" => Some(ShapeKind::Line),
        "text" => Some(ShapeKind::Text),
        _ => None,
    }
}
impl Board {
    fn position(self, x: f64, y: f64) -> Point {
        let r = document()
            .get_element_by_id("world")
            .unwrap()
            .get_bounding_client_rect();
        Point {
            x: ((x - r.x()) / self.zoom.get_untracked()).clamp(-100000., 100000.),
            y: ((y - r.y()) / self.zoom.get_untracked()).clamp(-100000., 100000.),
        }
    }
    fn centre(self) -> Point {
        let r = document()
            .get_element_by_id("drawing-viewport")
            .unwrap()
            .get_bounding_client_rect();
        self.position(r.x() + r.width() / 2., r.y() + r.height() / 2.)
    }
    fn shape_selected(self, id: &str) -> bool {
        self.selected
            .with(|s| matches!(s,Some(Selection::Shapes(ids)) if ids.iter().any(|s|s==id)))
    }
    fn change_style(self, f: impl FnOnce(&mut Style)) {
        let mut style = self.style.get_untracked();
        f(&mut style);
        self.style.set(style.clone());
        if let Some(selection) = self.selected.get_untracked() {
            self.e.snapshot();
            self.e.change(|n| match selection {
                Selection::Shapes(ids) => {
                    for s in &mut n.shapes {
                        if ids.contains(&s.id) {
                            s.style = style.clone();
                        }
                    }
                }
                Selection::Ink(id) => {
                    if let Some(s) = n.ink.iter_mut().find(|s| s.id == id) {
                        s.style = style;
                    }
                }
                _ => {}
            });
        }
    }
    fn zoom_at(self, zoom: f64, x: f64, y: f64) {
        let r = document()
            .get_element_by_id("drawing-viewport")
            .unwrap()
            .get_bounding_client_rect();
        let before = self.position(x, y);
        let zoom = zoom.clamp(0.1, 4.);
        self.zoom.set(zoom);
        self.pan.set(Point {
            x: x - r.x() - before.x * zoom,
            y: y - r.y() - before.y * zoom,
        });
    }
    fn zoom_centre(self, zoom: f64) {
        let r = document()
            .get_element_by_id("drawing-viewport")
            .unwrap()
            .get_bounding_client_rect();
        self.zoom_at(zoom, r.x() + r.width() / 2., r.y() + r.height() / 2.);
    }
    fn edit_text(self, id: String) {
        let style = self.e.workspace.with_untracked(|w| {
            w.notes[self.e.active.get_untracked()]
                .shapes
                .iter()
                .find(|s| s.id == id && s.kind == ShapeKind::Text)
                .map(|s| s.style.clone())
        });
        if let Some(style) = style {
            self.e.snapshot();
            self.style.set(style);
            self.selected.set(Some(Selection::Shapes(vec![id.clone()])));
            self.text_edit.set(Some(id));
        }
    }
    fn shape_down(self, id: String, ev: PointerEvent) {
        if self.tool.get_untracked() != "select" || ev.button() != 0 || self.space.get_untracked() {
            return;
        }
        ev.prevent_default();
        ev.stop_propagation();
        let mut ids = match self.selected.get_untracked() {
            Some(Selection::Shapes(ids)) => ids,
            _ => vec![],
        };
        if ev.shift_key() {
            if ids.contains(&id) {
                ids.retain(|s| s != &id);
            } else {
                ids.push(id.clone());
            }
        } else if !ids.contains(&id) {
            ids = vec![id.clone()];
        }
        self.selected.set(Some(Selection::Shapes(ids.clone())));
        self.text_edit.set(None);
        let originals = self.e.workspace.with_untracked(|w| {
            w.notes[self.e.active.get_untracked()]
                .shapes
                .iter()
                .filter(|s| ids.contains(&s.id))
                .map(|s| (s.id.clone(), s.x, s.y))
                .collect()
        });
        if let Some(style) = self.e.workspace.with_untracked(|w| {
            w.notes[self.e.active.get_untracked()]
                .shapes
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.style.clone())
        }) {
            self.style.set(style);
        }
        self.gesture_saved.set(false);
        self.gesture.set(Some(Gesture::VectorMove {
            pointer: ev.pointer_id(),
            start: point(&ev, self.zoom.get_untracked()),
            originals,
        }));
        capture(&ev);
        self.focus();
    }
    fn vector_resize(self, id: String, ev: PointerEvent) {
        if ev.button() != 0 {
            return;
        }
        ev.prevent_default();
        ev.stop_propagation();
        let shape = self.e.workspace.with_untracked(|w| {
            w.notes[self.e.active.get_untracked()]
                .shapes
                .iter()
                .find(|s| s.id == id)
                .cloned()
        });
        if let Some(shape) = shape {
            self.gesture_saved.set(false);
            self.gesture.set(Some(Gesture::VectorResize {
                pointer: ev.pointer_id(),
                id,
                start: point(&ev, self.zoom.get_untracked()),
                shape,
            }));
            capture(&ev);
        }
    }
    fn start(self, ev: PointerEvent) {
        if self.gesture.with_untracked(|g| g.is_some()) || !self.e.ready.get_untracked() {
            return;
        }
        if ev.button() == 1 || self.space.get_untracked() || self.tool.get_untracked() == "hand" {
            ev.prevent_default();
            self.text_edit.set(None);
            self.gesture.set(Some(Gesture::Pan {
                pointer: ev.pointer_id(),
                x: ev.client_x() as f64,
                y: ev.client_y() as f64,
                origin: self.pan.get_untracked(),
            }));
            capture(&ev);
            return;
        }
        if ev.button() != 0 {
            return;
        }
        // Card controls keep normal focus, links and native text selection.
        if ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            .is_some_and(|target| {
                target
                    .closest(".canvas-card, .drawing-text-editor")
                    .ok()
                    .flatten()
                    .is_some()
            })
        {
            return;
        }
        let p = point(&ev, self.zoom.get_untracked());
        self.text_edit.set(None);
        if self.tool.get_untracked() == "eraser" {
            ev.prevent_default();
            self.e.snapshot();
            self.gesture.set(Some(Gesture::Erase {
                pointer: ev.pointer_id(),
            }));
            capture(&ev);
            self.erase_at(ev.client_x() as f32, ev.client_y() as f32);
            return;
        }
        if let Some(kind) = shape_kind(self.tool.get_untracked()) {
            if self
                .e
                .workspace
                .with_untracked(|w| w.notes[self.e.active.get_untracked()].shapes.len() >= 5000)
            {
                self.e
                    .status
                    .set("This board has reached its shape limit.".into());
                return;
            }
            ev.prevent_default();
            self.e.snapshot();
            let id = uid();
            let end = if kind == ShapeKind::Text {
                Point {
                    x: p.x + 240.,
                    y: p.y + 40.,
                }
            } else {
                Point {
                    x: p.x + 1.,
                    y: p.y + 1.,
                }
            };
            let shape =
                Shape::from_drag(id.clone(), kind, p.clone(), end, self.style.get_untracked());
            self.e.change(|n| n.shapes.push(shape));
            self.selected.set(Some(Selection::Shapes(vec![id.clone()])));
            if kind == ShapeKind::Text {
                self.text_edit.set(Some(id));
                if !self.locked.get_untracked() {
                    self.tool.set("select");
                }
                return;
            }
            self.gesture.set(Some(Gesture::Shape {
                pointer: ev.pointer_id(),
                id,
                start: p,
                kind,
            }));
            capture(&ev);
            self.focus();
            return;
        }
        if self.tool.get_untracked() == "pen" {
            if self
                .e
                .workspace
                .with_untracked(|w| w.notes[self.e.active.get_untracked()].ink.len() >= 10000)
            {
                return;
            }
            ev.prevent_default();
            self.e.snapshot();
            self.selected.set(None);
            self.e.change(|n| {
                n.ink.push(InkStroke {
                    id: uid(),
                    points: vec![p],
                    style: self.style.get_untracked(),
                })
            });
            self.gesture.set(Some(Gesture::Draw {
                pointer: ev.pointer_id(),
            }));
            capture(&ev);
            self.focus();
            return;
        }
        if self.tool.get_untracked() == "select" {
            ev.prevent_default();
            self.selected.set(None);
            self.gesture.set(Some(Gesture::Marquee {
                pointer: ev.pointer_id(),
                start: p.clone(),
                end: p,
            }));
            capture(&ev);
            self.focus();
        }
    }
    fn motion(self, ev: PointerEvent) {
        let Some(gesture) = self
            .gesture
            .get_untracked()
            .filter(|g| g.pointer() == ev.pointer_id())
        else {
            return;
        };
        let p = point(&ev, self.zoom.get_untracked());
        if matches!(
            gesture,
            Gesture::VectorMove { .. } | Gesture::VectorResize { .. } | Gesture::InkMove { .. }
        ) && !self.gesture_saved.get_untracked()
        {
            self.e.snapshot();
            self.gesture_saved.set(true);
        }
        match gesture {
            Gesture::Erase { .. } => self.erase_at(ev.client_x() as f32, ev.client_y() as f32),
            Gesture::Pan { x, y, origin, .. } => self.pan.set(Point {
                x: origin.x + ev.client_x() as f64 - x,
                y: origin.y + ev.client_y() as f64 - y,
            }),
            Gesture::Marquee { pointer, start, .. } => self.gesture.set(Some(Gesture::Marquee {
                pointer,
                start,
                end: p,
            })),
            other => self.e.change(|n| match other {
                Gesture::InkMove {
                    id, start, points, ..
                } => {
                    if let Some(stroke) = n.ink.iter_mut().find(|s| s.id == id) {
                        stroke.points = points
                            .into_iter()
                            .map(|original| Point {
                                x: (original.x + p.x - start.x).clamp(-100000., 100000.),
                                y: (original.y + p.y - start.y).clamp(-100000., 100000.),
                            })
                            .collect();
                    }
                }
                Gesture::Draw { .. } => {
                    if let Some(s) = n.ink.last_mut()
                        && s.points.len() < 50000
                    {
                        s.points.push(p);
                    }
                }
                Gesture::Move { id, dx, dy, .. } => {
                    if let Some(c) = n.cards.iter_mut().find(|c| c.id == id) {
                        c.move_to(p.x - dx, p.y - dy, self.size(c));
                    }
                }
                Gesture::Resize { id, x, y, size, .. } => {
                    if let Some(c) = n.cards.iter_mut().find(|c| c.id == id) {
                        c.resize(size.width + p.x - x, size.height + p.y - y);
                    }
                }
                Gesture::Shape {
                    id, start, kind, ..
                } => {
                    let mut end = p;
                    if ev.shift_key() {
                        let dx = end.x - start.x;
                        let dy = end.y - start.y;
                        if matches!(kind, ShapeKind::Line | ShapeKind::Arrow) {
                            let angle = (dy.atan2(dx) / (std::f64::consts::PI / 4.)).round()
                                * (std::f64::consts::PI / 4.);
                            let length = dx.hypot(dy);
                            end = Point {
                                x: start.x + length * angle.cos(),
                                y: start.y + length * angle.sin(),
                            };
                        } else {
                            let side = dx.abs().max(dy.abs());
                            end = Point {
                                x: start.x + side * dx.signum(),
                                y: start.y + side * dy.signum(),
                            };
                        }
                    }
                    if let Some(s) = n.shapes.iter_mut().find(|s| s.id == id) {
                        let style = s.style.clone();
                        *s = Shape::from_drag(id, kind, start, end, style);
                    }
                }
                Gesture::VectorMove {
                    start, originals, ..
                } => {
                    for (id, x, y) in originals {
                        if let Some(s) = n.shapes.iter_mut().find(|s| s.id == id) {
                            s.x = (x + p.x - start.x).clamp(-90000., 90000.);
                            s.y = (y + p.y - start.y).clamp(-90000., 90000.);
                        }
                    }
                }
                Gesture::VectorResize {
                    id, start, shape, ..
                } => {
                    if let Some(s) = n.shapes.iter_mut().find(|s| s.id == id) {
                        s.width = shape.width + p.x - start.x;
                        s.height = shape.height + p.y - start.y;
                        if !matches!(s.kind, ShapeKind::Line | ShapeKind::Arrow) {
                            s.width = s.width.max(8.);
                            s.height = s.height.max(8.);
                        }
                    }
                }
                _ => {}
            }),
        }
    }
    fn finish(self, ev: PointerEvent) {
        let Some(gesture) = self
            .gesture
            .get_untracked()
            .filter(|g| g.pointer() == ev.pointer_id())
        else {
            return;
        };
        self.gesture.set(None);
        match gesture {
            Gesture::Marquee { start, end, .. } => {
                let x = start.x.min(end.x);
                let y = start.y.min(end.y);
                let right = start.x.max(end.x);
                let bottom = start.y.max(end.y);
                let ids = self.e.workspace.with_untracked(|w| {
                    w.notes[self.e.active.get_untracked()]
                        .shapes
                        .iter()
                        .filter(|s| {
                            let b = s.bounds();
                            b.x >= x
                                && b.y >= y
                                && b.x + b.size.width <= right
                                && b.y + b.size.height <= bottom
                        })
                        .map(|s| s.id.clone())
                        .collect()
                });
                self.selected.set(Some(Selection::Shapes(ids)));
            }
            Gesture::Shape { id, kind, .. } => {
                self.e.change(|n| {
                    if let Some(s) = n.shapes.iter_mut().find(|s| s.id == id)
                        && s.width.abs() < 4.
                        && s.height.abs() < 4.
                    {
                        s.width = 120.;
                        s.height = if matches!(kind, ShapeKind::Line | ShapeKind::Arrow) {
                            0.
                        } else {
                            80.
                        };
                    }
                });
                if !self.locked.get_untracked() {
                    self.tool.set("select");
                }
            }
            _ => {}
        }
    }
    fn fit(self) {
        let bounds = self.e.workspace.with_untracked(|w| {
            let n = &w.notes[self.e.active.get_untracked()];
            let mut boxes = n.shapes.iter().map(|s| s.bounds()).collect::<Vec<_>>();
            boxes.extend(n.cards.iter().map(|c| self.rect(c)));
            for s in &n.ink {
                for p in &s.points {
                    boxes.push(Rect {
                        x: p.x,
                        y: p.y,
                        size: Size {
                            width: 1.,
                            height: 1.,
                        },
                    });
                }
            }
            boxes
        });
        if bounds.is_empty() {
            self.pan.set(Point { x: 0., y: 0. });
            self.zoom.set(1.);
            return;
        }
        let x = bounds.iter().map(|b| b.x).fold(f64::INFINITY, f64::min);
        let y = bounds.iter().map(|b| b.y).fold(f64::INFINITY, f64::min);
        let right = bounds
            .iter()
            .map(|b| b.x + b.size.width)
            .fold(f64::NEG_INFINITY, f64::max);
        let bottom = bounds
            .iter()
            .map(|b| b.y + b.size.height)
            .fold(f64::NEG_INFINITY, f64::max);
        let r = document()
            .get_element_by_id("drawing-viewport")
            .unwrap()
            .get_bounding_client_rect();
        let zoom = ((r.width() - 300.).max(100.) / (right - x).max(1.))
            .min((r.height() - 150.).max(100.) / (bottom - y).max(1.))
            .clamp(0.1, 1.5);
        self.zoom.set(zoom);
        self.pan.set(Point {
            x: (r.width() + 200.) / 2. - (x + right) / 2. * zoom,
            y: r.height() / 2. - (y + bottom) / 2. * zoom,
        });
    }
    fn layer(self, front: bool) {
        if let Some(Selection::Shapes(ids)) = self.selected.get_untracked() {
            self.e.snapshot();
            self.e.change(|n| {
                let (mut chosen, mut others): (Vec<_>, Vec<_>) =
                    n.shapes.drain(..).partition(|s| ids.contains(&s.id));
                if front {
                    others.append(&mut chosen);
                    n.shapes = others;
                } else {
                    chosen.append(&mut others);
                    n.shapes = chosen;
                }
            });
        }
    }
}

impl Board {
    fn erase_at(self, x: f32, y: f32) {
        if let Some(target) = document().element_from_point(x, y).and_then(|e| {
            e.closest("[data-shape-id],[data-ink-id],[data-card-id],[data-stroke]")
                .ok()
                .flatten()
        }) {
            let shape = target.get_attribute("data-shape-id");
            let ink = target.get_attribute("data-ink-id");
            let card = target.get_attribute("data-card-id");
            let old = target
                .get_attribute("data-stroke")
                .and_then(|s| s.parse::<usize>().ok());
            self.e.change(|n| {
                if let Some(id) = shape {
                    n.shapes.retain(|s| s.id != id);
                } else if let Some(id) = ink {
                    n.ink.retain(|s| s.id != id);
                } else if let Some(id) = card {
                    n.remove_card(&id);
                } else if let Some(i) = old
                    && i < n.strokes.len()
                {
                    n.strokes.remove(i);
                }
            });
            self.selected.set(None);
        }
    }
}

#[component]
fn DrawingProperties(b: Board) -> impl IntoView {
    view! {<aside class="drawing-properties" aria-label="Drawing properties">
        <section class="property-section"><h3>"Stroke"</h3><div class="color-options">{["#1e1e1e","#e03131","#2f9e44","#1971c2","#6741d9"].into_iter().map(|color|view!{<button class="swatch" aria-label=format!("Stroke {color}") aria-pressed=move ||b.style.with(|s|s.stroke==color) style=format!("background-color:{color}") on:click=move |_|b.change_style(|s|s.stroke=color.into())/>}).collect_view()}<input type="color" aria-label="Custom stroke color" prop:value=move ||b.style.with(|s|s.stroke.clone()) on:input=move |ev|b.change_style(|s|s.stroke=event_target_value(&ev))/></div></section>
        <section class="property-section"><h3>"Background"</h3><div class="color-options">{["transparent","#ffc9c9","#b2f2bb","#a5d8ff","#ffec99"].into_iter().map(|color|view!{<button class="swatch" aria-label=format!("Fill {color}") aria-pressed=move ||b.style.with(|s|s.fill==color) style=format!("background-color:{color}") on:click=move |_|b.change_style(|s|s.fill=color.into())>{if color=="transparent"{"∅"}else{""}}</button>}).collect_view()}<input type="color" aria-label="Custom fill color" prop:value=move ||b.style.with(|s|if s.fill=="transparent"{"#ffffff".into()}else{s.fill.clone()}) on:input=move |ev|b.change_style(|s|s.fill=event_target_value(&ev))/></div></section>
        <section class="property-section"><h3>"Stroke width"</h3><div class="property-options">{[(1.,"Thin"),(2.,"Regular"),(4.,"Bold")].into_iter().map(|(width,label)|view!{<button aria-label=format!("Stroke width {width}") aria-pressed=move ||b.style.with(|s|s.stroke_width==width) on:click=move |_|b.change_style(|s|s.stroke_width=width)>{label}</button>}).collect_view()}</div></section>
        <section class="property-section"><h3>"Stroke style"</h3><div class="property-options"><button aria-pressed=move ||!b.style.with(|s|s.dashed) on:click=move |_|b.change_style(|s|s.dashed=false)>"Solid"</button><button aria-pressed=move ||b.style.with(|s|s.dashed) on:click=move |_|b.change_style(|s|s.dashed=true)>"Dashed"</button></div></section>
        <section class="property-section"><h3>"Sloppiness"</h3><div class="property-options"><button aria-pressed=move ||!b.style.with(|s|s.rough) on:click=move |_|b.change_style(|s|s.rough=false)>"Clean"</button><button aria-pressed=move ||b.style.with(|s|s.rough) on:click=move |_|b.change_style(|s|s.rough=true)>"Sketch"</button></div></section>
        <section class="property-section"><h3>"Font size"</h3><div class="property-options">{[(16.,"S"),(24.,"M"),(36.,"L"),(48.,"XL")].into_iter().map(|(size,label)|view!{<button aria-label=format!("Font size {size}") aria-pressed=move ||b.style.with(|s|s.font_size==size) on:click=move |_|b.change_style(|s|s.font_size=size)>{label}</button>}).collect_view()}</div></section>
        <section class="property-section"><label for="drawing-opacity">"Opacity "<span>{move ||b.style.with(|s|format!("{}%",s.opacity))}</span></label><input id="drawing-opacity" aria-label="Opacity" type="range" min="0" max="100" prop:value=move ||b.style.with(|s|s.opacity.to_string()) on:input=move |ev|{if let Ok(value)=event_target_value(&ev).parse::<u8>(){b.change_style(|s|s.opacity=value.min(100));}}/></section>
        <Show when=move ||matches!(b.selected.get(),Some(Selection::Shapes(_)))><section class="property-section"><h3>"Layers"</h3><div class="property-options"><button on:click=move |_|b.layer(false)>"To back"</button><button on:click=move |_|b.layer(true)>"To front"</button></div></section><div class="property-options"><button on:click=move |_|b.duplicate()>"Duplicate"</button><button on:click=move |_|b.remove()>"Delete"</button></div></Show>
    </aside>}
}
fn line_head(shape: &Shape) -> String {
    let end = Point {
        x: shape.x + shape.width,
        y: shape.y + shape.height,
    };
    let a = shape.height.atan2(shape.width);
    let size = 12. + shape.style.stroke_width * 2.;
    format!(
        "M{} {}L{} {}L{} {}",
        end.x - size * (a - 0.5).cos(),
        end.y - size * (a - 0.5).sin(),
        end.x,
        end.y,
        end.x - size * (a + 0.5).cos(),
        end.y - size * (a + 0.5).sin()
    )
}
#[component]
fn VectorShape(b: Board, id: String) -> impl IntoView {
    let id = StoredValue::new(id);
    let shape = Memo::new(move |_| {
        b.e.workspace.with(|w| {
            w.notes[b.e.active.get()]
                .shapes
                .iter()
                .find(|s| s.id == id.get_value())
                .cloned()
        })
    });
    view! {<g class="vector-shape" data-shape-id=move ||id.get_value() class:selected=move ||b.shape_selected(&id.get_value()) tabindex="0" role="img" aria-label=move ||shape.with(|s|s.as_ref().map(|s|format!("{:?}",s.kind)).unwrap_or_default()) opacity=move ||shape.with(|s|s.as_ref().map(|s|s.style.opacity as f64/100.).unwrap_or(1.)) on:focus=move |_|b.select(Selection::Shapes(vec![id.get_value()])) on:pointerdown=move |ev|b.shape_down(id.get_value(),ev) on:dblclick=move |ev|{ev.prevent_default();ev.stop_propagation();b.edit_text(id.get_value());}>
        {move ||shape.get().map(|s|{
            let bounds=s.bounds();let is_text=s.kind==ShapeKind::Text;let fill=if matches!(s.kind,ShapeKind::Line|ShapeKind::Arrow){"none".into()}else{s.style.fill.clone()};let dash=s.style.dashed.then_some("8 8");
            view!{
                {if is_text{let lines=s.text.lines().map(str::to_string).collect::<Vec<_>>();view!{<text class="drawing-text" visibility=move ||if b.text_edit.get().as_deref()==Some(id.get_value().as_str()){"hidden"}else{"visible"} x=s.x y=s.y+s.style.font_size fill=s.style.stroke.clone() style=format!("font-family:var(--font-drawing);font-size:{}px;pointer-events:none",s.style.font_size)>{lines.into_iter().enumerate().map(|(i,line)|view!{<tspan x=s.x dy=if i==0{0.}else{s.style.font_size*1.3}>{line}</tspan>}).collect_view()}</text>}.into_any()}else{view!{<path class="shape-line" d=s.path() fill=fill stroke=s.style.stroke.clone() stroke-width=s.style.stroke_width stroke-dasharray=dash stroke-linecap="round" stroke-linejoin="round" style="pointer-events:none"/>}.into_any()}}
                {(s.kind==ShapeKind::Arrow).then(||view!{<path d=line_head(&s) fill="none" stroke=s.style.stroke.clone() stroke-width=s.style.stroke_width stroke-linecap="round" stroke-linejoin="round" style="pointer-events:none"/>})}
                {if is_text{view!{<rect class="shape-hit" x=bounds.x y=bounds.y width=bounds.size.width height=bounds.size.height fill="transparent" stroke="none" style="pointer-events:all;cursor:text"/>}.into_any()}else{view!{<path class="shape-hit" d=s.path() fill=if matches!(s.kind,ShapeKind::Line|ShapeKind::Arrow){"none"}else{"transparent"} stroke="transparent" stroke-width=16./b.zoom.get() style="pointer-events:all"/>}.into_any()}}
            }
        })}
    </g>
    <Show when=move ||b.selected.with(|selection|matches!(selection,Some(Selection::Shapes(ids)) if ids.len()==1&&ids[0]==id.get_value()))&&b.tool.get()=="select"&&b.text_edit.get().is_none()>
        {move ||shape.get().map(|s|{let bounds=s.bounds();let pad=5./b.zoom.get();let handle=9./b.zoom.get();view!{<g class="shape-selection" style="pointer-events:none"><rect x=bounds.x-pad y=bounds.y-pad width=bounds.size.width+2.*pad height=bounds.size.height+2.*pad fill="none" stroke="var(--color-accent)" stroke-width=1./b.zoom.get()/><rect class="shape-handle" role="button" tabindex="0" aria-label="Resize shape" x=bounds.x+bounds.size.width-handle/2. y=bounds.y+bounds.size.height-handle/2. width=handle height=handle fill="var(--color-surface)" stroke="var(--color-accent)" stroke-width=1./b.zoom.get() style="pointer-events:all;cursor:nwse-resize" on:pointerdown=move |ev|b.vector_resize(id.get_value(),ev)/></g>}})}
    </Show>}
}
fn ink_path(points: &[Point]) -> String {
    let Some(first) = points.first() else {
        return String::new();
    };
    let mut d = format!("M{} {}", first.x, first.y);
    if points.len() == 1 {
        d.push_str("l0.01 0");
    } else {
        for pair in points.windows(2) {
            let a = &pair[0];
            let p = &pair[1];
            d.push_str(&format!(
                "Q{} {} {} {}",
                a.x,
                a.y,
                (a.x + p.x) / 2.,
                (a.y + p.y) / 2.
            ));
        }
        let p = points.last().unwrap();
        d.push_str(&format!("L{} {}", p.x, p.y));
    }
    d
}
#[component]
fn StyledInk(b: Board, id: String) -> impl IntoView {
    let id = StoredValue::new(id);
    let stroke = Memo::new(move |_| {
        b.e.workspace.with(|w| {
            w.notes[b.e.active.get()]
                .ink
                .iter()
                .find(|s| s.id == id.get_value())
                .cloned()
        })
    });
    view! {<g class="styled-ink" data-ink-id=move ||id.get_value() tabindex="0" role="img" aria-label="Freehand drawing" on:pointerdown=move |ev|{if b.tool.get_untracked()=="select"&&!b.space.get_untracked(){ev.prevent_default();ev.stop_propagation();b.select(Selection::Ink(id.get_value()));if let Some(s)=stroke.get_untracked(){b.style.set(s.style);b.gesture_saved.set(false);b.gesture.set(Some(Gesture::InkMove{pointer:ev.pointer_id(),id:id.get_value(),start:point(&ev,b.zoom.get_untracked()),points:s.points}));capture(&ev);}b.focus();}}>
        {move ||stroke.get().map(|s|{let path=ink_path(&s.points);view!{<path class="ink-line" d=path.clone() fill="none" stroke=s.style.stroke stroke-width=s.style.stroke_width opacity=s.style.opacity as f64/100. stroke-linecap="round" stroke-linejoin="round" stroke-dasharray=s.style.dashed.then_some("8 8") style="pointer-events:none"/><path d=path fill="none" stroke="transparent" stroke-width=16./b.zoom.get() stroke-linecap="round" style="pointer-events:stroke"/>}})}
    </g>}
}
#[component]
fn DrawingTextEditor(b: Board) -> impl IntoView {
    let id = b.text_edit.get_untracked().unwrap_or_default();
    let id = StoredValue::new(id);
    let node = NodeRef::<leptos::html::Textarea>::new();
    Effect::new(move |_| {
        if let Some(el) = node.get() {
            el.focus().ok();
            let length = el.value().encode_utf16().count() as u32;
            el.set_selection_range(length, length).ok();
        }
    });
    view! {<textarea node_ref=node class="drawing-text-editor" aria-label="Canvas text" placeholder="Type here" maxlength="100000" spellcheck="false" style=move ||b.e.workspace.with(|w|w.notes[b.e.active.get()].shapes.iter().find(|s|s.id==id.get_value()).map(|s|format!("left:{}px;top:{}px;width:{}px;height:{}px;font-family:var(--font-drawing);font-size:{}px;color:{};line-height:1.3;background:transparent;resize:none",s.x,s.y,s.width.max(240.)+20.,s.height.max(48.)+24.,s.style.font_size,s.style.stroke)).unwrap_or_default()) prop:value=move ||b.e.workspace.with(|w|w.notes[b.e.active.get()].shapes.iter().find(|s|s.id==id.get_value()).map(|s|s.text.clone()).unwrap_or_default()) on:pointerdown=move |ev|ev.stop_propagation() on:keydown=move |ev|{if ev.key()=="Escape"||((ev.ctrl_key()||ev.meta_key())&&ev.key()=="Enter"){ev.prevent_default();ev.stop_propagation();b.text_edit.set(None);b.focus();}} on:input=move |ev|{let text=event_target_value(&ev);if text.len()>100000{return;}b.e.change(|n|if let Some(s)=n.shapes.iter_mut().find(|s|s.id==id.get_value()){s.width=(text.lines().map(|l|l.chars().count()).max().unwrap_or(1) as f64*s.style.font_size*0.68).clamp(32.,10000.);s.height=(text.lines().count().max(1) as f64*s.style.font_size*1.3).max(s.style.font_size*1.3).min(10000.);s.text=text;});} on:blur=move |_|b.text_edit.set(None)/>}
}
