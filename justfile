default: build

build:
    cargo build --locked -p peritus-cli -p peritus-daemon -p peritus-tui -p peritus-web

package:
    cargo xtask product-package

install:
    cargo xtask product-install

# Behavioral testing is performed through the real-testing skill.

# Optional source-bound sanitizer and mutation campaigns. Evidence folders must be new.
testing-setup output:
    python3 discovery/checks.py setup --output {{quote(output)}}

fuzz output seconds="60":
    python3 discovery/checks.py fuzz --output {{quote(output)}} --seconds {{quote(seconds)}}

mutants output shard="0/64":
    python3 discovery/mutation/run.py run --output {{quote(output)}} --shard {{quote(shard)}}

testing-inventory output:
    python3 discovery/coverage.py --output {{quote(output)}}
