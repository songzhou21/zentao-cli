---
name: zentao
description: Query and view Zentao bugs — filter by title, assignee, resolver, date, module, affected/resolved build, or status; view details and analyze bugs.
---

# Zentao Bug Workflow

Zentao (禅道) is a project management and bug tracking platform. Use the `zentao` CLI to query it. **Agents should always add `--json` (or `--json=fields`) for structured output** — do not rely on human-readable tables (`bug view` defaults to JSON; `--json` only trims fields).

Entry points: `bug view <ID|URL>` / `bug list` / `bug stats` / `bug candidates --build`.

## List

```bash
zentao bug list --json=id,title,state,assignee,url
zentao bug list --title 会议 --json=id,title,state,assignee,url
zentao bug list -a zhousong -s active -L 100 --json=id,title,state,assignee
zentao bug list --opened-by chenjie --opened-by niuweilong -s active --json=id,title,state,openedBy,assignee,url
zentao bug list --module 1144 -s active -s resolved --json=id,title,state,assignee,url
zentao bug list --resolved-by zhousong --resolved-from 2026-07-01 --resolved-to 2026-07-31 -s all --json
```

- `--title` / `--opened-by` / `-s` each accept up to 3 values (OR). `--opened-by` takes user **account names**, not display names. `-s all` cannot combine with other states.
- `--opened-build` / `--resolved-build`: numbers are used as build IDs directly; strings are matched by unique substring against candidates — on multiple matches, use `bug candidates --build` to find the `value`.
- `-s` (state): `active` (default) / `resolved` / `closed` / `all`. Repeat for OR, e.g. `-s active -s resolved`. Must be set explicitly when querying resolved/closed bugs or date ranges.
- Two search groups, 3 slots each; repeating two of `--title` / `--opened-by` / `-s` fills both groups. The default `active` state occupies one slot. Use `--state all` to free it when slots are full.
- `--json` requires `=` for field selection: `--json=id,title`; bare `--json` = all fields.
- Fields: `id` `title` `state` `severity` `priority` `confirmed` `openedBy` `openedDate` `assignee` `resolvedBy` `resolvedDate` `resolution` `deadline` `url`.
- Dates are full timestamps (e.g. `2026-08-20 11:30:31`). People fields are display names (not account names).

## Candidates

```bash
zentao bug candidates --build
zentao bug candidates --build 会议5.1
zentao bug candidates --build --json=value,name
zentao bug candidates --module
```

`--build` (builds) / `--module` (modules). Defaults to a human table; `--json[=value,name]` for JSON. `value` is the candidate ID (used with `--opened-build` / `--resolved-build` / `--module`), `name` is the display name. Append a keyword to filter by name.

## Stats

```bash
zentao bug stats --title 会议优化5.1 --json=assignee,active,solved,closed,total
```

- Main table: assignee / active / solved / closed / total; pending-verification is a separate table. Active counts by current assignee; solved/closed count by resolver.
- Defaults to `--state all`, `-L 1000`. When the limit is hit, `incomplete` is `true`. Accepts the same filters as `bug list`.
- JSON: `rows` contains `assignee,active,solved,closed,total`; `pending.rows` contains `assignee,resolved`.

## View

```bash
zentao bug view 57801
zentao bug view 57801 --json=id,title,state,assignee,description,history,images,attachments,url
zentao bug view http://example/zentao/bug-view-57801.html --json=id,title,history
```

- Fields: `id` `title` `priority` `state` `openedBy` `openedDate` `assignee` `resolvedBy` `resolvedDate` `resolvedBuild` `description` `history` `images` `attachments` `url`.
- `description` and `history[].comment` are stripped HTML. `images` are collected from `<img src>` in HTML. `history` is an array of `at` `action` `actor`, optionally `assignee` `changes` `comment`.
- Use `--raw-json` for the raw API response (mutually exclusive with `--json`).

## Debug (only when user asks to analyze/fix a bug)

Download `images` / `attachments[].url` with curl, using `-o` with the last URL segment. Combine description, images, and logs — don't rely on title alone.

```bash
curl -L -o /tmp/bug-<id>/file-read-73844.png '<url>'
```

