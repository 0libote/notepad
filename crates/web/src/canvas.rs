use crate::{Editor, Icon, point, read_capture, uid};
use leptos::prelude::*;
use notepad_core::{
    Card, Workspace,
    canvas::{self, Connection, Rect, Size},
    markdown::{self, NoteLink},
};
use std::collections::HashMap;
use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{KeyboardEvent, PointerEvent};

#[derive(Clone, PartialEq)]
enum Selection {
    Card(String),
    Connection(String),
    Stroke(usize),
}
#[derive(Clone)]
enum Gesture {
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
            Self::Move { pointer, .. } | Self::Resize { pointer, .. } | Self::Draw { pointer } => {
                *pointer
            }
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
        match ev.key().as_str() {
            "v" | "V" => {
                self.tool.set("select");
                self.from.set(None);
            }
            "p" | "P" => {
                self.tool.set("pen");
                self.from.set(None);
            }
            "c" | "C" => {
                self.tool.set("connect");
                self.from.set(None);
            }
            _ => {}
        }
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
    };
    let link_open = RwSignal::new(false);
    let link = RwSignal::new(String::new());
    Effect::new(move |_| {
        let _ = e.active.get();
        let _ = e.workspace_epoch.get();
        let _ = e.ready.get();
        b.gesture.set(None);
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
        <div class="canvas-wrap" on:keydown=move |ev|b.key(ev)>
            <div class="canvas-intro"><span>"Think outside the page."</span><span>"Drop a link, an image, an email."</span></div>
            <div class="toolbar" role="toolbar" aria-label="Canvas tools">
                <button class:chosen=move ||b.tool.get()=="select" aria-label="Select and move" title="Select and move (V)" on:click=move |_|{b.tool.set("select");b.from.set(None);} ><Icon name="cursor"/></button>
                <button class:chosen=move ||b.tool.get()=="pen" aria-label="Draw" title="Draw (P)" on:click=move |_|{b.tool.set("pen");b.from.set(None);b.selected.set(None);} ><Icon name="pen"/></button>
                <button class:chosen=move ||b.tool.get()=="connect" aria-label="Connect cards" title="Connect cards (C)" on:click=move |_|{b.tool.set("connect");b.from.set(None);} ><Icon name="connect"/></button><span class="tool-divider"></span>
                <button aria-label="Add note to canvas" title="Add note to canvas" on:click=move |_|{picked.set(String::new());picker.set(Some("canvas"));}><Icon name="note"/></button>
                <button aria-label="Add text card" title="Add text card" on:click=move |_|{e.add("text","A new thought…".into(),250.,180.);b.tool.set("select");}><Icon name="text"/></button>
                <button aria-label="Add sticky note" title="Add sticky note" on:click=move |_|{e.add("sticky","Something worth keeping…".into(),530.,220.);b.tool.set("select");}><Icon name="sticky"/></button>
                <button aria-label="Add website" title="Add website" on:click=move |_|link_open.update(|v|*v = !*v)><Icon name="link"/></button>
                <label class="image-picker" title="Add email"><Icon name="mail"/><input type="file" accept=".eml,message/rfc822" aria-label="Add email" on:change=move |ev|{let input:web_sys::HtmlInputElement=event_target(&ev);if let Some(f)=input.files().and_then(|fs|fs.get(0)){read_capture(f,e,300.,220.);}input.set_value("");}/></label>
                <label class="image-picker" title="Add image"><Icon name="image"/><input type="file" accept="image/png,image/jpeg,image/webp,image/gif" aria-label="Add image" on:change=move |ev|{let input:web_sys::HtmlInputElement=event_target(&ev);if let Some(f)=input.files().and_then(|fs|fs.get(0)){read_capture(f,e,320.,260.);}input.set_value("");}/></label>
                <button aria-label="Undo" title="Undo (Ctrl/Cmd+Z)" disabled=move ||e.history.with(|h|h.is_empty()) on:click=move |_|e.undo()><Icon name="undo"/></button>
            </div>
            <Show when=move ||link_open.get()><form class="link-form" on:submit=move |ev|{ev.prevent_default();let url=link.get_untracked();if !notepad_core::safe_link(&url){e.status.set("Enter a full http:// or https:// website address.".into());return;}e.add("link",url,420.,380.);link.set(String::new());link_open.set(false);} ><label for="website-url">"Website address"</label><input id="website-url" type="url" required placeholder="https://…" prop:value=move ||link.get() on:input=move |ev|link.set(event_target_value(&ev))/><button class="primary" type="submit">"Add link"</button></form></Show>
            <div class="canvas-scroll" on:dragover=move |ev|ev.prevent_default() on:drop=move |ev|{
                ev.prevent_default();if !e.ready.get_untracked(){return;}
                let rect=document().get_element_by_id("world").unwrap().get_bounding_client_rect();let x=((ev.client_x() as f64-rect.x())/b.zoom.get_untracked()).clamp(0.,2200.);let y=((ev.client_y() as f64-rect.y())/b.zoom.get_untracked()).clamp(0.,1400.);
                if let Some(dt)=ev.data_transfer(){if let Some(files)=dt.files()&&files.length()>0{for i in 0..files.length().min(8){if let Some(f)=files.get(i){read_capture(f,e,x+i as f64*30.,y+i as f64*30.);}}return;}
                    let text=dt.get_data("text/uri-list").unwrap_or_default().lines().find(|s|!s.starts_with('#')).unwrap_or("").to_string();let text=if text.is_empty(){dt.get_data("text/plain").unwrap_or_default()}else{text};if !text.trim().is_empty(){e.add(if notepad_core::safe_link(text.trim()){"link"}else{"text"},text.trim().chars().take(100_000).collect(),x,y);}
                }
            }>
            <div class="world-size" style=move ||format!("width:{}px;height:{}px",canvas::WIDTH*b.zoom.get(),canvas::HEIGHT*b.zoom.get())>
                <div id="world" class="world" tabindex="0" aria-label="Whiteboard. Select a card or drawing; Delete removes it. V selects, P draws, C connects cards." class:pen-mode=move ||b.tool.get()=="pen" class:connect-mode=move ||b.tool.get()=="connect" style=move ||format!("transform:scale({})",b.zoom.get())
                    on:pointerdown=move |ev|{
                        if ev.button()!=0||b.gesture.with_untracked(|g|g.is_some())||!e.ready.get_untracked(){return;}
                        if b.tool.get_untracked()=="pen" {
                            if e.workspace.with_untracked(|w|w.notes[e.active.get_untracked()].strokes.len()>=10000){e.status.set("Drawing limit reached on this page.".into());return;}
                            ev.prevent_default();e.snapshot();b.selected.set(None);b.gesture.set(Some(Gesture::Draw{pointer:ev.pointer_id()}));let p=point(&ev,b.zoom.get_untracked());e.change(|n|n.strokes.push(vec![p]));capture(&ev);
                        }else if ev.target()==ev.current_target(){b.selected.set(None);b.from.set(None);b.focus();}
                    }
                    on:pointermove=move |ev|{
                        let Some(gesture)=b.gesture.get_untracked().filter(|g|g.pointer()==ev.pointer_id()) else{return;};let p=point(&ev,b.zoom.get_untracked());
                        e.change(|n|match gesture{
                            Gesture::Draw{..}=>{if let Some(s)=n.strokes.last_mut()&&s.len()<50000{s.push(p);}},
                            Gesture::Move{id,dx,dy,..}=>{if let Some(c)=n.cards.iter_mut().find(|c|c.id==id){c.move_to(p.x-dx,p.y-dy,b.size(c));}},
                            Gesture::Resize{id,x,y,size,..}=>{if let Some(c)=n.cards.iter_mut().find(|c|c.id==id){c.resize(size.width+p.x-x,size.height+p.y-y);}},
                        });
                    }
                    on:pointerup=move |ev|{if b.gesture.with_untracked(|g|g.as_ref().is_some_and(|g|g.pointer()==ev.pointer_id())){b.gesture.set(None);}}
                    on:pointercancel=move |ev|{if b.gesture.with_untracked(|g|g.as_ref().is_some_and(|g|g.pointer()==ev.pointer_id())){b.gesture.set(None);}} on:lostpointercapture=move |ev|{if b.gesture.with_untracked(|g|g.as_ref().is_some_and(|g|g.pointer()==ev.pointer_id())){b.gesture.set(None);}}>
                    <svg class="ink" width="2600" height="1800" aria-label="Your drawing">
                        <defs><marker id="canvas-arrow" markerWidth="8" markerHeight="8" refX="10" refY="5" orient="auto" viewBox="0 0 10 10"><path class="arrow-head" d="M0 0L10 5L0 10Z"/></marker></defs>
                        <For each=move ||e.workspace.with(|w|w.notes[e.active.get()].connections.iter().map(|c|c.id.clone()).collect::<Vec<_>>()) key=move |id|(e.workspace_epoch.get(),e.active.get(),id.clone()) children=move |id|view!{<ConnectionLine b=b id=id/>}/>
                        <For each=move ||e.workspace.with(|w|(0..w.notes[e.active.get()].strokes.len()).collect::<Vec<_>>()) key=move |i|(e.workspace_epoch.get(),e.active.get(),*i) children=move |i|view!{<DrawingStroke b=b index=i/>}/>

                    </svg>
                    <For each=move ||e.workspace.with(|w|w.notes[e.active.get()].cards.iter().map(|c|c.id.clone()).collect::<Vec<_>>()) key=move |id|(e.workspace_epoch.get(),e.workspace.with_untracked(|w|w.notes[e.active.get_untracked()].id.clone()),id.clone()) children=move |id|{
                        let card=e.workspace.with_untracked(|w|w.notes[e.active.get_untracked()].cards.iter().find(|c|c.id==id).unwrap().clone());
                        view!{<BoardCard b=b card=card observer=observer note_catalog=note_catalog/>}
                    }/>
                    {move ||e.workspace.with(|w|{let n=&w.notes[e.active.get()];(n.cards.is_empty()&&n.strokes.is_empty()).then(||view!{<div class="empty-canvas"><Icon name="canvas"/><h2>"Give your thoughts some room."</h2><p>"Add a note above, draw a line, or drop something here."</p></div>})})}
                </div>
            </div></div>
            <div class="canvas-bottom"><Show when=move ||b.selected.get().is_some()&&b.tool.get()=="select" fallback=move ||view!{<span class="canvas-hint" role="status">{move ||if b.tool.get()=="connect"{if b.from.get().is_some(){"Choose another card. Escape cancels."}else{"Choose a card to start a connection."}}else if b.tool.get()=="pen"{"Draw on the board. Undo reverses a stroke."}else{"Drag a card's header. Select a line to remove it."}}</span>}>
                <div class="selection-controls" role="group" aria-label="Selection"><span>{move ||match b.selected.get(){Some(Selection::Card(id))=>e.workspace.with(|w|w.notes[e.active.get()].cards.iter().find(|c|c.id==id).map(|c|{let s=b.size(c);format!("{} × {}",s.width.round() as i32,s.height.round() as i32)}).unwrap_or_default()),Some(Selection::Connection(_))=>"Connection".into(),Some(Selection::Stroke(_))=>"Drawing".into(),_=>String::new()}}</span><Show when=move ||matches!(b.selected.get(),Some(Selection::Card(_)))><button title="Duplicate card (Ctrl/Cmd+D)" on:click=move |_|b.duplicate()>"Duplicate"</button></Show><button title="Delete selection (Delete or Backspace)" on:click=move |_|b.remove()>"Delete"</button></div>
            </Show><div class="zoom"><button aria-label="Zoom out" on:click=move |_|b.zoom.update(|z|*z=(*z-0.1).max(0.4))>"−"</button><button aria-label="Reset zoom" on:click=move |_|b.zoom.set(1.)>{move ||format!("{}%",(b.zoom.get()*100.).round() as i32)}</button><button aria-label="Zoom in" on:click=move |_|b.zoom.update(|z|*z=(*z+0.1).min(1.5))>"+"</button></div></div>
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
