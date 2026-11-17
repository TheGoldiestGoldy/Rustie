# Legal, licensing and asset hygiene

**Working policy, not legal advice.** Review licensing and trademark issues with appropriate experts before public release.

## Interaction design

- Common patterns such as tracks, transport, clip grids, a piano roll and a searchable browser are broad DAW interaction concepts.
- Build original pixel layout, graphics, type choices, names, behaviors and implementation. **Do not copy exact Ableton screens, icons, branding, manuals, proprietary code or sound assets**.
- Never claim Ableton affiliation, certification, flawless compatibility or format support without verification.
- Keep a source register for every external code snippet, dependency, icon, photograph, preset or sample.

## Source licenses

Workspace manifests currently declare **`MIT OR Apache-2.0`**, but the publication snapshot includes matching top-level license texts. That metadata is not a substitute for a complete rights review and required notices. Before distributing binaries, confirm all contributor permissions and ship full compatible license texts, attribution and THIRD_PARTY_NOTICES as needed.

The intended license is not a blanket permission to redistribute materials developed by other agents, nor any commercial VST/plugin/sample assets. Contributor rights must be explicit.

## Plugins and formats

- **CLAP:** investigate SDK terms, host boundary and extension versions; only load plugins installed legally by the end user.
- **VST3:** review the applicable Steinberg SDK/license version before distributing a host; never bundle SDK-derived files without permission.
- **LV2:** review per-component licensing and platform bundle layout.
- **VST2:** do not assume distribution/SDK authorization. Deferred pending legal review.
- Ableton `.als`: interoperability should be based on independently observed document behavior and legally obtained test files, with clear limitations. Do not embed commercial projects.
- Avoid trademark terms in the product name, website and metadata in ways that imply affiliation.

## Privacy and secrets

Before any push, scan staged diff and Git history (including binary blobs) for passwords/API keys, local paths, SSH data, personal audio, business/customer info, IP addresses, machine usernames, session files, licensed sample packs and recordings. A private repository still needs this review.

Examples of forbidden tracked material: `.env`, `*.pem`, `*.key`, access tokens, `~/.config` state, full Linux journal logs, private audio projects, user-installed plugin bundles, sample libraries, `target/`.

Don't place a document listing the exact private details you removed into the repository. Keep audit notes generic and non-identifying.

## External source register template

| Component | Origin URL | License and version | Files/changes | Attribution | Verified by |
| --- | --- | --- | --- | --- | --- |
| *None recorded yet* | | | | | |

Add an entry whenever new third-party source, art or sample material is introduced.
