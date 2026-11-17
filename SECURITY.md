# Security and responsible disclosure

Slate Studio is experimental native software. No production security guarantees, stable project schema or safe processing of arbitrary plugin binaries are claimed.

## Reporting

Please do not disclose unpatched vulnerabilities or sensitive files in public issues. Until a dedicated private disclosure channel is configured, contact the project maintainer via GitHub's private security reporting mechanism **if enabled**, or privately through an agreed channel. Do not paste secrets into issues.

## Untrusted inputs

- Treat audio files and project JSON as untrusted. Validate sizes, paths, IDs and allocation limits.
- Treat future XML/ALS parsing as hostile-input parsing; defend against oversized decompression, recursion and entity issues.
- Plugin binaries are executable third-party code. Listing an installed plugin must never automatically execute it.
- Audio-thread IPC and shared memory must validate sizes/versions and fail safely on worker crash.
- Never automatically reconfigure global audio devices, privileges or installed software to solve a project-local problem.

## Secrets and personal information

Do not commit `.env`, keys, tokens, private paths, recordings, downloaded plugins, or local service logs. Use secret scanning and inspect full Git history before turning a repo public. This applies even to agent-created documents.

Support for private vulnerability reporting can be configured in GitHub repository settings.
