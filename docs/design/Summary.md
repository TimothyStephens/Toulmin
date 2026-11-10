# Toulmin: summary

Nov 9, 2026 · @Tim

## What Toulmin is

Toulmin links every claim in a scientific report to the exact evidence behind it, checks that link automatically where it can, and lets people review it. The evidence is the specific data, the declared steps applied to it, the cells those steps produce, and the history of how that data was made.

It is built first for reports written by AI agents and reviewed by humans, but human writers use the same path. The name comes from Stephen Toulmin's model of argument (claim, grounds, warrant, backing, qualifier, and rebuttal), which the system uses as its working vocabulary.

Toulmin runs alongside git and does not replace it. It sits idle while the user or an agent works. When a script or analysis is run, it runs it, records fingerprints of the script, inputs, and outputs, and goes idle again. When the user or an agent flags a file as important, it copies and fingerprints it and prepares it for claims. File history and backups stay with git.

## The problem

Checking a claim in a report today means rebuilding the writer's work by hand. A report says "Gene X is upregulated under condition Y vs Z, and has the 5th highest fold change," and cites "Dataset 1." The reviewer opens the table, guesses the filter and sort, finds the gene, and compares numbers.

That is slow for the reviewer and error-prone for the writer. Typical slips are copying the p-value instead of the adjusted p-value, reading a neighboring column, or dropping "likely" from an annotation.

AI agents sharpen the problem in two ways:

- **Speed.** Agents produce analyses and drafts faster than a person can retrace them by hand.
- **Hidden choices.** An agent's filters, thresholds, ranking scope, and handling of ties and missing values are rarely stated, so the same sentence can rest on very different analyses.

Existing executable-article tools link code to a paper as a whole. They do not tie each sentence to the exact cells that support it at the moment it is written, and they do not track the full history of an analysis.

## Who it is for

The first user is a single computational scientist running their own analyses, often with AI agents doing much of the writing. Others, in any field, should be able to adopt it without changing how they work.

| Who | What they need |
| --- | --- |
| Owner (the scientist running the project) | To let agents do tedious analysis and drafting without losing control of the record, and to review claims quickly |
| Agent (an AI model working on the project) | A clear, limited way to query what is registered, run steps, and draft claims, with no way to approve its own work |
| Collaborator or reviewer (second version) | To open a shared project, re-check the claims themselves, and record their own review, without installing the full system or trusting the owner's word |

It must work where research actually happens: a personal workstation or a shared cluster, without administrator rights, and with or without a separate machine or user account to isolate agents.

## Goals

1. **Every claim is linked; quantitative claims are checked.** Each claim points to data registered in the system, the steps applied to it, the cells it cites, and the rule that makes those cells support it. Quantitative claims (a value, a rank, a comparison) are checked by the system. Qualitative claims, which summarize or interpret results, are linked to their evidence and left to expert review. A claim can be either kind or a mix, and the system does not dictate how each is used.
2. **Hidden choices become explicit.** For a checked claim, filters, thresholds, ranking scope, ties, and missing values are part of its declared steps, so they are stated by construction.
3. **Reviewers see the evidence at once.** For any claim: the cited cells highlighted in their full row, the neighboring rows, and the output of each declared step.
4. **Methods can be reviewed inside the system.** For each analysis run, the reader can see the script as it is on disk, with a flag if it has changed since the run.
5. **Agents do the tedious work safely.** Agents can write analysis code and draft claims, but cannot alter the record, approve their own work, or hide what they did. Everything they produce is attributed to them.
6. **Provenance goes all the way back.** Any registered file can be traced through the analysis runs that produced it to the registered inputs at the root.
7. **Files describe themselves.** Every registered file carries a stable title, a description, and its source where one applies. Users and agents can retrieve this on request, and the viewer shows it beside the file.
8. **Everything is queryable before writing.** Before any text is written, the owner or an agent can query everything registered from the workspace: files and their metadata, runs, lineage, and check results. A capable agent can get its evidence in order before it writes a word.
9. **Changes are visible, not silent.** When a file gets a new version, dependent claims are re-checked and quantitative claims that no longer match are flagged. When a cited file is no longer produced, claims that cited it are highlighted.
10. **Works without special privileges.** A single user on Linux, including shared clusters, with no administrator rights.
11. **Export and import.** A project can be exported and imported again, to validate and test the system's output. Sharing with collaborators comes in the second version, and the design allows for it from the start.
12. **Modular and replaceable.** Each part can be rewritten or swapped for a better tool, as long as it keeps the same records and passes the same shared tests.
13. **Imperfect setups still count.** A weaker setup that is recorded honestly is better than no record, so the system allows it and labels it.

## Non-goals

- **Judging scientific validity.** Toulmin checks that claims follow from the data, not that the analysis design is sound. That stays with human reviewers.
- **Reducing how many claims a reviewer reads.** A well-defined task produces a manageable number of claims, and a good reviewer checks each one anyway. Toulmin makes that existing work easier rather than adding a new burden. An agent producing far too many claims points to a poorly defined task, not a design problem.
- **Policing writers.** Toulmin does not detect unlinked numbers or force a style of writing or analysis. A reviewer sends back weak claims.
- **Running workflows.** Existing workflow tools run analyses; Toulmin records them. It does not require any particular workflow tool or project layout.
- **Version control, backup, and recovery.** Git keeps file history and backups. Each run's script, inputs, and outputs are fingerprinted, but only files flagged as important for claims are copied into the record. A future version might grow into an all-in-one tool.
- **Sharing and multi-user use in the first version.** Each user has their own instance, with no logins or accounts. Sharing with collaborators comes in the second version.
- **Stopping a determined attacker when no isolation is available.** Without a separate machine, user, or container, tampering is detected and reported, not prevented.
- **Detecting cherry-picking.** The system does not count or judge the exploration that came before a claim.
- **Handling restricted data on the user's behalf.** What data an agent or a cloud model may see is the user's responsibility. The system provides controls and warnings, not compliance.

## Principles and trust model

The system computes; the author declares; humans judge. No single party is trusted for everything.

**Principles**

- **Nothing is overwritten.** Records are permanent. A correction is a new record that points at the old one.
- **System checks are reproducible.** The same data, steps, and engine always give the same verdict. Anything not reproducible (image claims, model judgments, text recognition) is labeled as such and never shown as a system check.
- **The author never writes the evidence summary.** Step outputs, row counts, plain-language descriptions of steps, and trust labels are computed by the system.
- **Safe by default, weaker by explicit choice.** Good practice is on by default. Weaker options exist, but only the owner can enable them, and each one is recorded and shown to every reader.
- **Agents cannot raise their own privileges.** They cannot approve, promote, opt in, change trust labels, or write outside their own draft area.
- **Everything a reader sees can be traced.** Every displayed number links back to a record, and every record links back to its inputs.
- **Enable, don't police.** The system makes transparent, checkable claims easy to write. Judging whether a writer used it well, and sending weak claims back, is the reviewer's job.
- **Every field's standards are welcome.** The system gives structure for good practice; it does not impose one way of writing or analyzing. Fields built on images and hand descriptions, such as ecology, are first-class. Labels describe how a claim is supported, never how good it is.
- **Iteration is normal.** Remade figures, panels combined into one figure, and abandoned analyses leave uncited files behind. That is ordinary revision, not a warning sign, and nobody is expected to get it right the first time.
- **Mistakes are highlighted, not punished.** When something changes in a way the user may not have intended, such as a cited file no longer being produced, the system highlights it and lets the user correct it as they meant to.

**Who is trusted for what**

| Actor | Trusted for | Not trusted for |
| --- | --- | --- |
| Owner | Approvals, promotions, opt-ins, and trust labels | Scientific correctness |
| Agent | Nothing by default; its output is labeled as its own | Anything privileged |
| Collaborator (second version) | Their own reviews, attributed to them | Changing the owner's records |
| Outside source (web page, PDF, downloaded file) | Nothing; its content is data, never instructions | Instructions of any kind |
| Imported project | Nothing until its integrity checks pass | Running any code it contains |

**Separate lines of evidence.** Each claim keeps its system check and its human reviews as separate verdicts, never merged into one score. A human review can annotate a system verdict but cannot override it. Qualitative claims have no system check: they rely on human review alone and are shown that way. Model checks may become a third line later.

**Isolation sets the guarantee.** When agents run on a separate machine, as a separate user, or in a container, they cannot change the record except through the system. With no isolation, tampering can be detected but not prevented. The isolation level of every agent session is recorded and shown beside its work.

## Key journeys

Four of these five journeys define the first version. Journey 4, collaborator review, comes in the second version.

**1. An agent drafts a claim, and the owner accepts it**

1. The owner starts an agent session on one project, with a stated role and isolation level.
2. The agent queries what is registered (files with their titles and descriptions, runs, lineage, and check results) and gathers its evidence before writing. Its queries are logged for diagnostics.
3. The agent proposes a claim: the sentence, the cited file, the declared steps, and the rule it relies on.
4. The system checks a quantitative claim and records a verdict, or records a qualitative claim for review.
5. The claim lands in the agent's draft area, labeled as the agent's work.
6. The owner reviews the draft and promotes it into the project record, or sends it back.

**2. The owner reviews a report**

1. The owner reads in their preferred order: methods, then data, then results, then discussion. Reviewing everything at once is also possible.
2. For any claim, they see the claim, its grounds, the output of each step, the rule, the provenance, the qualifiers, and anything that argues against it.
3. They confirm, dispute, or question each claim. Their review is recorded separately from the system's verdict.

**3. An analysis is run and recorded**

1. The owner reviews the script, flagged if it has changed since its last run, including any changes an agent made.
2. The analysis runs with their usual tools, and the system records fingerprints of the script, any declared support scripts, the inputs, and the outputs.
3. The system checks the run, and the owner approves it. Outputs the owner or an agent flags as important are copied into the record, with a title and description, and can then be cited.

**4. A collaborator checks the work (second version)**

1. The owner exports the project as a single package.
2. The collaborator opens it offline, re-checks the claims themselves, and adds their own reviews.
3. The collaborator sends back only their reviews, and the owner imports them.

**5. A file changes**

1. An analysis is rerun and a registered file gets a new version, keeping its title and description.
2. Claims that cited the old version are re-checked against the new one. Quantitative claims that no longer match are flagged.
3. If a cited file is no longer produced at all, it is retired, and the claims that cited it are highlighted as needing new evidence.
4. The owner fixes the script, points each claim at new evidence, or removes the link.

## Scope

The first version is for personal use: one scientist on Linux, working alone or with agents. It should include everything below, built in stages.

**In the first version**

- Registering files as inputs (raw data, downloaded tables, PDFs, web pages, images) or as outputs of a tracked analysis, each with a title, description, source, and other metadata that users and agents can retrieve. Files no claim cites are hidden by default, with a plain count of cited and uncited files. Re-registering a file after its analysis reruns keeps its metadata and adds a new version.
- Querying everything registered (files and their metadata, runs, lineage, and check results) from the workspace, for the owner and agents alike.
- Evidence from data tables, PDFs that have a text layer, and saved web pages.
- Quantitative, qualitative, and mixed claims: the system checks what can be checked and leaves the rest to review.
- Image and plot claims with a fuzzy expected-vs-observed check, labeled as checked by eye rather than by the system. Some analysis tools only produce images, and some fields work mostly with images, so these cannot wait.
- Claims with declared steps, verdicts, reviews, conflict detection, and stale-claim tracking.
- Reports with claims anchored in their sections, and sections marked by kind (methods, results, discussion).
- Recording analysis runs (fingerprints of each run's script, any declared support scripts, inputs, and outputs), their checks, and the owner's approval.
- Agent sessions with roles, isolation, a draft area, and a diagnostic log of each agent's queries and the data it was shown.
- A viewer for the owner, including each file's title and description, and each run's script as it is on disk, flagged if it has changed.
- Export and import of projects, to validate and test the system's output.
- Field-specific requirements, starting with a general profile and one for RNA-seq.

**Proposed build order** (a recommendation, not yet agreed)

Each stage ends with something usable and testable on its own. Agents come after the record and runs, because an agent's work is drafting claims against registered files and proposing runs, and its limits (draft area, no self-approval) are defined in terms of that record. Before stage 4, agent-written claims can be tested by entering them through the owner's own commands, though they will be attributed to the owner.

1. **Record and claims on tables.** Register input files and flag important outputs, with titles and descriptions; declare steps one at a time; register quantitative, qualitative, and mixed claims, with verdicts and reviews. Query everything registered. Export and import, for testing the output.
2. **Runs.** Record a run's script, inputs, and outputs, check it, and approve it, so files trace back to the run that made them.
3. **Reports and viewer.** Claims anchored in report sections, and the viewer: reading modes, the evidence panel, and the methods view with scripts flagged when changed. Usable on real, human-written reports from here.
4. **Agents.** Sessions with roles and isolation, the draft area, promotion, and the diagnostic query log. Usable for agent-written reports, the main aim, from here.
5. **More evidence.** PDFs with a text layer, saved web pages, and image claims with the fuzzy check.
6. **Change over time.** New file versions with re-checking and flags, retired files, and field profiles beyond the general one.

**Later**

| Stage | Aim | Examples |
| --- | --- | --- |
| Second version: sharing and collaboration | Let others check and review the work | Collaborator packages that can be re-checked offline, review round trips, views for reviewers with a particular expertise (statistics, for example) |
| Filling out the core | Cover more tools and smooth daily use | More workflow tools, more ways to isolate agents, editing claims, a version-comparison screen, highlighting numbers in claim text that are not linked to evidence |
| Non-expert users and deeper review | Help people who cannot judge an analysis themselves | Decision logs for each analysis, design and quality checks, flags for possible fabrication, model checks as a separate line of evidence, more field profiles |
| Labs and the ecosystem | Share across a group and beyond | A shared lab instance, syncing between instances, image text recognition, export to common writing formats, sharing only cited rows with proof, deposit to public archives |
| Longer term | Make the format a standard | An open claim and package format that journals could accept, an agent benchmark based on how well agents' claims hold up, and possibly an all-in-one tool that also handles file versioning, backup, and recovery |

## Decisions

Each decision below is deliberate. The design review may challenge any of them, but should engage with the reason given.

| Decision | Why |
| --- | --- |
| Toulmin requires a git repository and works alongside git without replacing it; users are encouraged to use git throughout, and agents may use it too | Makes clear where git ends and Toulmin begins; file history and backups stay with git |
| Each run's script, inputs, and outputs are fingerprinted, plus any support scripts declared when the analysis is registered; only files flagged as important for claims are copied into the record | Keeps the record focused on the study's final results and supporting evidence, such as a table of read counts, without searching for every possible script |
| Claims can cite only files registered in Toulmin, either as inputs or as outputs of a tracked analysis | Registered inputs are the roots of an unbroken verification tree |
| Each registered file has a stable title (never "Figure 1"), a description, and a source where one applies | Figure numbers change as a report is reorganized; a good title and description do not |
| Files no claim cites are hidden by default, with a plain count of cited and uncited files | Uncited files are a normal part of revising, but the count keeps them visible |
| A cited file that its script no longer produces is retired (hidden, not citable again), and claims that cited it are highlighted as needing new evidence | The change may have been a mistake; highlighting lets the owner fix the script or the claim, and no claim silently loses its support |
| Steps are declared one at a time (for example, sort by a column, then keep the top 10) and translated to a standard query language behind the scenes | The output of each step can be shown on its own, so the user can see what each step did and debug it |
| Claims can be quantitative, qualitative, or a mix; the system checks what can be checked and does not dictate how each kind is used | Datasets differ, and summaries such as "the top 10 genes are energy-metabolism related" need expert interpretation |
| Numbers in claim text are not detected or enforced in the first version; a reviewer sends back claims that should have been checked | Number detection adds work and false positives, and some hard numbers (outliers read off a figure) need manual validation anyway |
| When a file gets a new version, dependent claims are re-checked and quantitative claims that no longer match are flagged; the owner decides what to keep, and automatic acceptance is an opt-in | Updating one file flags only the claims that depend on it |
| The first version is for personal use; sharing and collaboration come in the second version, and the design allows for them from the start | Keeps the first version focused. Being ready for sharing means reviews attributed to named people, records that make sense away from the owner's machine, and an export format that will not need to change. Remote guest access is left to the operating system |
| One instance per user, with no accounts or logins | Keeps the first version simple and secure |
| Works without administrator rights, on one machine or with agents on a separate host | Research often runs on shared systems; a separate host is the ideal setup, but any transparency beats none |
| The record is always tamper-evident; the owner signs at checkpoints, not every change | Detects tampering everywhere while keeping signing practical |
| No required workflow tool or project layout | People keep their existing tools |
| Good practice on by default; weaker options are owner-only, recorded, and shown | Allows imperfect setups without hiding them |
| Cloud AI models are allowed, with a clear warning and per-dataset controls | Most people use them; the risk is stated, not banned |
| Agent queries are logged for diagnostics, but there is no exploration count | How much an agent explores depends on its prompt, effort setting, model, input files, and the task, so a count has no reliable meaning yet |
| Projects are exported in the project's own format, wrapped in a widely used research-packaging standard | Other tools can read the package's description without understanding everything in it |
| Built as separate parts behind defined interfaces, with shared tests | Any part can be rewritten or replaced, and coding agents can build one part at a time |
| Linux first; the most open licenses (permissive for code, public domain for the specification) | Matches where the first users work, and encourages reuse |
