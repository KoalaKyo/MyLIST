# MyLIST Windows client

This directory contains the production MyLIST desktop client and its custom installer shell.

## Development

    npm install
    npm run dev

Frontend production check:

    npm run build

Rust tests:

    cargo test --manifest-path src-tauri/Cargo.toml

## Packaging

Do not package directly from this directory. Use the repository-level controlled entrypoint:

    powershell -NoProfile -ExecutionPolicy Bypass -File ..\..\tools\mylist-build.ps1 -Mode Test
    powershell -NoProfile -ExecutionPolicy Bypass -File ..\..\tools\mylist-build.ps1 -Mode Release -Version <version>

Test mode produces an executable for functional validation. Release mode produces the formal installer with the custom MyLIST install and uninstall UI, source snapshot, checksums, build metadata, and version archive.
