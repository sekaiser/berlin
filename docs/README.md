# Berlin documentation

## Get started

- [Project overview and quick start](../README.md)
- [Local publishing projects](local-publishing.md)
- [Unified publications and the pipeline DSL](unified-publications.md)
- [Migration to unified publications](unified-publications-migration.md)

## Write and organize content

- [Emacs and Org authoring](emacs-authoring.md)
- [Authoring checks](authoring-check.md)
- [Document references and backlinks](document-references.md)
- [Stable URLs](stable-urls.md)
- [Topic guides](topic-guides.md)
- [Code listings and references](code-listings.md)
- [Mathematics](mathematics.md)
- [LaTeX diagrams](diagrams.md)
- [LaTeX papers](papers.md)
- [Article reviews](article-review.md)

## Compose and present publications

- [Composable publications](composable-publications.md)
- [Component contracts](component-contracts.md)
- [Rust builders and the view macro](view-macro.md)
- [Tera components](tera-components.md)
- [Text and LinkedIn drafts](publication-text.md)
- [Notebook theme](../themes/notebook/README.md)
- [Notebook search](notebook-search.md)
- [Article comments](article-comments.md)
- [RSS subscriptions](rss.md)

## Build, release, and track publications

- [Build caching](build-caching.md)
- [Website releases](website-releases.md)
- [Release announcements](announcement-workflow.md)
- [Lifecycle storage](lifecycle-state.md)
- [Metrics storage](metrics.md)

## Architecture

- [Architecture and system boundaries](../ARCHITECTURE.md)
- [Decision: composable publications](decisions/composable-publications.md)

## Contributing documentation

Version documentation that helps people use, understand, or maintain Berlin:
guides, supported contracts, migration instructions, and lasting design decisions.
Keep examples reproducible with synthetic inputs.

Keep implementation plans, agent prompts and handoffs, session notes, raw test
logs, screenshots from individual validation runs, and review campaign records
in the ignored `/.agent-work/` directory. They do not belong in `docs/` or commits.
Turn any lasting conclusion into a maintained guide or design decision. Keep
runnable regression tests and their synthetic fixtures versioned alongside the
code; those are part of the project, rather than records of a particular run.
