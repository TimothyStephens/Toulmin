# toulmin

This project runs in an agentbox sandbox. These are enforced outside your control, not requests:
- .env is masked; you see .env.dev as .env. Do not try to read real secrets — you can't, and the
  values you see are fake. Real runs against live credentials happen outside this sandbox.
- live data and live credentials are unreachable to you. Develop against dev/fixtures/,
  never live data.
- You may build, run and test the dev stack (compose.dev.yaml) freely; it only affects this project.
- Network egress is restricted to the project's allowlist.
Workflow: develop against dev/fixtures/; the human captures real samples and verifies live on the host.
agentbox injects your real egress mode, data layout and limits at the start of each session — read the
"agentbox sandbox" section it adds for the authoritative, live description of this environment.
