default: build

build:
    cargo build --locked -p peritus-cli -p peritus-daemon -p peritus-tui -p peritus-web

package:
    cargo xtask product-package

install:
    cargo xtask product-install

# Behavioral testing is performed through the real-testing skill.
