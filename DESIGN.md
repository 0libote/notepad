# Notepad design

A quiet personal workspace. The working surface, rather than product chrome, gets the room.

Locked direction: drawing-first canvas on a white surface with violet and lavender selection in the manner of Excalidraw, a compact floating centered toolbar for primary drawing tools, a left properties panel at 202px, and very quiet top chrome. Note, image, and email captures live in the Insert menu, not the toolbar. Read and Write keep the existing note styling, measure, and type.

Use the Ribbon app treatment, adapted to a muted blue accent at the user's request. Chalk #f7f8fa, white #ffffff, ink #263142, slate #5d6a7b, blue #486b9c, rule #e0e5ec. Drawing selection adds violet #6965db, lavender #e6e5ff, and deep violet #4f4bb8 for focus. Yellow belongs only to sticky notes as an identifiable object. All colours and fonts are theme tokens in `crates/web/styles`. Instrument Sans for titles, Figtree for reading and controls, bundled fonts and system fallbacks if fonts are unavailable. Keep labels sentence case and small but readable.

The identity is one note with a linear page (Write and Read) and a spatial canvas. No marketing header, ornamental metrics, gradients, stock photography, or animation. Dot-grid marks are spatial reference points on the canvas. The note rail is a file navigator, not a general dashboard sidebar. Writing measure is capped at 760px. The board fills the available viewport and scrolls in both directions; mobile hides the rail behind a button. Visible focus outlines and reduced-motion rules are required.

Controls: 36px visual footprint, compact tool grouping; enlarge hit targets if touch testing shows a need. Blue marks selection, focus, or the primary save action. Card headers are drag handles. Destruction is reversible with undo. Errors and persistence status appear in a live region. Never claim content was saved remotely until the server responds successfully.
