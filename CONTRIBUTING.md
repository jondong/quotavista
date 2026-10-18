# Contributing to QuotaVista

QuotaVista has reviewed requirements and architecture and a Trellis-managed prototype backlog. There is no runnable application or supported application build procedure yet.

Read [task management](docs/task-management.md) and the [Phase 1 plan](docs/phase-one-plan.md) before implementation. Initialize your local Trellis identity, select an existing task, verify its prerequisites, and follow its acceptance criteria.

## Documentation language

Write all repository documentation in English so the project is accessible to a global audience. This applies to README files, requirements, architecture decisions, development plans, installation instructions, contributor guidance, and diagram labels.

Preserve official provider and product names. Keep examples and explanations understandable without access to private discussions. The first-release UI supports English and Simplified Chinese; documentation and task artifacts remain English.

## Proposing changes

- Use GitHub Issues to describe a use case, missing capability, or problem.
- Keep changes focused and update related documentation when scope or behavior changes.
- Identify assumptions and distinguish planned behavior from validated functionality.
- For provider work, document permissions, measurement units, historical coverage, and unsupported capabilities.
- Never include real keys, tokens, cookies, or identifying account data in issues, fixtures, logs, or commits.

## Validation

For documentation changes, check local links, code fences, diagram labels, and consistency with the requirements. Run git diff --check before submitting.

Once the application exists, follow the documented checks relevant to the change. Do not claim platform support from compilation alone or real provider integration from demo data.

## License

Original application and documentation contributions are made under the project's [MIT License](LICENSE). Retain notices required for third-party material and record its source. Changes to installed Trellis tooling must retain its [upstream terms](.trellis/NOTICE.md).
