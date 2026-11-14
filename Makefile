.PHONY: fmt clippy test check run docker-build local-ci

fmt:
	cargo fmt --all -- --check

clippy:
	cargo clippy --workspace --all-targets -- -D warnings
	cargo clippy --workspace --lib --bins -- -D warnings -D clippy::unwrap_used -D clippy::expect_used

test:
	cargo test --workspace

check: fmt clippy test

run:
	cargo run --bin iris-server

docker-build:
	docker build -t iris:local .

local-ci:
	powershell -ExecutionPolicy Bypass -File ./local-ci.ps1
