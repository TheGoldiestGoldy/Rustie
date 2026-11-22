# Source publication and release checklist

Source publication and a shippable release are different milestones. The source/documentation update does not change the Beta 1 NO-GO decision.

- Preserve the existing GitHub history; publish only from an isolated reviewed snapshot.
- Wait for active implementation work before freezing the combined source.
- Record source provenance and run formatting, workspace tests, strict Clippy and a matched UI/worker build.
- Validate Markdown/image links and inspect screenshots at full size.
- Exclude private recordings, project documents, proprietary banks/plug-ins, caches, credentials and raw system logs.
- Keep dated evidence and current summaries distinct; report incomplete listening/hardware gates.
- Use the existing repository licenses; complete contributor-rights and dependency notices before distributing a release.
- Verify the published branch SHA and tree after upload. Do not call a local commit a successful publication.

Current product state: [status](PROJECT_STATUS.md). Release criteria: [Beta 1 decision](BETA1_RELEASE_DECISION.md) and [v1 roadmap](ROADMAP.md).
