---
name: commit
description: Review staged changes and create a git commit
tags: git, workflow
---

Review the currently staged git changes and create a well-formatted commit message.

Steps:
1. Run `git diff --staged` to see what's staged
2. Run `git status` to see the overall state
3. Write a commit message following this format:
   - First line: `<type>: <short summary>` (max 72 chars)
   - Types: feat, fix, refactor, docs, test, chore
   - Optional body: explain WHY, not what
4. Run `git commit -m "<message>"` to commit

Do not ask for confirmation - just do it. If nothing is staged, say so.