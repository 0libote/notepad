# Drawing and collecting on a page

Each page has Write, Read and Canvas views of the same content. Canvas now opens a white drawing surface with a floating toolbar and a collapsible Style panel. Notes remain available from Toggle notes.

Draw rectangles, diamonds, ellipses, arrows, lines, freehand ink and standalone handwritten text. Drag to create geometry; Shift constrains proportions or line direction. Select to move, use the corner handle to resize, and double-click text to edit it. Shift-click shapes for multiple selection, or drag a selection rectangle over shapes. Stroke/fill, width, dash, clean/sketch, font size and opacity apply to the selection and new drawings. Layer controls bring shapes forward or backward.

| Shortcut | Action |
| --- | --- |
| V / H | Select / hand |
| R / D / O | Rectangle / diamond / ellipse |
| A / L / P / T / E | Arrow / line / pen / text / eraser |
| Space + drag | Pan |
| Ctrl/Cmd + wheel | Zoom around the pointer |
| Ctrl/Cmd + A | Select all shapes |
| Ctrl/Cmd + D | Duplicate selection |
| Delete / Backspace | Remove selection |
| Ctrl/Cmd + Z | Undo |
| Ctrl/Cmd + Shift + Z, Ctrl/Cmd + Y | Redo |
| Escape | Finish text or clear selection |

Shortcuts apply outside text fields. Text fields retain native editing. History holds 40 workspace snapshots in the current session and clears after remote content changes. Pan and zoom do not modify notebook content. The board supports negative coordinates; stored geometry is bounded to ±100,000 pixels.

Insert adds text cards, stickies, website links, emails and live links to existing pages. Images can be picked from the toolbar. Drop images, .eml messages, web addresses, plain text or Markdown files onto the board. Card headers move cards, corner handles resize them; ordinary content retains typing and clickable links. Connect cards creates an arrow anchored to card borders. Removing a card removes its connections; deleting a page-link card leaves its page intact.

The interface and geometry are Rust, using DOM text editing and SVG rendering. Excalifont is bundled under its OFL license. This is an initial drawing editor: shape rotation, grouping, bound shape arrows, pressure input, mixed card/ink/shape multi-selection, and .excalidraw import/export remain unimplemented.

## Format upgrades

Workspace format 3 adds styled shapes and ink. Format 1 and 2 notebooks, local drafts, sync baselines and JSON backups upgrade while preserving existing notes, cards, strokes, dimensions and connections. The server upgrades stored data transactionally; network writes require the current version so older clients cannot silently discard drawings. Refresh web clients after upgrading the server; desktop clients need matching assets. Geometry is included in JSON backups and sync, but not Markdown exports. Floating-point JSON parsing preserves exact geometry to prevent endless sync changes.
