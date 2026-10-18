# QuotaVista

See your AI subscription quotas, prepaid balances, and usage in one place.

QuotaVista is a planned desktop application for macOS and Linux. The first integrations target Kimi's shared membership quota and OpenRouter prepaid accounts, with room for additional providers and future Windows support.

## Planned features

- An overview of remaining quotas, balances, reset times, and connection health.
- Automatic and manual refresh, with low-quota, low-balance, and expiration alerts.
- Daily, weekly, and custom-range views of token usage, plan consumption, and trends.
- Local credential and usage storage, without a QuotaVista cloud account.
- English and Simplified Chinese interfaces in the first release.

Providers expose different metrics and levels of detail. Token availability, historical coverage, and automated access to Kimi's shared membership quota still require validation. Missing data will be identified explicitly.

## Project status

QuotaVista is in the architecture design phase. There is no runnable application yet.

The proposed stack is Tauri 2, React, TypeScript, Rust, and SQLite. Platform compatibility and provider capabilities will be validated during prototyping and integration.

The confirmed Linux target is Ubuntu 26.04 with GNOME on x86_64.

- [Requirements](docs/requirements.md)
- [Architecture](docs/architecture.md)
- [Development plan](docs/development-plan.md)
- [Requirements review](docs/reviews/requirements-review.md)
- [Architecture review](docs/reviews/architecture-review.md)
- [Acceptance traceability](docs/traceability.md)

## Contributing

Discussions about use cases and requirements are welcome in GitHub Issues. All project documentation is written in English for a global audience. See [CONTRIBUTING.md](CONTRIBUTING.md).

Never include API keys, session tokens, cookies, or logs containing credentials in an issue or pull request.

## License

QuotaVista is licensed under the [MIT License](LICENSE).

QuotaVista is an independent project and is not affiliated with the model service providers it supports.
