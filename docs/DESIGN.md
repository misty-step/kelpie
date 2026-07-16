# kelpie site DESIGN.md

This file is the public-site brand contract for kelpie's marketing page at
https://misty-step.github.io/kelpie/. Update `docs/` from this file; do not
invent a second design system. The kelpie *app's* design system lives at the
repo root `DESIGN.md` and is a separate contract.

## Brand Voice

- Plain-spoken, concrete, and operator-facing.
- Lead with the user outcome, then the proof.
- Short sentences. No marketing fog, no mascot language.

## Pitch One-Liner

`kelpie helps one operator triage a fleet of omp coding agents from a phone
without babysitting terminals.`

## Mark

- Image: `docs/assets/kelpie-mark.png` — kelpie's line-art water horse,
  dark strokes on transparent, inverted via CSS in dark mode.
- **Deliberate deviation** from the site-kit's Lucide-only mark contract:
  kelpie has a real product mark (it is also the app's PWA icon and favicon),
  and the operator asked for it on the site. The image sits inside the
  standard `.ae-app-mark` frame so the header geometry stays kit-identical.
- Everything else follows the kit: no colored wordmark, no emoji marks.

## Palette Hooks

kelpie steers only the accent pair, matching the app's own accent
(`static/style.css`):

```css
:root {
  --ae-accent: #0f6e6e;
  --ae-accent-dark: #2dd4cf;
}
```

No named theme is pinned; everything else is stock Aesthetic ink.

## Layout

One focused page. Sticky header bar, sticky footer bar; content scrolls
inside `.ae-stage-scroll` per the Aesthetic "no page scroll" law. Sections:
hero, features (`ae-list-rows`), screens (three phone shots, zoomable),
quickstart (one code block).

## Screenshot Inventory

| File                                          | Surface       | State                              | Caption          |
| --------------------------------------------- | ------------- | ---------------------------------- | ---------------- |
| `docs/assets/screenshots/inbox-light.png`     | Inbox         | Live fleet, attention-sorted, 390×844 | Inbox · light    |
| `docs/assets/screenshots/session-dark.png`    | Agent session | Transcript + composer, 390×844     | Session · dark   |
| `docs/assets/screenshots/terminal-dark.png`   | Raw terminal  | PTY screen + key row, 390×844      | Terminal · dark  |

Screenshots are real captures of the live app. Retake at 390×844 whenever the
app's header, composer, or card anatomy changes.

## Footer Links

- Misty Step: `https://mistystep.io` (always)
- GitHub: `https://github.com/misty-step/kelpie`
- Weave: omitted — kelpie is not a weave-family product.

## Release Notes Rule

No changelog page yet; the page links to the GitHub repo instead. If release
notes are added later, restore the kit's `changelog.html` pattern.

## Deployment Note

The site kit's convention is `site/` plus a Pages workflow. This repo serves
GitHub Pages from `main:/docs` (branch build) instead because the publishing
credential lacks the `workflow` OAuth scope; the directory is the kit's
`site/` scaffold, renamed. If a workflow-scoped credential lands later, move
this back to `site/` and restore `.github/workflows/pages.yml` from the kit.
