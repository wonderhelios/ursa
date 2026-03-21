---
name: review
description: Review code for quality and issues
tags: code-quality, review
---

Perform a code review on the specified file or recent changes.

Steps:
1. If a file path is provided in context, read it with `read_file`
2. Otherwise, run `git diff HEAD~1` to see recent changes
3. Review for:
   - Logic errors or bugs
   - Missing error handling
   - Code clarity and naming
   - Performance issues
   - Security concerns
4. Provide a structured report with: Summary, Issues (severity: error/warning/suggestion), and Recommendations

Be specific and actionable. Reference line numbers where relevant.