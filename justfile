# Display available recipes
default:
    just --list --unsorted

# Install dependencies and set up the development environment
bootstrap:
    cargo build

alias fmt := format

# Format code and project files
format:
    just --fmt
    dprint fmt
    cargo fmt --all
    fd -e nix -X nixfmt
    # The trailing `.` is required: with no path, ripgrep reads stdin when
    # stdin is not a TTY and blocks forever instead of searching the tree.
    rg -l '[^\n]\z' --multiline . | xargs -r sed -i -e '$a\\'

# Run linters and static analysis
check:
    just --fmt --check
    dprint check
    @fd -e md -X awk '/^[[:space:]]*\|/ && length > 80 {print FILENAME ":" FNR ": table row is " length " chars (>80)"; bad=1} END {exit bad}'
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    fd -e nix -X nixfmt --check
    ! rg -l '[^\n]\z' --multiline .

# Run the test suite
test:
    cargo test --workspace --all-targets

# Build the release library
build:
    cargo build --release

# Run all benchmarks, optimized for the host CPU
bench *args:
    RUSTFLAGS="-C target-cpu=native" cargo bench -- {{ args }}

# Run the synthetic workload experiment, optimized for the host CPU
bench-synthetic *args:
    RUSTFLAGS="-C target-cpu=native" cargo bench --bench synthetic -- {{ args }}

# Replay the QuantCup trace, optimized for the host CPU
bench-quantcup *args:
    RUSTFLAGS="-C target-cpu=native" cargo bench --bench quantcup -- {{ args }}

# Run focused operation and hasher experiments
bench-operations *args:
    RUSTFLAGS="-C target-cpu=native" cargo bench --bench operations -- {{ args }}
