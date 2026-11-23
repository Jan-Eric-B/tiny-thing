# tiny-thing

## Git attribution (project exception)

In this repository only, commits made by Claude carry a `Co-Authored-By: Claude <noreply@anthropic.com>` trailer. This deliberately overrides the global rule against crediting AI agents; it applies to commit messages, not to PR text, tags or release notes. Commits use the GitHub noreply address of the Jan-Eric-B account (repo-local git config), never a private email address.

## Language

Use the terms in [GLOSSARY.md](GLOSSARY.md). Decisions and their reasons are in [docs/adr/](docs/adr/).

## Changelog

Every user-visible change gets an entry under `## [Unreleased]` in [CHANGELOG.md](CHANGELOG.md), following [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/) (Added, Changed, Deprecated, Removed, Fixed, Security; dates as YYYY-MM-DD). Versions follow Semantic Versioning; on release the Unreleased entries move into a new version section with a compare link at the bottom.

## Agent skills

### Issue tracker

Issues and specs are local Markdown files under `.scratch/<feature-slug>/`. See `docs/agents/issue-tracker.md`.

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.
