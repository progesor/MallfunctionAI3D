# Studio workbench UI

Version M1.3 removes the previously accumulated CSS override layers.

The UI is split into four independent zones: navigation sidebar, scrollable production/library panel, central GLB viewport, and a scrollable asset inspector with GPU footer. **The viewport owns the remaining space**, not the side panels.

Focus uses `.app-shell.focus-mode` + `.main-grid.inspector-expanded`. A higher-specificity one-column grid rule is applied at every breakpoint. ESC or the same button restores normal mode. Independent header controls toggle the left/right panels. At window width below 1120 px the right inspector starts hidden but can be reopened.

The generation button is permanently visible at the foot of the production panel while long workflow settings scroll independently. The bottom activity monitor is intentionally one line high. Settings and dialogs are unchanged except for styling.

The user-approved `source` workflow production path and its seed-fidelity safeguards were deliberately not modified. The M1.3 user-facing acceptance gate is a **single existing GLB visualization** on Windows and a focus/in/out interaction. No new AI generation is required to validate the layout.
