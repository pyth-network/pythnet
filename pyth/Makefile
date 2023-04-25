# This Makefile contains helpers for testing PythNet modifications.

VERSION = $(shell git describe --tags --dirty)


.PHONY: help
# Generate a help message for all targets in this Makefile.
help:
	@echo
	@echo "Available Helpers for PythNet ($(VERSION)):"
	@echo
	@awk '/^[a-zA-Z\-_0-9%:\\]+/ { \
		helpMessage = match(lastLine, /^# (.*)/); \
		if (helpMessage) { \
			helpCommand = substr($$1, 0, index($$1, ":")); \
			helpMessage = substr(lastLine, RSTART + 2, RLENGTH); \
			printf "  \033[36m%-30s\033[0m %s\n", helpCommand, helpMessage; \
		} \
	} \
	{ lastLine = $$0 }' $(MAKEFILE_LIST)
	@echo


.PHONY: mirror
# Download all known PythNet accounts to `accounts/`
mirror:
	@echo "Mirroring PythNet to Local"
	@mkdir -p accounts/
	@if [ -n "$(shell ls accounts/)" ]; then \
		echo "Error: accounts/ is not empty. Are you sure you want to do this?"; \
		exit 1; \
	fi
	@cargo run --example generate_pyth_data


# We want to run ../target/debug/solana-test-validator with the accounts under
# `accounts/` as arguments, the files are named after the addresses they should
# belong in so we can do a transformation to the following:
#
# Given:
#
#     accounts/
#     ├─ 6KUaSr9dsEkM9pTRQPWuWvVPvyWbcTPALHyetkV6r2R.json
#     ├─ 8apEP8G5ZThPSXsff11nMoBmwo3nU176EEFZHM6jzLox.json
#     └─ ...
#
# We want to run:
#
#     ../target/debug/solana-test-validator \
#         --account 6KUaSr9dsEkM... accounts/6KUaSr9dsEkM...json \
#         --account 8apEP8G5ZThP... accounts/8apEP8G5ZThP...json \
#         ...
#
.PHONY: validator
# Run a local PythNet validator loaded with all accounts under `accounts/`
validator:
	@echo "Running PythNet Validator"
	@if [ ! -f ../target/debug/solana-test-validator ]; then \
		echo "Error: ../target/debug/solana-test-validator does not exist. Maybe cargo build?"; \
		exit 1; \
	fi
	../target/debug/solana-test-validator $(foreach addr,$(wildcard accounts/*.json),--account $(basename $(notdir $(addr))) $(addr)) \
		--geyser-plugin-config pyth-geyser.json


.PHONY: clean
# Remove all accounts under `accounts/`
clean:
	@echo "Cleaning PythNet Accounts"
	@rm -rf accounts/
