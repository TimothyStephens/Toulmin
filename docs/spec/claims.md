<!-- design-sections: 7, 11.22 -->
### 7. Claim states and the evidence-exists outcome table

#### 7.1 States of a claim

```text
proposed (journal) --promote--> promoted --review--> reviewed
      |                            |
      +--send back--> rejected     +--retract--> retracted
      |                            +--supersede--> superseded (new claim added)
      +--revise--> proposed (new revision, same ref)
owner claim: added (main chain) == promoted
```

The current revision of a ref is the latest promoted revision, or, for a ref with no promoted revision, the latest proposal; the promote event names the exact revision event it accepts (D29). Any session may propose a revision of any unpromoted or sent-back ref. Revisions of one ref across journals are ordered by read-in order, then journal `seq`; a proposal response is provisional until read-in. A revision read in after the ref has a promoted revision is recorded as rejected with the reason "ref already promoted". Every revision event records its authoring session, which the views show beside the ref. Promoted claims change only by owner action.

#### 7.2 Checks per kind

| Kind | Evidence-exists check | Assertion check | Review |
| --- | --- | --- | --- |
| Quantitative | Yes | Yes | Yes |
| Qualitative | Yes | None (shown as "no system check") | Yes, the only line of judgment |
| Mixed | Yes | Yes, for the quantitative part | Yes |

#### 7.3 Evidence-exists outcome table

Rows are citation types; columns are the cited version's situation at check time. The check runs against the pinned version. "Disk differs" means the on-disk file no longer matches the pinned fingerprint.

| Citation type | Kept, or on disk and matches | On disk, disk differs | Missing, not kept | Removable and deleted, not kept | Fingerprint only (owner chose) and missing | Retired |
| --- | --- | --- | --- | --- | --- | --- |
| Cells by key | Holds if every key is present; fails if a key is absent | Holds or fails against the pinned bytes if kept; otherwise `not checkable` plus flag `evidence moved: disk differs` | `not checkable` | `not checkable`, labeled `expected-missing` | `not checkable` | As the matching column, plus flag `cited file retired` |
| Whole file | Holds | Holds against the pinned version; flag `evidence moved` | `not checkable` | `not checkable`, `expected-missing` | `not checkable` | As the matching column, plus flag `cited file retired` |
| Page | Holds if page <= page count of the pinned version; fails otherwise | As cells | `not checkable` | `not checkable` | `not checkable` | As the matching column, plus flag `cited file retired` |
| Panel | Holds if the label is registered on the pinned image version; fails otherwise | Holds (labels are metadata); flag `evidence moved` | `not checkable` | `not checkable`, `expected-missing` | `not checkable` | As the matching column, plus flag `cited file retired` |
| Reserved (quote, page region, image region) | `not checkable` | `not checkable` | `not checkable` | `not checkable` | `not checkable` | `not checkable` |

An absent key is `E6003`, a refusal, only when `tm claim add` first validates the citation, which makes it a typo in the command; every check after that, including a check against a version the key has since left, records the outcome in this table instead of refusing. A claim is never shown as "holds" from this table unless the cell says holds. `error` is reserved for failures of the checker itself (parse error, DuckDB error) and is shown with the error code.

Reading the columns: a version on disk whose bytes differ from the pinned fingerprint is in "On disk, disk differs" whether or not it is kept, and "Kept, or on disk and matches" covers a kept version that is missing from disk; a missing version that is marked removable and not kept (bytes not kept, or fingerprint only) is in "Removable and deleted, not kept", before "Fingerprint only (owner chose) and missing". The `evidence moved: disk differs` flag goes with every outcome in the "disk differs" column, as the whole-file and panel cells state it, since `evidence moved` is computed from the stat cache whatever the outcome (item 66). A claim's `evidence` in `claim.checked` combines its citations: `fails` if any citation fails, else `not_checkable` if any is not checkable, else `holds`; a claim with no citations is `not_checkable`. `evidence.detail` is `null` for `holds`, else one clause per citation that did not hold, in citation order, joined by `; `, each `<i> <subject>: <reason>` with `<i>` the citation's index from 0, `<subject>` its `version` (a reserved kind's `kind`), and `<reason>` one of `a cited key is absent`, `page beyond the page count`, `panel not registered` (fails), `disk differs, bytes not kept`, `missing, not kept`, `removable and deleted, not kept`, `fingerprint only and missing`, `reserved citation kind` (not checkable).

#### 11.22 Span grammar and drift normalization

A span is `[` text `]{` attributes `}` on one logical line of the report; text may not contain an unescaped `]`, and `\]` escapes one. Attributes are whitespace-separated; the first must be `.claim`; `ref=<ref>` is required; `<ref>` matches `C[1-9][0-9]*` or `S[1-9][0-9]*\.[1-9][0-9]*` (the 11.16 ref form); other attributes are stored as given and not interpreted. A span inside a fenced code block (lines between ```` ``` ```` or `~~~` fences) is not a span. A span whose text contains inline code (backticks) is recorded with `inline_code` true, and its drift check is `not checkable`. Two spans with the same ref in one report are both recorded (one claim, two locations). A heading class is `{.<kind>}` at the end of an ATX heading line; the scanner records the kind of the nearest preceding heading for each span as `section_kind` (in the `spans` table), used only for display. Normalization for drift, in order: NFC; replace every run of Unicode whitespace with one space; trim.

Scanner rules. A logical line is one line of the file split at LF with a trailing CR removed; lines are numbered from 1. On a line, candidates are tried left to right: from a `[`, the text runs to the first `]` not preceded by `\`; when `{` follows that `]` directly and a `}` closes the attributes on the same line, and the attributes make a span, the span is recorded and the scan resumes after its `}`; otherwise the scan resumes at the character after that `[` (so `[a [b]{.claim ref=C1}` records the text `a [b`). `\]` is the only escape: the recorded text has each `\]` replaced by `]` and is otherwise as written, not normalized; any other `\` is literal. The attributes are the characters between `{` and the first `}`, split at runs of Unicode whitespace; they make a span when the first is `.claim` and exactly one attribute begins `ref=` and the remainder is a ref; anything else is not a span and not an error. A fence opens at a line that, after at most three spaces, begins with a run of at least three backticks or at least three tildes, and closes at the next line that, after at most three spaces, holds a run of the same character at least as long and nothing else but whitespace; an unclosed fence runs to the end of the file; both fence lines are inside the block, and nothing inside is a span or a heading. An ATX heading line is, after at most three spaces, one to six `#` followed by a space, a tab, or the end of the line; it carries a class when the line, trailing whitespace removed, ends with `{.` kind `}`, where kind is one or more characters none of which is whitespace, `{`, or `}`. A span's `section_kind` is the class of the nearest heading line above it, `null` when that heading has none or no heading precedes it. Drift compares the claim's current revision's `sentence` with the span's recorded text, both normalized: equal is no drift, different is `text drifted`, and a span with `inline_code` is `not checkable` whatever the texts.

---
