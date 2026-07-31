# Default to Chrome but make password login immediately usable

Chrome remains the default Cookie Source on macOS, while `zentao auth login` writes the supported local Cookie file and switches the configured source to `file` after a successful login. This preserves Chrome-first day-to-day use without leaving a newly created password session unusable.

## Consequences

- `auth status` masks Cookie values by default and only reveals them through an explicit opt-in flag.
- Passwords are never accepted as command-line arguments; interactive login prompts securely and automation uses standard input.
