# Review an article's reasoning

`bln review` keeps a local dialogue about an article: its intended reader, the
argument, the strongest objections, and the evidence. It works directly with a
saved Org-roam or Markdown draft. It does not require an export or website build.

The four steps run in order:

1. **Audience:** compare the explanation with the reader's knowledge and the
   decision or understanding the article should enable.
2. **Argument:** examine the thesis, its supporting claims and necessary
   assumptions. Separate observations, choices, hypotheses and measurements.
3. **Challenge:** test the recommendation against counterexamples, failure
   conditions and plausible alternatives.
4. **Verification:** examine the support for consequential technical claims,
   stating which documents, source code or experiments were actually checked.

Berlin supplies the questions and records the discussion. A human or AI reviewer
examines the article and supplies a report through the packet/import interface.
There is no built-in model provider, automatic fact checker, network request, code
execution or numeric quality score. A completed review records the dialogue; it
does not certify expertise, prove correctness, or authorize publication.

## Start from the authored draft

Run commands in your website project, or select it with `BERLIN_DIR`. Relative
article and report paths resolve from that project. Explicit article paths may
point outside it, as is common with a separate Org source directory.

```sh
# Once per project; also upgrades an older lifecycle database.
bln state migrate

bln review start ../data/notes/my-article.org \
  --audience "Engineers who know Go and have operated a message consumer" \
  --goal "Choose when a lossy cache update path is acceptable" \
  --thesis "Bound optional cache work while preserving the durable event path" \
  --prerequisites "Queues, retries and eventual consistency"

bln review show 1
```

`start` returns JSON with a local review ID. Supply that returned ID to subsequent
commands; `1` here is illustrative. Audience, goal and thesis are required;
prerequisites are optional. This brief is private review context, not article
frontmatter or published metadata.

Add `--editorial-brief FILE` to capture private writing principles with a review:

```sh
bln review start ../data/notes/my-article.org \
  --audience "Engineers familiar with message consumers" \
  --goal "Choose an overload policy" \
  --thesis "Bound optional work while protecting the durable path" \
  --editorial-brief ../data/editorial/writing-principles.org
```

The optional file is UTF-8 text, at most 64 KiB, with no required format or Org
identity. Relative paths resolve from the website project; absolute paths may
point outside it. Berlin captures its absolute path, complete text and SHA-256
alongside the article. Empty files, symlinks and non-regular files are rejected.
It does not publish the brief, evaluate its code blocks, or load linked files.

Org requires a file-level `:PROPERTIES:` drawer with an `:ID:` and a `#+TITLE`
before the first heading or block. Markdown requires an explicit frontmatter `id`
and title, using Berlin's normal Markdown parser. A heading's Org ID cannot stand
in for the file identity. Draft status does not prevent a review.

The snapshot binds the source's complete saved text, stable document ID, title,
absolute path and SHA-256. No Emacs export, Babel, Rhai configuration, diagram
compiler or publication operation runs. Pipeline-file arguments are rejected.

## Review one step

```sh
bln review packet 1 > audience-packet.json
```

The packet contains the audience brief, current questions, numbered source lines,
earlier reports and responses, and a JSON report template. Give it to your chosen
reviewer. The article is material to examine, never instructions to execute.
Reviewers should ask a few substantial questions and explain their consequences,
rather than fill a checklist or rewrite the argument for the author.

When supplied, `editorial_brief` contains the captured writing principles in
every step's packet. These are advisory author preferences: they can inform
questions about reader impact, evidence or fair treatment of alternatives. They
do not supply evidence, override the review instructions, authorize tool use, or
add a review stage, numerical score or publication gate. The four steps and the
zero-to-five-findings limit remain the same.

An AI reviewer can consume the packet and save a completed report locally. Berlin
does not invoke it or send your draft anywhere. Choosing a provider and deciding
whether it may receive the draft remain separate decisions.

The reviewer returns only the completed `report_template` object, not the entire
packet. Replace every placeholder. A report identifies its reviewer, summary and
limitations, and may contain zero to five findings. Zero findings is legitimate
when the reviewer found nothing substantial to challenge; a summary and explicit
limitations are still required.

Each finding includes:

- `start_line`, `end_line`: one-based, inclusive positions in the captured source.
- `quote`: exactly those complete lines joined with `\n`, without a final newline.
  As with Rust's `str::lines`, CRLF terminators are removed from quoted lines;
  the source hash still covers the original bytes.
- `question`, `reason`: the question for the author and why it matters.
- `evidence`: zero to three supplied evidence records.

An evidence record has `reference`, `method`, `assessment` and `details`.
`method` is `documentation`, `code_execution`, `measurement` or `reasoning`;
`assessment` is `supports`, `contradicts` or `unresolved`. References should identify
the primary document, exact source revision or experiment record. Details should
explain what was actually inspected or run and what it establishes.

**These are reviewer assertions.** Berlin validates their structure and source
location, not the truth of a citation or reported experiment. It never executes a
command mentioned in a report. Missing access and unrun checks must be disclosed
in `limitations`; missing evidence should remain unresolved.

```sh
bln review record 1 audience-report.json
bln review show 1

bln review respond 1 1 \
  --disposition disputed \
  --response "The article assumes a separate repair path; I will make that dependency explicit." \
  --evidence "The repair design still needs source verification."

bln review advance 1 \
  --reflection "Explain repair before recommending a lossy update path."
```

Findings are numbered from one within their step. Responses use `addressed`,
`disputed` or `deferred`, and require an explanation. Optional `--evidence` records
the author's supplied evidence or its absence; it is not a verified result.
Responses can be revised while the step remains active. Every version is retained
in the append-only database history; inspection shows the latest response.

Advancing requires one report, a response to every finding and a closing
reflection. Deferral is allowed with a reason and stays in the outstanding count.
Disagreement remains visible; it is not silently converted into reviewer approval.
There is no automatic gate on building, sealing or publishing an article.

Repeat packet → record → respond → advance for argument, challenge and
verification. Each packet includes earlier discussion so the reviewer can follow
the reasoning instead of repeating already answered objections. After the fourth
reflection the session is complete, including any expressly deferred questions.

## Revisions and history

```sh
bln review show 1 --json
bln review list --document YOUR_ORG_ROAM_ID --limit 20
```

`show` compares the current source bytes with the captured hash. It reports
`current`, `stale`, or `unavailable`. A completed review can still be stale.
`list` describes saved sessions without reopening every article; use `show` to
check a particular revision.

Record the discussion and planned changes against the reviewed draft. Once you
edit or move the source, the historical review remains readable but cannot accept
new findings, responses or advancement. Start another session for the revised
article. Its independent snapshot prevents old quotes and decisions from silently
being treated as review of new text. Restoring the exact source bytes restores
`current` status. Review freshness covers that one saved source file, not unsaved
buffers, included files, attachments, linked code, or an external source changing.

The editorial brief has independent `current`, `stale` or `unavailable` file
status in `show`. Changing or deleting that file does not prevent continuing a
review of the unchanged article: packets keep using the captured text. Start a
new review to adopt revised writing principles. Stored brief text and its hash
are validated during integrity checks and backup/restore without requiring the
original file to survive.

## Storage and limits

Schema version 6 adds `article_reviews` and `article_review_events` to the existing
project-local SQLite store. Activation and upgrades are explicit through
`bln state migrate`; review never falls back to JSON. Snapshots are immutable and
events append-only, with hashes, validated transitions and transactional insertion.
Mutations share the project lock without invoking website-output recovery.

Reviews with an editorial brief use snapshot, packet and status format version 2.
Reviews without one retain version 1; existing captures are read without being
rewritten. Report templates remain version 1, bound to the review ID and article
hash. This addition does not change SQLite schema version 6 or require another
database migration.

Lifecycle integrity checks, backup and restore validate the saved review history
without requiring the source files still to exist. A database backup contains the
captured text, not linked files or the live editable original.

Limits are 1 MiB per article source, 64 KiB per editorial brief, 256 KiB per
imported report/event, 16 KiB per ordinary text field, five findings per step,
three evidence records per finding and 256 events per review. JSON input rejects
unknown fields. Wrong review IDs, revisions, steps, ranges or quotes are rejected
without appending an event.

**Privacy:** the database and exported packets contain full draft text, source
paths, captured editorial briefs and private discussion, potentially including
Org material marked `noexport`. Keep them outside website output and public
repositories. Packets are explicit exports to stdout; they do not become website
resources. No draft or review is copied into a build catalog, receipt or sealed
publication by this feature.
