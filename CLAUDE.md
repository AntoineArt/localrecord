# localrecord

Project rules live in `.cursor/rules/`, shared by every agent working on this
repo. Cursor loads them natively through their frontmatter mode; Claude Code
loads the always-on ones through the imports below, so there is a single source
of truth per topic.

## Always on

@.cursor/rules/release-pipeline.mdc
