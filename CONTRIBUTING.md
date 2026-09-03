# Contributing to MyLIST

Thank you for helping improve MyLIST.

## Before you start

- Search existing issues before opening a new one.
- Keep each change focused on one problem or feature.
- Discuss substantial product or architecture changes in an issue first.
- Do not include personal task data, credentials, generated build directories, or unrelated files.

## Development checks

From `app/windows-client`, build the frontend with:

```powershell
npm install
npm run build
```

Run the Rust tests with:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml
```

User-facing text must remain available in all eight supported languages. Changes to installation, uninstallation, MCP, or packaging must be verified through the repository's controlled build workflow.

## Pull requests

Explain what changed, why it changed, and how it was tested. Include screenshots for visible interface changes. By contributing, you agree that your contribution is licensed under GPL-3.0-only.
