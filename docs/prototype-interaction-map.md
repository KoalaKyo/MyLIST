# Desktop prototype interaction map

The React and Vite prototype in app/prototype was used to validate the compact Windows desktop-widget layout and its core interaction model. Its data is held in page memory and resets when the page reloads.

## Main window

| Area | Interaction | Result |
| --- | --- | --- |
| Pin icon | Click | Switch between desktop and always-on-top behavior |
| Settings icon | Click | Open settings |
| Close icon | Click | Simulate minimizing to the system tray |
| Pending / Completed tabs | Click | Switch task lists and update counts |
| Task completion control | Click | Complete or restore a task |
| Task content | Click | Open the edit view |
| Add button | Click | Open the task creation view |
| Bottom-right resize handle | Drag | Resize from the lower-right corner |

## Task creation and editing

- A title is required.
- Tasks support notes, category, due time, and color.
- Saving updates the list and counts immediately.
- Completed tasks can be restored.

## Settings

- Appearance includes system, light, and dark themes.
- Categories can be created, renamed, recolored, and removed subject to system-category rules.
- Data workflows cover plaintext export, encrypted export, import preview, merge, and replace.

## Layout and accessibility

- The default target size is 360 × 520 DIP.
- Compact layouts keep primary actions reachable and allow internal scrolling where required.
- Interactive controls expose focus states and accessible labels.
- Reduced-motion preferences are respected.
