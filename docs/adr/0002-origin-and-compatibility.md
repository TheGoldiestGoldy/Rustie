# ADR 0002: Original visual identity, clear compatibility claims

- **Status:** Adopted for project documentation
- **Date:** 2026-11-17

## Context

The intended workflow has dense music editing controls, searchable assets, an Arrangement View, and a performance-oriented Session grid. This overlaps conventions used by established DAWs.

## Decision

Use an original near-black, glass-inspired identity under the Rustie repository name and Slate Studio working UI name. Implement generic workflows from independent design and code. Do not copy exact assets, wording, control art, trademark branding, commercial sound sets, or source code from other DAWs.

Declare compatibility only after real tests. CLAP/VST3/LV2 discovery must not be confused with loading. `.als` import/export is not implemented and must not be advertised.

## Consequences

- UI specs prioritize workflow ergonomics and proof over visual cloning.
- Third-party dependencies require license and attribution checks.
- All public docs distinguish implemented, planned and prototype features.
- See `docs/UI_UX_SPEC.md` and `docs/LEGAL_AND_ASSETS.md`.
