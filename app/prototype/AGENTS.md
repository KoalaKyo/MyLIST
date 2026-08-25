# Prototype Instructions

Run the local server yourself and open the preview in the browser available to this environment. Do not give the user server-start instructions when you can run it.

Before making substantial visual changes, use the Product Design plugin's `get-context` skill when the visual source is unclear or no longer matches the current goal. When the user gives durable prototype-specific design feedback, preferences, or decisions, record them in `AGENTS.md`.

When implementing from a selected generated mock, treat that image as the source of truth for layout, component anatomy, density, spacing, color, typography, visible content, and hierarchy.

Build app UI in `src/`. Keep `.openai/hosting.json`, `worker/index.js`, `scripts/prepare-sites-build.mjs`, and `tests/sites-worker.test.mjs` intact so the same local prototype can be handed to Sites. Before a Sites handoff, run `npm run build` and `npm run test:sites`; the build must leave `dist/client/index.html`, `dist/server/index.js`, and `dist/.openai/hosting.json`.

## Durable product decisions

- The selected visual source is `design/concepts/concept-1.png`: a 360 × 520 DIP Windows Fluent/Acrylic desktop widget.
- Preserve the compact single-widget shape, light translucency, two-tab hierarchy, realistic Chinese task data, and restrained blue accent.
- The prototype must support light, dark, and follow-system themes. Theme selection belongs in Settings only.
- Use official Microsoft Fluent System Icons from `public/icons`; do not draw or approximate visible icons.
- Do not generate more visual concept images unless the user explicitly reverses that decision.
- Keep all prototype behavior local and simulated. Do not add accounts, cloud synchronization, search, priorities, recurring tasks, subtasks, or routing.
- Align each task's reminder/time text with the type label on the second line; keep task title on the first line.
- The Windows title bar includes pin and close controls. The close control uses the same 34 × 34 icon-button geometry and Fluent icon weight as the pin control; Settings is an icon-only control at the lower-left of the footer.
- Theme selection belongs only in Settings; do not keep a quick theme toggle in the title bar.
- Settings must close through bottom “取消 / 保存设置” actions. Cancel discards draft changes; Save applies them. Do not place a close button in the Settings header.
- Standard form controls and primary/secondary buttons use a shared 32 px height. Typography follows the tokens in `styles.css` instead of ad-hoc sizes.
- Palette colors use circular swatches without rounded-square wrappers; keep the balanced slate, lake blue, pine green, amber, coral red, and iris purple set.
- Task rows are compact single lines in this fixed visual order: completion control, type chip, title, then time. Do not add separators between rows; distinguish the header and footer using only a subtly grayer surface. The todo/done switcher is a two-option pill, not an underlined tab bar.
- The todo/done switcher has one shallow-gray rounded outer container with a smaller rounded selected segment inside. Task time and reminder text are faint gray by default; only overdue/danger timing uses a semantic emphasis color.
- Settings navigation (常规 / 分类 / 数据) reuses the same shallow-gray outer segmented container and inner selected segment as the home todo/done switcher. Appearance uses one native select dropdown, not a segmented control.
- Settings pages have no horizontal divider lines. The title header and bottom Cancel/Save action area use subtly gray surfaces to separate them from the content.
- Settings header omits the eyebrow “本地设置” and uses the same 60 px title-bar height as the home screen. Settings navigation occupies the same 48 px row as the home segmented control; the bottom action area is a visible shallow-gray 54 px footer with comfortable button padding.
- Interactive color system uses a bold near-black action token for primary/confirm buttons, toggles, focus rings, and hover states. Selected segmented controls intentionally use a white background with near-black text so the state remains quiet against the shallow-gray outer container. Secondary controls stay neutral gray, and destructive controls remain red for semantic warning. Typography uses the shared button token (`12px`) and existing body/section/title scale.
- The centered floating Add button is a primary action: solid near-black background with a white add glyph by default; hover uses the slightly lighter action-hover tone.
- Todo/done segment labels put the count before the text. The selected count badge uses a solid near-black background with white numerals; the selected segment itself remains white with near-black text.
- Task completion controls use the action color rather than task type colors: incomplete tasks show a near-black outline circle, while completed tasks show a solid near-black circle with a white check for contrast.
- Time labels use compact copy: remaining durations omit “剩余” (for example `45分钟`), and overdue tasks display only `已逾期`. Newly created tasks calculate and use the same compact duration format.
- Remove the textual bottom “添加事项” button. Put a round, accent-soft, icon-only compact add button immediately left of the todo/done segmented control; align its center with the completion circles below.
- The lower-left Settings control is a 28 px round icon button aligned to the task completion controls. Its default surface and icon are shallow gray; it changes to blue/accent-soft only on hover. Keep the gray footer tall enough for balanced vertical padding (42 px normally, 38 px in compact height).
- The add control is not shown in the upper toolbar. It is a centered floating 48 px solid medium-gray circular button at the boundary between the task area and the gray footer, matching the reference composition. The add glyph is white; there is no outline ring, and hover may switch to blue for feedback.
- The bottom-right resize handle uses exactly six circular dots in a 1–2–3 right-isosceles triangle: the single dot is on the top row, the right angle is at the lower-right corner, and dragging it resizes the widget with a 320 × 360 DIP minimum. The visible dot matrix is inset equally (8 px) from the right and bottom edges; keep the larger 26 px hit target invisible.
