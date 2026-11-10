# toulmin

Scaffolded by `ab init python`. It runs as an agentbox session (the agent) and, for the `pod`
template, as a live stack on the host (you, via deploy).

## The cycle

**Capture** — real samples go in `dev/fixtures/` (and private ones in `dev/fixtures/private/`, which is
gitignored). The agent develops against these, never against live data.

**Develop** — start a session (`ab run --agent <agent> --project toulmin`). Inside it the agent
tests against `dev/fixtures/`, and commits. It sees `.env.dev` as `.env`; the real `.env` is masked.

**Verify** — you run against live credentials and live data on the host, outside the sandbox.

## What the agent cannot see

- The real `.env` (masked; the agent sees the fake `.env.dev`).
- The real data directory (`DATA_DIR` in the masked `.env`); in a session `DATA_DIR` is `./dev/data`.

## Backups

- `git` (and the upstream, if set) backs up the CODE only.
- `.env` is gitignored and never leaves the host.
- Live data lives at the real `DATA_DIR` (outside this repo); agentbox does not back it up.
