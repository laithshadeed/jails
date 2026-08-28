BASE_URL ?= http://localhost:8080
VUS ?= 10
DURATION ?= 30s

.PHONY: run
run:
	k6 run -e BASE_URL=$(BASE_URL) -e VUS=$(VUS) -e DURATION=$(DURATION) load-test.js
