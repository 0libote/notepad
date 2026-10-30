use crate::{Note, Workspace};
use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use pulldown_cmark::{Event, LinkType, Options, Parser, Tag, TagEnd, html};

pub const MAX_MARKDOWN_BYTES: usize = 1_000_000;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteLink {
    pub id: String,
    pub title: String,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Resolution<'a> {
    Found(&'a NoteLink),
    Missing,
    Ambiguous,
}
pub fn catalog(notes: &[Note]) -> Vec<NoteLink> {
    notes
        .iter()
        .map(|n| NoteLink {
            id: n.id.clone(),
            title: n.title.clone(),
        })
        .collect()
}
pub fn resolve_title<'a>(notes: &'a [NoteLink], title: &str) -> Resolution<'a> {
    let title = title.trim().to_lowercase();
    let mut matches = notes
        .iter()
        .filter(|n| n.title.trim().to_lowercase() == title);
    match (matches.next(), matches.next()) {
        (Some(n), None) => Resolution::Found(n),
        (None, _) => Resolution::Missing,
        _ => Resolution::Ambiguous,
    }
}
fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_WIKILINKS
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
pub fn encode_id(id: &str) -> String {
    utf8_percent_encode(id, NON_ALPHANUMERIC).to_string()
}
pub fn decode_id(value: &str) -> Option<String> {
    percent_decode_str(value)
        .decode_utf8()
        .ok()
        .map(|s| s.into_owned())
}
pub fn note_href(id: &str, view: &str) -> String {
    format!(
        "#note={}&view={}",
        encode_id(id),
        if ["read", "write", "canvas"].contains(&view) {
            view
        } else {
            "read"
        }
    )
}
pub fn markdown_link(note: &NoteLink) -> String {
    let label = note
        .title
        .chars()
        .flat_map(|c| {
            if c == '\n' || c == '\r' {
                vec![' ']
            } else if c.is_ascii_punctuation() {
                vec!['\\', c]
            } else {
                vec![c]
            }
        })
        .collect::<String>();
    format!("[{label}](notepad:{})", encode_id(&note.id))
}
fn stable_target(destination: &str) -> Option<String> {
    decode_id(destination.strip_prefix("notepad:")?)
}
fn target<'a>(destination: &str, wiki: bool, notes: &'a [NoteLink]) -> Resolution<'a> {
    if wiki {
        resolve_title(notes, destination)
    } else if let Some(id) = stable_target(destination) {
        notes
            .iter()
            .find(|n| n.id == id)
            .map(Resolution::Found)
            .unwrap_or(Resolution::Missing)
    } else {
        Resolution::Missing
    }
}
/// Only generated markup reaches the DOM. Raw HTML and unsupported URLs become text.
pub fn render_markdown(source: &str, notes: &[NoteLink]) -> String {
    if source.len() > MAX_MARKDOWN_BYTES {
        return "<p>This note is too large to preview. Export or shorten it in Write.</p>".into();
    }
    let mut closers = Vec::new();
    let events=Parser::new_ext(source,options()).map(|event|match event{
        Event::Html(text)|Event::InlineHtml(text)=>Event::Text(text),
        Event::Start(Tag::Link{link_type,dest_url,..})=>{
            let wiki=matches!(link_type,LinkType::WikiLink{..});
            let (opening,closing)=if wiki||dest_url.starts_with("notepad:"){
                match target(&dest_url,wiki,notes){
                    Resolution::Found(note)=>(format!("<a data-note-id=\"{}\" href=\"{}\">",escape(&note.id),escape(&note_href(&note.id,"read"))),"</a>"),
                    Resolution::Missing if wiki && !dest_url.trim().is_empty() && dest_url.len()<=512 =>(format!("<button class=\"missing-note\" type=\"button\" data-create-note=\"{}\" title=\"Create this note\">",escape(dest_url.trim())),"</button>"),
                    Resolution::Ambiguous=>("<span class=\"unresolved-note\" title=\"Several notes share this title. Use a specific note link.\">".into(),"</span>"),
                    _=>("<span class=\"unresolved-note\" title=\"Note not found\">".into(),"</span>"),
                }
            }else if crate::safe_link(&dest_url){(format!("<a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">",escape(&dest_url)),"</a>")}
            else{("<span>".into(),"</span>")};
            closers.push(closing);Event::Html(opening.into())
        },
        Event::End(TagEnd::Link)=>Event::Html(closers.pop().unwrap_or("").into()),
        // Images stay inert, including data URLs and Obsidian-style embeds.
        Event::Start(Tag::Image{dest_url,..})=>{
            let (opening,closing)=if crate::safe_link(&dest_url){(format!("<a class=\"markdown-image\" href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">Image: ",escape(&dest_url)),"</a>")}
            else{("<span class=\"markdown-image\">Image: ".into(),"</span>")};
            closers.push(closing);Event::Html(opening.into())
        },
        Event::End(TagEnd::Image)=>Event::Html(closers.pop().unwrap_or("").into()),
        other=>other,
    });
    let mut output = String::new();
    html::push_html(&mut output, events);
    output
}
pub fn referenced_notes(source: &str, notes: &[NoteLink]) -> Vec<String> {
    if source.len() > MAX_MARKDOWN_BYTES {
        return vec![];
    }
    let mut ids = vec![];
    for event in Parser::new_ext(source, options()) {
        if let Event::Start(Tag::Link {
            link_type,
            dest_url,
            ..
        }) = event
            && let Resolution::Found(note) = target(
                &dest_url,
                matches!(link_type, LinkType::WikiLink { .. }),
                notes,
            )
            && !ids.contains(&note.id)
        {
            ids.push(note.id.clone());
        }
    }
    ids
}
pub fn backlinks(notes: &[Note], target: &str) -> Vec<NoteLink> {
    let index = catalog(notes);
    notes
        .iter()
        .filter(|n| {
            n.id != target
                && (n.cards.iter().any(|c| c.kind == "note" && c.text == target)
                    || referenced_notes(&n.body, &index)
                        .iter()
                        .any(|id| id == target))
        })
        .map(|n| NoteLink {
            id: n.id.clone(),
            title: n.title.clone(),
        })
        .collect()
}
impl Workspace {
    /// Convert resolved title links to stable ID links before a title changes.
    pub fn rename_note(&mut self, id: &str, title: String) -> Result<(), &'static str> {
        if title.len() > 512 {
            return Err("Note title is too long");
        }
        let index = catalog(&self.notes);
        let Some(original) = index.iter().find(|n| n.id == id) else {
            return Err("Note not found");
        };
        if original.title == title {
            return Ok(());
        }
        let mut replacements = Vec::new();
        for note in &self.notes {
            let mut changes = vec![];
            for (event, range) in Parser::new_ext(&note.body, options()).into_offset_iter() {
                if let Event::Start(Tag::Link {
                    link_type: LinkType::WikiLink { .. },
                    dest_url,
                    ..
                }) = event
                    && let Resolution::Found(found) = resolve_title(&index, &dest_url)
                    && found.id == id
                {
                    let raw = &note.body[range.clone()];
                    if let Some(inside) = raw.strip_prefix("[[").and_then(|v| v.strip_suffix("]]"))
                    {
                        let mut escaped = false;
                        let pipe = inside.char_indices().find_map(|(i, c)| {
                            if escaped {
                                escaped = false;
                                None
                            } else if c == '\\' {
                                escaped = true;
                                None
                            } else if c == '|' {
                                Some(i)
                            } else {
                                None
                            }
                        });
                        let label = pipe
                            .map(|i| inside[i + 1..].to_string())
                            .filter(|s| !s.trim().is_empty());
                        let link = if let Some(label) = label {
                            format!("[{label}](notepad:{})", encode_id(id))
                        } else {
                            markdown_link(found)
                        };
                        changes.push((range, link));
                    }
                }
            }
            let mut body = note.body.clone();
            for (range, link) in changes.into_iter().rev() {
                body.replace_range(range, &link);
            }
            if body.len() > MAX_MARKDOWN_BYTES {
                return Err("Renaming would exceed a note's size limit");
            }
            replacements.push(body);
        }
        for (note, body) in self.notes.iter_mut().zip(replacements) {
            note.body = body;
            if note.id == id {
                note.title = title.clone();
            }
        }
        Ok(())
    }
}
pub fn import_markdown(filename: &str, bytes: &[u8]) -> Result<(String, String), &'static str> {
    if bytes.len() > MAX_MARKDOWN_BYTES {
        return Err("Markdown file is too large. Choose one under 1 MB.");
    }
    let body = std::str::from_utf8(bytes)
        .map_err(|_| "Choose a UTF-8 Markdown file.")?
        .trim_start_matches('\u{feff}')
        .to_string();
    let mut heading = String::new();
    let mut in_heading = false;
    for event in Parser::new(&body) {
        match event {
            Event::Start(Tag::Heading {
                level: pulldown_cmark::HeadingLevel::H1,
                ..
            }) => in_heading = true,
            Event::End(TagEnd::Heading(_)) if in_heading => break,
            Event::Text(t) | Event::Code(t) if in_heading => heading.push_str(&t),
            _ => {}
        }
    }
    let fallback = filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("Imported note");
    let fallback = fallback
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(fallback);
    let title = if heading.trim().is_empty() {
        fallback.trim()
    } else {
        heading.trim()
    };
    let mut title = title.chars().take(160).collect::<String>();
    while title.len() > 512 {
        title.pop();
    }
    if title.is_empty() {
        title = "Imported note".into();
    }
    Ok((title, body))
}
pub fn markdown_filename(title: &str) -> String {
    let stem: String = title
        .chars()
        .filter(|c| !c.is_control() && !"<>:\"/\\|?*".contains(*c))
        .take(80)
        .collect();
    let mut stem = stem;
    while stem.len() > 200 {
        stem.pop();
    }
    let stem = stem.trim().trim_end_matches('.');
    let upper = stem.split('.').next().unwrap_or("").to_uppercase();
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ]
    .contains(&upper.as_str());
    format!(
        "{}.md",
        if stem.is_empty() || reserved {
            "note"
        } else {
            stem
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn notes() -> Vec<NoteLink> {
        vec![
            NoteLink {
                id: "a & /\"".into(),
                title: "Alpha".into(),
            },
            NoteLink {
                id: "b".into(),
                title: "Beta".into(),
            },
        ]
    }
    #[test]
    fn renders_commonmark_without_active_html_or_images() {
        let html = render_markdown(
            "# Heading\n\n**Bold** and `code`.\n\n- [x] Done\n\n<script>alert(1)</script>\n\n[bad](javascript:alert(1)) ![pixel](https://tracking.invalid/x)\n\n| A | B |\n|---|---|\n| 1 | 2 |",
            &notes(),
        );
        for part in [
            "<h1>",
            "<strong>",
            "<code>",
            "checked=",
            "<table>",
            "&lt;script&gt;",
        ] {
            assert!(html.contains(part), "{part}: {html}");
        }
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<img"));
        assert!(!html.contains("href=\"javascript:"));
    }
    #[test]
    fn resolves_aliases_stable_ids_missing_and_ambiguous_titles() {
        let index = notes();
        let source = format!(
            "[[alpha|Read this]] {} [[New note]] `[[Beta]]`\n\n```\n[[Beta]]\n```",
            markdown_link(&index[0])
        );
        let html = render_markdown(&source, &index);
        assert!(html.contains("data-note-id=\"a &amp; /&quot;\""));
        assert!(html.contains("data-create-note=\"New note\""));
        assert_eq!(referenced_notes(&source, &index), vec![index[0].id.clone()]);
        let mut index = index;
        index.push(NoteLink {
            id: "c".into(),
            title: "ALPHA".into(),
        });
        assert_eq!(resolve_title(&index, "alpha"), Resolution::Ambiguous);
        assert!(!render_markdown("[[Alpha]]", &index).contains("data-create-note"));
    }
    #[test]
    fn hostile_titles_are_escaped_and_ids_round_trip() {
        let index = vec![NoteLink {
            id: "a\" onload=\"bad".into(),
            title: "<img src=x onerror=bad>".into(),
        }];
        let html = render_markdown(&markdown_link(&index[0]), &index);
        assert!(!html.contains("<img"));
        assert!(html.contains("&lt;img"));
        assert_eq!(
            decode_id(&encode_id(&index[0].id)),
            Some(index[0].id.clone())
        );
    }
    #[test]
    fn patched_parser_does_not_expand_empty_wikilinks() {
        let source = "[[a|]] ".repeat(64);
        assert!(Parser::new_ext(&source, options()).count() < 1000);
        assert!(render_markdown(&source, &notes()).len() < 20000);
    }
    #[test]
    fn rename_keeps_links_and_aliases_without_touching_code() {
        let mut w = Workspace::default();
        let id = w.notes[0].id.clone();
        let old = w.notes[0].title.clone();
        w.notes[0].body = format!("[[{old}]] [[{old}|**alias**]] `[[{old}]]`");
        w.rename_note(&id, "Renamed".into()).unwrap();
        assert_eq!(
            referenced_notes(&w.notes[0].body, &catalog(&w.notes)),
            vec![id]
        );
        assert!(w.notes[0].body.contains("[**alias**](notepad:"));
        assert!(w.notes[0].body.contains(&format!("`[[{old}]]`")));
    }
    #[test]
    fn imports_utf8_and_exports_portable_filenames() {
        assert_eq!(
            import_markdown("file.md", b"# My **note**\n\nBody")
                .unwrap()
                .0,
            "My note"
        );
        assert!(import_markdown("bad.md", &[255]).is_err());
        assert_eq!(markdown_filename("CON"), "note.md");
        assert_eq!(markdown_filename("Plan / draft?"), "Plan  draft.md");
    }
}

/// DOM text selections use UTF-16 offsets; convert them without splitting UTF-8.
pub fn insert_link_at(
    body: &mut String,
    link: &str,
    start: u32,
    end: u32,
) -> Result<(), &'static str> {
    fn offset(text: &str, position: u32) -> usize {
        let mut units = 0;
        for (index, ch) in text.char_indices() {
            if units >= position {
                return index;
            }
            units += ch.len_utf16() as u32;
        }
        text.len()
    }
    let start = offset(body, start);
    let end = offset(body, end).max(start);
    if body.len() - (end - start) + link.len() > MAX_MARKDOWN_BYTES {
        return Err("The link would exceed the note's size limit");
    }
    body.replace_range(start..end, link);
    Ok(())
}
#[cfg(test)]
mod selection_tests {
    use super::*;
    #[test]
    fn inserts_at_unicode_selection_without_splitting_characters() {
        let mut body = "A😀B".into();
        insert_link_at(&mut body, "[[Note]]", 1, 3).unwrap();
        assert_eq!(body, "A[[Note]]B");
        let mut body = "é".into();
        insert_link_at(&mut body, "!", 99, 99).unwrap();
        assert_eq!(body, "é!");
    }
}

pub fn search_notes(notes: &[Note], query: &str) -> Vec<NoteLink> {
    let query = query.trim().to_lowercase();
    let contains = |text: &str| text.to_lowercase().contains(&query);
    notes
        .iter()
        .filter(|n| {
            query.is_empty()
                || contains(&n.title)
                || contains(&n.body)
                || n.cards.iter().any(|c| match c.kind.as_str() {
                    "image" => contains("image"),
                    "note" => notes
                        .iter()
                        .find(|n| n.id == c.text)
                        .is_some_and(|n| contains(&n.title)),
                    _ => {
                        contains(&c.text)
                            || c.email.as_ref().is_some_and(|m| {
                                contains(&m.subject)
                                    || contains(&m.sender)
                                    || contains(&m.recipient)
                                    || contains(&m.date)
                            })
                    }
                })
        })
        .map(|n| NoteLink {
            id: n.id.clone(),
            title: n.title.clone(),
        })
        .collect()
}
#[cfg(test)]
mod reference_tests {
    use super::*;
    use crate::Card;
    #[test]
    fn backlinks_follow_body_links_and_canvas_refs_but_ignore_self_and_code() {
        let mut w = Workspace::default();
        let target = w.notes[0].id.clone();
        w.notes.push(Note {
            id: "source".into(),
            title: "Source".into(),
            body: markdown_link(&catalog(&w.notes)[0]),
            cards: vec![],
            strokes: vec![],
            connections: vec![],
        });
        w.notes.push(Note {
            id: "canvas".into(),
            title: "Canvas".into(),
            body: String::new(),
            cards: vec![Card {
                id: "ref".into(),
                kind: "note".into(),
                text: target.clone(),
                email: None,
                size: None,
                x: 0.,
                y: 0.,
            }],
            strokes: vec![],
            connections: vec![],
        });
        let mut code = w.notes[1].clone();
        code.id = "code".into();
        code.body = format!("`[[{}]]`", w.notes[0].title);
        w.notes.push(code);
        assert_eq!(
            backlinks(&w.notes, &target)
                .iter()
                .map(|n| n.id.as_str())
                .collect::<Vec<_>>(),
            ["source", "canvas"]
        );
        assert!(w.validate().is_ok());
        assert_eq!(search_notes(&w.notes, "  source  ")[0].id, "source");
    }
}
