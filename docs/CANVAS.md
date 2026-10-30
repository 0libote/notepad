# Working on the canvas

Each page has its own board. Write, Read, and Canvas refer to the same page. Drop an image, `.eml` message, web address or plain text to collect it. A dropped Markdown file creates a new page and a linked card; Add note to canvas adds a live reference to an existing page.

Select a card by its header or content. Drag its header to move it. The corner handle resizes the selected card; images retain their proportions inside the frame, and longer text or previews scroll. The small controls below show the frame dimensions and offer Duplicate and Delete. Duplicating a linked-page card creates another reference to that page; it does not duplicate the page itself.

Choose Connect cards, then choose a source card and a destination card. The arrow follows their borders as the cards move, resize, or change layout. Escape cancels a pending connection. Connections belong to this board; they are separate from Markdown links and backlinks between pages.

Select a line or drawing, then use Delete below or the Delete/Backspace key to remove it. A single pen tap makes a dot that can also be selected. Removing a card removes its attached connections in one step; Undo restores the card and connections together. Deleting a linked-page card leaves the referenced page intact.

| Control | Action |
| --- | --- |
| Tab | Focus board controls, card headers, resize handles, drawings and connections |
| Arrow keys on a selected card/header | Move 10 canvas pixels |
| Arrow keys on a resize handle | Change width or height by 10 canvas pixels |
| Shift + arrow keys | Use 40-pixel steps |
| V / P / C | Select / pen / connect, while the canvas has focus |
| Enter or Space on a card header in Connect mode | Choose that connection endpoint |
| Delete / Backspace | Remove the selection, outside text fields |
| Ctrl/Cmd + D | Duplicate the selected card, outside text fields |
| Ctrl/Cmd + Z | Undo a board gesture, outside text fields |
| Escape | Clear selection and cancel a pending connection |

Text fields retain normal typing, deletion and native text Undo. The board Undo history holds up to 40 snapshots for the current session. Remote content changes clear this local history so Undo cannot silently restore an old remote page. Redo, multi-selection, endpoint reassignment, line labels and grouped drawings are not implemented yet.

The board currently spans 2600 × 1800 canvas pixels. Scroll to move around and use the zoom controls to change scale. Resized cards stay inside that world; frames range from 180 × 120 to 1000 × 1200, limited by the remaining space at their position. Existing cards and old backups retain automatic sizing until resized. Dimensions and connectors travel through local drafts, server sync, conflict copies and JSON backup exports. Board geometry is not part of Markdown source exports.

The model follows the node/edge distinction used by the [JSON Canvas specification](https://jsoncanvas.org/spec/1.0/): dimensions belong to nodes, and edges refer to node IDs. This app does not yet import/export `.canvas` or `.excalidraw` scenes. Drawing and card interaction code is written in Rust, with DOM text editing and SVG rendering.

## Format upgrades

Sizes and connections use workspace format 2. The server upgrades a format-1 notebook at startup in a transaction, preserving its content and revision. Browser/desktop drafts, sync baselines, and JSON backup imports also upgrade from format 1. Old automatic card layouts remain unchanged until you resize them. Future or invalid formats fail validation and are kept rather than overwritten.

The server accepts writes only in the current format. An older client attempting to save format 1 receives an error instead of silently dropping dimensions or connections it cannot understand. Reload a browser client after upgrading the server, and ship desktop builds with their matching web assets. Export a backup and stop the old server before upgrading its installation. Future fields that an old writer could discard must also introduce a format version change; read-side migrations must not normalize network writes before the version check.
