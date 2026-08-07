# kelpie — notes for agents

Two surfaces, one product. Read `VISION.md` before scoping work: `src/` +
`static/` are the phone-first PWA (axum bridge + Yew/WASM), `desktop/` is the
Tauri 2 + React desktop console.

## Desktop changes must reach the installed binary

`kelpie` on a workstation is the installed release binary in `~/.local/bin`, not
a dev server. Editing `desktop/src` and getting clean `tsc`/`vite build` output
proves the code compiles — it says nothing about what the operator sees.

- Verifying a desktop change: `cd desktop && npm run install-local`, then
  exercise the relaunched app.
- Commits touching desktop sources reinstall automatically via
  `scripts/git-hooks/post-commit` (enable per clone:
  `git config core.hooksPath scripts/git-hooks`).
- The Wayland window is invisible to X tools such as `evcap`. For a capture,
  run a throwaway second instance with `GDK_BACKEND=x11`.
- Launch detached processes with `setsid nohup …`; a plain `nohup … &` from a
  tool shell does not outlive the session.

## Board

Work lives on the Powder board, repo label `misty-step/kelpie`:
`powder list-cards --repo misty-step/kelpie`. No local ticket files.
