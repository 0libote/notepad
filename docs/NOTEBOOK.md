# Pages, links, and the canvas

Each note has one Markdown body and a canvas. **Write** edits the source. **Read** renders it. **Canvas** arranges captures and references around it.

## Reading and linking

The Rust renderer supports headings, emphasis, lists, code blocks, tables, block quotes and read-only task checkboxes. Raw HTML is displayed as text. Remote Markdown images are shown as links instead of loading automatically; use image cards for collected images.

Write `[[A note title]]` or `[[A note title|display text]]` to link to an existing note. Title lookup ignores surrounding whitespace and case. If no note matches, the rendered link creates one when clicked. Duplicate titles are marked unresolved instead of choosing the wrong note.

**Link a note** in Write opens a picker and inserts a stable ID link at the text selection. It preserves Unicode around the selection. Links inserted this way survive renames. Renaming a note also converts resolved title-based wikilinks pointing to it into stable ID links, preserving their display text and aliases. Code examples are left untouched. Ambiguous links are not rewritten.

The Read view's **Linked from** list includes other pages linking to the note and other canvases holding a reference to it. Self-links and code examples do not appear there. Search includes note titles, source text, text/sticky/link cards, email metadata/body, and the titles of referenced notes; it does not search image base64 data or perform OCR.

Note selection and the current view live in the URL fragment. Reload, browser Back/Forward, and opening a note link in a new tab preserve the destination. Missing destinations do not silently create a note with an arbitrary ID.

## A page on the whiteboard

**Pin page** in Write adds the current note to its own canvas. **Add note to canvas** in the toolbar lets you choose another page. The card is a reference, not a copy: title and source edits appear in its preview. Previews show up to 2,000 characters; Open note opens the full page. Remove the reference without deleting the note, and use Undo to restore it.

Note references participate in JSON backups and server snapshots. A reference to a missing note is retained so the rest of a workspace remains usable. Update the web UI and server together; older builds do not understand the new card type.

## Markdown files

**Import Markdown** creates a new page from a UTF-8 `.md`/`.markdown` file, up to 1 MB. The first level-one heading supplies its title; otherwise the filename does. Dropping a Markdown file on a canvas creates the page and a reference card at the drop location, leaving the current page open. Invalid UTF-8 is rejected without replacing existing notes.

**Export .md** downloads the current Markdown source using a filename safe for common desktop filesystems. It preserves source text rather than flattening the rendered view. Stable `notepad:` links need the original note IDs to resolve; a Markdown file alone does not recreate a whole workspace graph. Use the full JSON export/import to preserve IDs, drawings, collected images/emails, canvas positions and references.

## Implementation

Rendering and link discovery use [pulldown-cmark](https://docs.rs/pulldown-cmark/0.13.5/pulldown_cmark/), with a minimum version of 0.13.5. That minimum includes the wikilink event-growth fix documented by [RustSec](https://rustsec.org/advisories/RUSTSEC-2026-0414.html). A regression test bounds event count for repeated empty-pipe wikilinks. Enabled syntax is explicit; raw HTML, arbitrary URL schemes and automatic image fetching are not passed through to the DOM.

The format remains a version-one workspace container with an additive `note` card kind. Titles are editable labels; references use persistent note IDs. Bulk workspace replacement invalidates card views so an imported card cannot keep showing a previous image, URL, or reference merely because its ID was reused.

## Sync between devices

While signed in, edits save on this device first and sync after a short pause. An open client checks for remote changes about every 15 seconds. **Save to server** also checks immediately. Offline drafts remain editable; reconnecting resumes sync. The browser still needs the app assets to open, so this is not an installable offline PWA yet. A connected desktop client uses the same coordinator and its Rust HTTP bridge while the app is open; native execution remains unverified.

Each local draft stores the last known server snapshot in the same transaction. Sync compares that baseline, the device draft, and the latest server copy. Changes to different pages merge automatically. A page includes its text, cards and ink, so two edits anywhere on the same page pause sync rather than attempting a text or drawing merge. The conflict notice offers **Keep both versions**: the server version keeps its ID, and the device version becomes a page titled `… (device copy)`. Both appear in the note list and sync to other devices. Stable links continue to target the original ID. Review the copies and move the material you want to keep; deletion/cleanup controls are not implemented yet.

A draft created before sync existed has no baseline. If its server revision differs, differing pages require recovery even when one might have been unchanged. This conservative migration retains content. Recovery also retains a surviving page when a deletion cannot be inferred. If recovery would exceed the 1,000-page limit, sync pauses and asks for exports/manual reconciliation.

Server revisions still reject overlapping uploads. A rejected or interrupted request leaves the local draft intact and retries on a later check; **Save to server** retries immediately. Edits typed while an upload is in flight stay local and upload afterward. This uses whole-workspace snapshots, including inline images and email sources; it is intended for small personal notebooks. It does not provide simultaneous collaborative text editing, sync while the application is closed, or a historical backup service. Keep independent JSON exports.

One browser tab at a time can edit a notebook on an origin. An exclusive Web Lock protects the shared IndexedDB cache from two tabs saving different offline drafts. A second tab explains how to continue; close the editing tab and reload the other one. Separate browser profiles/devices keep separate caches and can edit concurrently through sync. Web Locks require a current browser and a secure context (HTTPS, or localhost). A browser without that API stays paused instead of risking its cache.
