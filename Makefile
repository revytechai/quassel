REPO_PURPOSE = fork
UPSTREAM_BUILD = follows upstream's build
JSON = 0

.PHONY: help purpose preflight

help:
	@printf "%s\n" "purpose: $(REPO_PURPOSE)"
	@printf "%s\n" "Usage: make [help|purpose|preflight]"
	@printf "%s\n" "Quassel IRC fork. Follows upstream's build with CMake."

purpose:
	@printf "%s\n" "$(REPO_PURPOSE)"

preflight:
	@printf "%s\n" "preflight --json --purpose $(REPO_PURPOSE)"
