---
name: mylist-maintenance
description: Maintain MyLIST when changing code, testing, packaging, releasing, archiving versions, checking retained releases, handling temporary review files, or preparing a rollback. Use automatically inside the MyLIST repository.
---

# MyLIST maintenance

Treat the user as the product owner, not the code maintainer. Codex owns the technical maintenance workflow and discloses product-relevant results in plain language.

## Workspace

- The only MyLIST workspace is E:\Codex\Todo_List.
- Keep source in the existing project folders, formal archives under releases, reusable maintenance tools under tools, and all disposable review, screenshot, extraction, cache, and intermediate files under temp.
- Do not create MyLIST files in the conversation working directory, OneDrive\文档\服务器监控, the desktop, or another dated Codex folder.
- Before writing, resolve the destination and verify that it is inside E:\Codex\Todo_List.
- The temp directory is disposable. Never store the only copy of source code, a formal installer, a release manifest, or user data there.
- Do not delete or move legacy files outside the project without first showing the user an inventory and receiving authorization.

## Maintenance rules

- Preserve unrelated user changes. Never discard, reset, or clean files to make a build pass.
- Inspect the current branch, changed files, current version, and relevant build scripts before changing code.
- Keep one logical change per commit when practical. Codex performs Git operations; do not require the user to learn Git.
- Do not claim completion without proportionate verification.
- Follow this skill and tools\mylist-build.ps1 instead of relying on conversation memory.

## Build routing

Use only:

    powershell -NoProfile -ExecutionPolicy Bypass -File tools\mylist-build.ps1 -Mode Test
    powershell -NoProfile -ExecutionPolicy Bypass -File tools\mylist-build.ps1 -Mode Release -Version x.y.z
    powershell -NoProfile -ExecutionPolicy Bypass -File tools\mylist-build.ps1 -Mode List
    powershell -NoProfile -ExecutionPolicy Bypass -File tools\mylist-build.ps1 -Mode CleanTemp -RetentionDays 30

- Testing a feature or asking for an EXE means Test. State that it is not an installer.
- Asking to package, generate an installation package, or create a formal build means Release. It must contain the custom MyLIST install and uninstall UI.
- Asking which versions are retained or how much space they use means List.
- CleanTemp without ConfirmCleanup is a preview. Use ConfirmCleanup only after disclosing the exact candidates and receiving user approval.
- Never substitute lower-level Tauri, NSIS, installer-shell, or ad-hoc copy commands for a user-facing artifact.
- Formal filenames use MyLIST_x.y.z_x64-setup.exe. Do not add final, new, fixed, date, or other suffixes.
- Test builds are delivered from app\windows-client\artifacts as MyLIST-test.exe.
- Formal installers are delivered and retained only under releases\major.minor\x.y.z as MyLIST_x.y.z_x64-setup.exe. Do not leave duplicate formal installers in app\windows-client\artifacts.
- Keep the generated source ZIP in the local release archive for rollback, but never upload that custom source ZIP as a GitHub Release asset. GitHub's automatic source-code links are platform-managed and remain visible.

## Formal release gate

Before Release:

1. Confirm the requested version and ensure all app and installer version files agree.
2. Confirm the intended changes are committed and the working tree is clean. Never hide uncommitted changes.
3. Run checks proportional to the changes. Include install, uninstall, path handling, data preservation, localization, MCP, and Skill checks when affected.
4. A release succeeds only when the custom UI installer, committed-source archive, SHA-256 file, build information, Git tag, and version index exist.
5. Never report only that the build succeeded.

## Retention and rollback

- Retain the latest patch for every major.minor line. A successful 1.2.3 replaces 1.2.2, while the latest 1.1.x and 2.0.x remain.
- Remove an older patch archive only after the newer archive and checksum are verified.
- Never delete another major.minor line automatically.
- Start rollback read-only. Report the current version, requested version, archive availability, database compatibility risk, and proposed changes before execution.
- Program rollback must not change user task data unless separately authorized.

## Required disclosure

After maintenance, tell the user:

- what changed;
- what was verified and what was not;
- whether the artifact is a test EXE or formal installer;
- version, exact filename, size, SHA-256, and the formal installer path under releases;
- Git commit and tag for a release;
- retained versions, removed superseded patches, and archive space;
- temporary files created and whether they remain;
- unresolved risk or manual validation still needed.

If a release gate fails, identify the gate and confirm that the previous formal archive remains intact.
