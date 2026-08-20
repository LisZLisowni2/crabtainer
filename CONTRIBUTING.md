# Contributing to crabtainer

First off, thank you for considering contributing to **crabtainer**! Community contributions help make this project better for everyone.

As a solo maintainer project, following these guidelines helps me review and merge your pull requests as quickly as possible.

---

## How Can I Contribute?

### 1. Reporting Bugs
Before opening a bug report, please check existing issues to make sure it hasn't already been reported.

When creating a bug report, please include:
* **A clear title** describing the issue.
* **Steps to reproduce** the behavior.
* **Expected vs. actual behavior**.
* Your environment details (OS version, Rust version, Docker/OCI runtime version).

> **Note:** If you find a **security vulnerability**, please do **NOT** open a public issue. Follow our [Security Policy](SECURITY.md) to report it privately.

### 2. Suggesting Enhancements
Feature requests are always welcome! When proposing a feature:
* Provide a clear description of the feature and why it would be useful.
* Detail how it should work or provide example syntax/commands if applicable.

### 3. Pull Requests (PRs)
1. **Fork the repo** and create your branch from `main`:

```bash
git checkout -b feature/my-cool-feature
```
or
```bash
git switch -c feature/my-cool-feature
```

2. Make your changes and write clean, readable Rust code.
3. Run checks and tests locally before submitting (see Local Development below).
4. Commit your changes with a clear commit message.
5. Open a Pull Request against the main branch with a description of what your changes do.
   
### Local Development

Make sure you have Rust (toolchain 1.70+ recommended) and cargo installed.
#### Build the Project

```bash
cargo build
```

#### Run Tests

Please ensure all existing and new tests pass before opening a PR:
```bash
cargo test
```

#### Code Style & Formatting

This project uses standard Rust formatting and linting tools. Please format your code before pushing:
```bash
# Format code
cargo fmt --check

# Run linter
cargo clippy -- -D warnings
```

#### Code Style Guidelines

Keep PRs focused—try to keep individual pull requests targeted to a single issue or feature.
Write unit tests for new functionality where reasonable.
Document new public functions or complex logic.

## AI Usage Policy

In short, it is acceptable to use AI for this codebase, but careless vibe coding is strictly prohibited.
Every line you write and submit is your responsibility. You must understand how your code works and
why it was written that way. You also possibly have to be able to explain
or refactor it during code review. All AI-generated code must be tested, debugged and validated before opening a 
pull request due to risk of hallucinations, outdated libraries or patterns and subtle bugs.
I (LisZLisowni2) encourage using AI as an assistant that helps in coding, but can not be a substitute for software engineering.
If you are unsure, please ask!

## Need Help?

If you have questions or aren't sure where to start, feel free to open a Q&A thread in GitHub Discussions or comment on an open issue!
