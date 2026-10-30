.PHONY: help build test clippy fmt release-test installer-test powershell-test workflow-test docs-test public-acceptance-test public-acceptance check

help:
	@printf '%s\n' \
		'make build             Build the mpaka executable' \
		'make test              Run CLI tests with nextest' \
		'make clippy            Run Clippy with warnings denied' \
		'make fmt               Check formatting' \
		'make release-test      Test release packaging and signing' \
		'make installer-test    Test the POSIX installer' \
		'make powershell-test   Test the PowerShell installer' \
		'make workflow-test     Check the release workflow contract' \
		'make docs-test         Check the CLI documentation contract' \
		'make public-acceptance-test Test the public topology assertions' \
		'make public-acceptance Verify the public distribution endpoint' \
		'make check             Run every local check'

build:
	cargo build --bin mpaka

test:
	cargo nextest run

clippy:
	cargo clippy --all-targets -- -D warnings

fmt:
	cargo fmt --all -- --check

release-test:
	bash scripts/cli-release-test.sh
	bash -n scripts/cli-release.sh scripts/cli-release-test.sh
	shellcheck scripts/cli-release.sh scripts/cli-release-test.sh

installer-test:
	bash scripts/installer-test.sh
	bash -n install scripts/installer-test.sh
	shellcheck install scripts/installer-test.sh

powershell-test:
	bash scripts/installer-test-static.sh
	if command -v pwsh >/dev/null 2>&1; then pwsh -NoProfile -File scripts/installer-test.ps1; else echo 'pwsh unavailable; static PowerShell contract checks passed'; fi

workflow-test:
	bash scripts/cli-release-workflow-test.sh

docs-test:
	bash scripts/cli-docs-test.sh

public-acceptance-test:
	bash scripts/cli-public-acceptance-test.sh

public-acceptance:
	bash scripts/cli-public-acceptance.sh

check: fmt test clippy release-test installer-test powershell-test workflow-test docs-test public-acceptance-test
