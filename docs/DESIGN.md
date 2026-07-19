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

- Image: `docs/assets/kelpie-mark.svg` — kelpie's line-art water horse,
  inlined in the header markup with `fill="currentColor"` so it inherits
  `.ae-app-mark`'s `color: var(--ae-ink)` and themes for free (no filter
  hack, no raster asset). Traced from the original PNG via potrace;
  `assets/kelpie-mark.svg` at the repo root is the canonical source, copied
  into both `docs/assets/` and `static/`.
- **Deliberate deviation** from the site-kit's Lucide-only mark contract:
  kelpie has a real product mark (it is also the app's PWA icon and favicon),
  and the operator asked for it on the site. The image sits inside the
  standard `.ae-app-mark` frame so the header geometry stays kit-identical.
- Everything else follows the kit: no colored wordmark, no emoji marks.

## The Well on the public site

The site and app now share one visual language: warm parchment, the four
`vellum / panel / well / well-deep` decks, quiet ridges, a single teal signal,
and 6/10/16px radii. The public page defines `--w-*` tokens in
`docs/marketing.css`, then maps them onto the shared Aesthetic `--ae-*`
contract. This preserves the site kit's structure while making the rendered
site unmistakably kelpie.

Light accent is `#0c7263`; dark accent is `#4dc4b0`. Light depth uses small
lift shadows. Dark depth uses border rings. Screenshot frames and quickstart
code are recessed wells; feature rows are raised panels. The site uses the
same system sans and mono roles as the app.

## Layout

One focused, document-scrolling page. The header is sticky; the footer remains
in normal flow. The hero introduces the operator outcome and proof. Features
use compact panel rows with mono labels, screenshots sit in well-backed frames,
and quickstart is one deep code well. The full composition remains
single-column at phone width and expands without changing hierarchy on desktop.

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
