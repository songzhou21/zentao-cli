# Use a resource-oriented Bug command interface

Version 0.2.0 exposes the existing Bug capabilities as `zentao bug list` and `zentao bug view`; the former top-level `search` command and `bug show` are removed without compatibility aliases. The shape borrows the list/view information architecture of `gh issue`, but keeps Zentao terms and only exposes parameters backed by verified Zentao behavior.

## Consequences

- `bug view` accepts a Bug ID when a Site is configured, or a complete Bug URL that supplies its own Site.
- Bug list queries require a Product scope rather than relying on a hard-coded product ID.
- This is intentionally a breaking 0.2.0 release; migration guidance belongs in user-facing documentation, not in compatibility code.
