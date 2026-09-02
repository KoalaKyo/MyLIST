---
name: mylist
description: Invoke whenever the prompt contains the standalone word mylist in any capitalization. Save concrete responsibilities to MyLIST through its MCP tools, never as Codex reminders.
---

# MyLIST task capture

## Invocation and destination

The standalone trigger word `mylist` is case-insensitive. When it appears anywhere in the user's prompt, treat it as an explicit request to use MyLIST.

Remove the trigger from task text. Never create a Codex reminder, scheduled task, automation, heartbeat, cron job, or reminder card. If MyLIST is unavailable, report that nothing was saved; never substitute another reminder system.

## Capture rule

Create a task when the prompt expresses a concrete matter the user wants to record, do, remember, coordinate, supervise, receive, or follow up.

Time is optional after the user explicitly invokes MyLIST:

- When a reliable date, time, deadline, or recurrence is provided, preserve it.
- When no time is provided, create the task without a due time. Do not ask for a time merely because it is absent.
- Ask one short question only when the requested matter itself is too ambiguous to form a useful task.

The executor may be the user or another person. Capture actions by a child, family member, subordinate, colleague, vendor, or other person when the user wants to track the outcome. Preserve the executor in the title; use a follow-up title when appropriate, such as `Follow up on Alex's report submission`.

Do not capture examples, hypothetical scenarios, casual discussion, completed work, or unrelated people's activities unless the user clearly asks to record them.

## Task fields

- Write a concise, independently understandable action title.
- Put only useful context, expected outcome, dependencies, and acceptance criteria in the note.
- Read existing categories and select the closest one. Do not create a category automatically.
- Preserve explicit dates and times. For a date without a time, use 18:00 in the user's local timezone. Resolve unambiguous relative dates from the current local date.
- For recurring work, set the stated interval and unit. Use `update_due` when one continuing checklist item should advance; use `create_new` when every occurrence should remain as a separate record.
- Split multiple matters only when each can be completed independently; otherwise keep one task and structure its note.

## MCP workflow

Confirm service availability with `mylist_get_overview`, then read categories with `mylist_list_categories`. Use `protocolVersion: "mylist.mcp.v1"` and a fresh `requestId` for each write. Create with `mylist_create_task` and briefly report the title and due time, or state that it was saved without a due time.

Before creating, check current unfinished tasks when practical. Do not duplicate an item with the same responsibility and due time. If a matching task exists, report that it is already recorded.

For updates, read the existing task and preserve its current revision. Deletion, category deletion, imports that replace data, and exports always require MyLIST's visible local confirmation flow.
