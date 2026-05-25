# ADR 0005: Secure Desktop-Extension IPC over Native Messaging

## Status

Accepted

## Context

The browser extension needs vault access without storing master keys or decrypted vaults. Browser pages are untrusted, and content scripts are exposed to hostile DOMs.

## Decision

Use browser native messaging between the extension background context and a desktop-installed ESPASS native messaging host. Pin extension origins, require a handshake, establish ephemeral signed IPC sessions, validate every message schema, and enforce replay counters and correlation IDs.

## Consequences

Native messaging keeps vault operations inside the desktop trust boundary and uses browser-supported installation manifests. It adds platform-specific host registration and requires exact extension ID management for Chromium and Firefox releases.

