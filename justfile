set shell := ["pwsh.exe", "-c"]

run side *args:
    cargo run --release -- -i input -t tiles -k -m {{side}} {{args}}

run-debug side *args:
    cargo run -- -i input -t tiles -k -m {{side}} {{args}}
